//! 从原版 CC Switch 数据库一键导入供应商配置（路线 B：只读逐行导入）。
//!
//! 安全边界（实施计划 §3.1 P1 要求）：
//! - 源库**只读**打开（`mode=ro`），绝不写 `~/.cc-switch`
//! - 列级白名单映射：只导入两边都存在的列，schema 版本差异（v16→v21）不影响
//! - 合并策略：与现有 (id, app_type) 冲突的**跳过**；导入的卡片一律**不激活**
//! - 执行前自动备份本地库（复用 backup 基建），可回滚
//! - 范围：仅 providers 表；不导入用量日志等大表（原版也没有胜算云账号可导）

use crate::database::Database;
use crate::database::ImportedProviderRow;
use rusqlite::Connection;
use serde::Serialize;
use std::path::PathBuf;

/// 默认源路径：原版 CC Switch 的数据库
pub fn default_source_path() -> PathBuf {
    crate::config::get_home_dir()
        .join(".cc-switch")
        .join("cc-switch.db")
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub source_exists: bool,
    pub source_path: String,
    pub source_version: i32,
    pub total: i64,
    /// 各 app_type 的可导入数量
    pub per_app: Vec<(String, i64)>,
    /// 与现有卡片冲突（将被跳过）的数量
    pub conflicts: i64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: i64,
    pub skipped_existing: i64,
    pub skipped_invalid: i64,
    pub per_app: Vec<(String, i64)>,
    pub backup_file: Option<String>,
}

/// v16 与 v21 共有的 providers 列（按列名兼容映射）
const IMPORT_COLUMNS: &[&str] = &[
    "id",
    "app_type",
    "name",
    "settings_config",
    "website_url",
    "category",
    "created_at",
    "notes",
    "icon",
    "icon_color",
    "meta",
];

fn open_read_only(path: &PathBuf) -> Result<Connection, String> {
    if !path.exists() {
        return Err(format!("源数据库不存在：{}", path.display()));
    }
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("打开源数据库失败（只读）：{e}"))
}

fn providers_columns(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(providers)")
        .map_err(|e| e.to_string())?;
    let cols = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(cols)
}

fn app_type_allowed(app_type: &str) -> bool {
    use crate::app_config::AppType;
    use std::str::FromStr;
    AppType::from_str(app_type).is_ok()
}

/// 预览：只读扫描源库，统计可导入数量与冲突（不写任何数据）
pub fn preview(db: &Database, path: &PathBuf) -> Result<ImportPreview, String> {
    let conn = open_read_only(path)?;
    let cols = providers_columns(&conn)?;
    if cols.is_empty() {
        return Ok(ImportPreview {
            source_exists: false,
            source_path: path.display().to_string(),
            source_version: 0,
            total: 0,
            per_app: vec![],
            conflicts: 0,
        });
    }
    let source_version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0);

    if !cols.contains(&"id".to_string()) || !cols.contains(&"app_type".to_string()) {
        return Err("源库 providers 表缺少 id/app_type 列".into());
    }

    let mut stmt = conn
        .prepare(
            "SELECT app_type, COUNT(*) FROM providers
             WHERE COALESCE(name,'') != '' GROUP BY app_type ORDER BY app_type",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut per_app = Vec::new();
    let mut total = 0i64;
    for r in rows {
        let (app, n) = r.map_err(|e| e.to_string())?;
        if app_type_allowed(&app) {
            total += n;
            per_app.push((app, n));
        }
    }

    let existing = db.list_all_provider_keys()?;
    let mut c_stmt = conn
        .prepare("SELECT app_type, id FROM providers")
        .map_err(|e| e.to_string())?;
    let c_rows = c_stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut conflicts = 0i64;
    for r in c_rows {
        let (app, id) = r.map_err(|e| e.to_string())?;
        if existing.iter().any(|(ea, eid)| ea == &app && eid == &id) {
            conflicts += 1;
        }
    }

    Ok(ImportPreview {
        source_exists: true,
        source_path: path.display().to_string(),
        source_version,
        total,
        per_app,
        conflicts,
    })
}

/// 执行导入：备份本地库 → 逐行读取 → 跳过冲突 → 全部不激活、排序追加到尾部。
pub fn execute(db: &Database, path: &PathBuf) -> Result<ImportResult, String> {
    let conn = open_read_only(path)?;
    let cols = providers_columns(&conn)?;
    let usable: Vec<&str> = IMPORT_COLUMNS
        .iter()
        .copied()
        .filter(|c| cols.contains(&c.to_string()))
        .collect();
    if !usable.contains(&"id") || !usable.contains(&"app_type") || !usable.contains(&"name") {
        return Err("源库 providers 表缺少必要列（id/app_type/name）".into());
    }

    // 导入前备份本地库（可回滚）
    let backup_file = db
        .backup_database_file()
        .map_err(|e| e.to_string())?
        .map(|p| p.display().to_string());

    let select_sql = format!("SELECT {} FROM providers", usable.join(", "));
    let mut stmt = conn.prepare(&select_sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let get_s = |name: &str| -> Option<String> {
                let idx = usable.iter().position(|c| *c == name)?;
                row.get::<_, Option<String>>(idx).unwrap_or_default()
            };
            let get_i = |name: &str| -> Option<i64> {
                let idx = usable.iter().position(|c| *c == name)?;
                row.get::<_, Option<i64>>(idx).ok().flatten()
            };
            Ok(ImportedProviderRow {
                id: get_s("id").unwrap_or_default(),
                app_type: get_s("app_type").unwrap_or_default(),
                name: get_s("name").unwrap_or_default(),
                settings_config: get_s("settings_config").unwrap_or_default(),
                website_url: get_s("website_url").filter(|s| !s.is_empty()),
                category: get_s("category").filter(|s| !s.is_empty()),
                created_at: get_i("created_at"),
                notes: get_s("notes").filter(|s| !s.is_empty()),
                icon: get_s("icon").filter(|s| !s.is_empty()),
                icon_color: get_s("icon_color").filter(|s| !s.is_empty()),
                meta: {
                    let m = get_s("meta").unwrap_or_default();
                    Some(if m.is_empty() { "{}".to_string() } else { m })
                },
            })
        })
        .map_err(|e| e.to_string())?;

    let mut imported = 0i64;
    let mut skipped_existing = 0i64;
    let mut skipped_invalid = 0i64;
    let mut per_app: std::collections::BTreeMap<String, i64> = Default::default();

    for r in rows {
        let row = match r {
            Ok(row) => row,
            Err(_) => {
                skipped_invalid += 1;
                continue;
            }
        };
        if row.id.is_empty() || row.app_type.is_empty() || row.name.is_empty() {
            skipped_invalid += 1;
            continue;
        }
        if !app_type_allowed(&row.app_type) {
            skipped_invalid += 1;
            continue;
        }
        match db.insert_imported_provider(&row) {
            Ok(true) => {
                imported += 1;
                *per_app.entry(row.app_type).or_insert(0) += 1;
            }
            Ok(false) => skipped_existing += 1,
            Err(e) => {
                log::warn!("SSY 导入：写入失败 {}/{}: {e}", row.app_type, row.id);
                skipped_invalid += 1;
            }
        }
    }

    log::info!(
        "SSY: CC Switch 导入完成 — 导入 {imported}，跳过冲突 {skipped_existing}，无效 {skipped_invalid}"
    );
    Ok(ImportResult {
        imported,
        skipped_existing,
        skipped_invalid,
        per_app: per_app.into_iter().collect(),
        backup_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::lock_conn;
    use crate::error::AppError;
    use rusqlite::params;

    fn create_v16_style_source(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE providers (
                id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
                settings_config TEXT NOT NULL, website_url TEXT, category TEXT,
                created_at INTEGER, sort_index INTEGER, notes TEXT, icon TEXT,
                icon_color TEXT, meta TEXT NOT NULL DEFAULT '{}',
                is_current BOOLEAN NOT NULL DEFAULT 0, in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
                cost_multiplier REAL, provider_type TEXT,
                PRIMARY KEY (id, app_type)
            );
            INSERT INTO providers VALUES
              ('old-claude', 'claude', '旧中转', '{"env":{"ANTHROPIC_BASE_URL":"https://old.example"}}',
               'https://old.example', 'third_party', 100, 0, NULL, 'claude', NULL, '{}', 1, 0, NULL, NULL),
              ('old-codex', 'codex', '旧 Codex', '{"auth":{"OPENAI_API_KEY":"k"}}',
               NULL, NULL, 101, 1, '备注', NULL, NULL, '{}', 0, 0, NULL, NULL),
              ('bad', 'nosuchapp', '未知宿主', '{}', NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{}', 0, 0, NULL, NULL);
            PRAGMA user_version = 16;
            "#,
        )
        .unwrap();
    }

    #[test]
    fn preview_and_execute_roundtrip() -> Result<(), crate::error::AppError> {
        let db = Database::memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cc-switch.db");
        create_v16_style_source(&src);

        let pv = preview(&db, &src).unwrap();
        assert!(pv.source_exists);
        assert_eq!(pv.source_version, 16);
        assert_eq!(pv.total, 2); // 未知宿主不计入
        assert_eq!(pv.conflicts, 0);

        let result = execute(&db, &src).unwrap();
        assert_eq!(result.imported, 2);
        assert_eq!(result.skipped_invalid, 1); // 未知宿主
        assert!(result.per_app.contains(&("claude".into(), 1)));

        // 导入的卡片一律不激活
        let n_current: i64 = {
            use crate::database::lock_conn;
            let conn = lock_conn!(db.conn);
            conn.query_row(
                "SELECT COUNT(*) FROM providers WHERE is_current = 1 AND id LIKE 'old-%'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(n_current, 0);

        // 重复导入：全部按冲突跳过
        let again = execute(&db, &src).unwrap();
        assert_eq!(again.imported, 0);
        assert_eq!(again.skipped_existing, 2);
        Ok(())
    }

    #[test]
    fn conflicting_id_is_skipped_not_overwritten() -> Result<(), crate::error::AppError> {
        let db = Database::memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cc-switch.db");
        create_v16_style_source(&src);
        // 预置同 id 卡片，内容不同
        db.insert_imported_provider(&ImportedProviderRow {
            id: "old-claude".into(),
            app_type: "claude".into(),
            name: "我的同名卡".into(),
            settings_config: "{}".into(),
            website_url: None,
            category: None,
            created_at: None,
            notes: None,
            icon: None,
            icon_color: None,
            meta: None,
        })
        .unwrap();

        let result = execute(&db, &src).unwrap();
        assert_eq!(result.imported, 1); // 只有 codex 进来
        assert_eq!(result.skipped_existing, 1);
        let name = {
            use crate::database::lock_conn;
            let conn = lock_conn!(db.conn);
            conn.query_row(
                "SELECT name FROM providers WHERE app_type='claude' AND id='old-claude'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        assert_eq!(name, "我的同名卡"); // 未被覆盖
        Ok(())
    }

    #[test]
    fn missing_source_errors() {
        let db = Database::memory().unwrap();
        assert!(preview(&db, &PathBuf::from("/nonexistent/x.db")).is_err());
    }

    #[test]
    fn real_source_preview_if_present() {
        // 本机存在真实 ~/.cc-switch 时做只读冒烟（不存在则跳过）
        let src = default_source_path();
        if !src.exists() {
            return;
        }
        let db = Database::memory().unwrap();
        let pv = preview(&db, &src).unwrap();
        assert!(pv.source_exists);
        assert!(pv.source_version >= 1);
    }
}
