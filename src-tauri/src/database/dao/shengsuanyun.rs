//! 胜算云账号与 Provider 绑定数据访问对象。
//!
//! 只存元数据与余额缓存；API Key 永远在 OS Keychain（见 `shengsuanyun::credential_store`）。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use ssy_core::models::{AccountRecord as ShengsuanyunAccountRow, SsyUserInfo};

/// 充值/账单之外：绑定记录（壳层数据结构，含用户选中的上游 Key ID）
#[derive(Clone, Debug, serde::Serialize)]
pub struct ShengsuanyunBindingRow {
    pub app_type: String,
    pub provider_id: String,
    pub account_id: String,
    pub credential_source: String,
    pub key_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}
use rusqlite::params;

impl Database {
    /// 新增或按胜算云 uid 覆盖账号记录（同一上游账号重复登录时旧记录连同旧 Key 一起被替换）。
    /// 返回落库后的行。
    pub fn upsert_shengsuanyun_account(
        &self,
        id: &str,
        info: &SsyUserInfo,
        is_creator: bool,
        balance_assets: Option<f64>,
        voucher_assets: Option<f64>,
        now: i64,
    ) -> Result<ShengsuanyunAccountRow, String> {
        let conn = lock_conn!(self.conn);
        // 同 uid 旧记录直接删除（凭据侧由调用方保证）。
        // uid 为空时必须跳过：`uid = ''` 会匹配到**所有**历史空 uid 行，
        // 把别人的账号连同绑定一起删掉（2026-09-16 线上事故根因之一）。
        if !info.uid.is_empty() {
            conn.execute(
                "DELETE FROM shengsuanyun_accounts WHERE uid = ?1 AND id != ?2",
                params![info.uid, id],
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute(
            "INSERT OR REPLACE INTO shengsuanyun_accounts
             (id, uid, display_name, email, avatar_url, is_creator, balance_assets, voucher_assets, balance_updated_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                id,
                info.uid,
                info.display_name,
                info.email,
                info.avatar_url,
                is_creator as i64,
                balance_assets,
                voucher_assets,
                balance_assets.map(|_| now),
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(ShengsuanyunAccountRow {
            id: id.to_string(),
            uid: info.uid.clone(),
            display_name: info.display_name.clone(),
            email: info.email.clone(),
            avatar_url: info.avatar_url.clone(),
            is_creator,
            balance_assets,
            voucher_assets,
            balance_updated_at: balance_assets.map(|_| now),
            created_at: now,
            updated_at: now,
        })
    }

    /// 按胜算云 uid 反查本地账号 id。
    ///
    /// 重登时复用它，避免「删旧行 + 新 id」把绑定记录级联删掉
    /// （绑定被删会导致用户选中的 Key 丢失）。
    pub fn find_shengsuanyun_account_id_by_uid(&self, uid: &str) -> Result<Option<String>, String> {
        if uid.is_empty() {
            return Ok(None);
        }
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id FROM shengsuanyun_accounts WHERE uid = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![uid], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(id)) => Ok(Some(id)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    /// 回填账号身份字段（uid / 昵称 / 邮箱 / 头像）。
    ///
    /// 用途：旧版本 `data.ID`（数字）被 `as_str` 静默解析成空串，登录产生的账号
    /// uid 恒为空。这类账号**是真实用户数据**（带凭据与绑定），不能删——
    /// 启动时用已存凭据重新查一次 `/user/info` 把 uid 补回来。
    pub fn update_shengsuanyun_account_identity(
        &self,
        id: &str,
        info: &SsyUserInfo,
        now: i64,
    ) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE shengsuanyun_accounts
                SET uid = ?2, display_name = ?3, email = ?4, avatar_url = ?5, updated_at = ?6
              WHERE id = ?1",
            params![
                id,
                info.uid,
                info.display_name,
                info.email,
                info.avatar_url,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 列出所有 uid 为空的账号 id。
    ///
    /// 这些是旧版本 `data.ID` 解析失败（数字被当成字符串）留下的真实账号，
    /// 供启动时用已存凭据回填 uid，**不可删除**。
    pub fn list_empty_uid_shengsuanyun_account_ids(&self) -> Result<Vec<String>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id FROM shengsuanyun_accounts WHERE uid = '' OR uid IS NULL")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn list_shengsuanyun_accounts(&self) -> Result<Vec<ShengsuanyunAccountRow>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, uid, display_name, email, avatar_url, is_creator,
                        balance_assets, voucher_assets, balance_updated_at, created_at, updated_at
                 FROM shengsuanyun_accounts ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ShengsuanyunAccountRow {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    display_name: row.get(2)?,
                    email: row.get(3)?,
                    avatar_url: row.get(4)?,
                    is_creator: row.get::<_, i64>(5)? != 0,
                    balance_assets: row.get(6)?,
                    voucher_assets: row.get(7)?,
                    balance_updated_at: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_shengsuanyun_account(
        &self,
        id: &str,
    ) -> Result<Option<ShengsuanyunAccountRow>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, uid, display_name, email, avatar_url, is_creator,
                        balance_assets, voucher_assets, balance_updated_at, created_at, updated_at
                 FROM shengsuanyun_accounts WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(ShengsuanyunAccountRow {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    display_name: row.get(2)?,
                    email: row.get(3)?,
                    avatar_url: row.get(4)?,
                    is_creator: row.get::<_, i64>(5)? != 0,
                    balance_assets: row.get(6)?,
                    voucher_assets: row.get(7)?,
                    balance_updated_at: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    pub fn update_shengsuanyun_balance(
        &self,
        id: &str,
        balance_assets: f64,
        voucher_assets: f64,
        now: i64,
    ) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE shengsuanyun_accounts
             SET balance_assets = ?2, voucher_assets = ?3, balance_updated_at = ?4, updated_at = ?4
             WHERE id = ?1",
            params![id, balance_assets, voucher_assets, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 删除账号及其全部绑定。
    pub fn delete_shengsuanyun_account(&self, id: &str) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM shengsuanyun_provider_bindings WHERE account_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM shengsuanyun_accounts WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 写入/更新绑定（app_type + provider_id 唯一）。
    ///
    /// `key_id` 为 None 表示「不改变已记录的选中 Key」（例如 OAuth 重登时只更新
    /// 账号归属）；显式传入（含选回默认 Key）则覆盖。
    pub fn upsert_shengsuanyun_binding(
        &self,
        app_type: &str,
        provider_id: &str,
        account_id: &str,
        credential_source: &str,
        key_id: Option<i64>,
        now: i64,
    ) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO shengsuanyun_provider_bindings
             (app_type, provider_id, account_id, credential_source, key_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(app_type, provider_id) DO UPDATE SET
               account_id = excluded.account_id,
               credential_source = excluded.credential_source,
               key_id = COALESCE(excluded.key_id, shengsuanyun_provider_bindings.key_id),
               updated_at = excluded.updated_at",
            params![
                app_type,
                provider_id,
                account_id,
                credential_source,
                key_id,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 只更新已存在绑定的选中 Key（不存在则不创建，返回 false）。
    pub fn set_shengsuanyun_binding_key(
        &self,
        app_type: &str,
        provider_id: &str,
        account_id: &str,
        key_id: Option<i64>,
        now: i64,
    ) -> Result<bool, String> {
        let conn = lock_conn!(self.conn);
        let changed = conn
            .execute(
                "UPDATE shengsuanyun_provider_bindings
                 SET account_id = ?3, key_id = ?4, updated_at = ?5
                 WHERE app_type = ?1 AND provider_id = ?2",
                params![app_type, provider_id, account_id, key_id, now],
            )
            .map_err(|e| e.to_string())?;
        Ok(changed > 0)
    }

    /// 清理指定 app 下 provider_id 已不存在于 providers 表的幽灵绑定行
    /// （常驻自愈重指向后，旧卡片 id 的绑定必须删掉，否则下次对账先命中幽灵行）。
    /// 绑定表主键是 (app_type, provider_id)，upsert 新 id 不会覆盖旧行。
    pub fn delete_stale_shengsuanyun_bindings(&self, app_type: &str) -> Result<usize, String> {
        let conn = lock_conn!(self.conn);
        let n = conn
            .execute(
                "DELETE FROM shengsuanyun_provider_bindings
                 WHERE app_type = ?1
                   AND provider_id NOT IN (SELECT id FROM providers WHERE app_type = ?1)",
                params![app_type],
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn get_shengsuanyun_binding(
        &self,
        app_type: &str,
        provider_id: &str,
    ) -> Result<Option<ShengsuanyunBindingRow>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT app_type, provider_id, account_id, credential_source, key_id,
                        created_at, updated_at
                 FROM shengsuanyun_provider_bindings
                 WHERE app_type = ?1 AND provider_id = ?2",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![app_type, provider_id], map_binding_row)
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    pub fn list_shengsuanyun_bindings(&self) -> Result<Vec<ShengsuanyunBindingRow>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT app_type, provider_id, account_id, credential_source, key_id,
                        created_at, updated_at
                 FROM shengsuanyun_provider_bindings",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], map_binding_row)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}

fn map_binding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ShengsuanyunBindingRow> {
    Ok(ShengsuanyunBindingRow {
        app_type: row.get(0)?,
        provider_id: row.get(1)?,
        account_id: row.get(2)?,
        credential_source: row.get(3)?,
        key_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssy_core::models::SsyUserInfo;

    fn info(uid: &str, name: &str) -> SsyUserInfo {
        SsyUserInfo {
            uid: uid.into(),
            display_name: name.into(),
            email: "alice@example.com".into(),
            avatar_url: String::new(),
            wallet_assets: 235000.0,
            voucher_assets: 0.0,
        }
    }

    #[test]
    fn upsert_replaces_same_uid() {
        let db = Database::memory().unwrap();
        let a = db
            .upsert_shengsuanyun_account(
                "id-1",
                &info("u1", "Alice"),
                false,
                Some(1.0),
                Some(0.0),
                100,
            )
            .unwrap();
        assert_eq!(a.display_name, "Alice");
        db.upsert_shengsuanyun_account(
            "id-2",
            &info("u1", "Alice2"),
            false,
            Some(2.0),
            Some(0.0),
            200,
        )
        .unwrap();
        let accounts = db.list_shengsuanyun_accounts().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "id-2");
        assert_eq!(accounts[0].display_name, "Alice2");
    }

    #[test]
    fn delete_removes_account_and_bindings() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_account("id-1", &info("u1", "Alice"), false, None, Some(0.0), 100)
            .unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "id-1", "oauth", None, 100)
            .unwrap();
        db.delete_shengsuanyun_account("id-1").unwrap();
        assert!(db.list_shengsuanyun_accounts().unwrap().is_empty());
        assert!(db.list_shengsuanyun_bindings().unwrap().is_empty());
    }

    #[test]
    fn binding_upsert_conflicts_on_app_and_provider() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "a1", "oauth", None, 100)
            .unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "a2", "oauth", None, 200)
            .unwrap();
        db.upsert_shengsuanyun_binding("codex", "p1", "a1", "oauth", None, 200)
            .unwrap();
        let bindings = db.list_shengsuanyun_bindings().unwrap();
        assert_eq!(bindings.len(), 2);
        assert!(bindings
            .iter()
            .any(|b| b.app_type == "claude" && b.account_id == "a2"));
    }

    #[test]
    fn balance_update_persists() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_account("id-1", &info("u1", "A"), false, None, Some(0.0), 100)
            .unwrap();
        db.update_shengsuanyun_balance("id-1", 50000.0, 0.0, 300)
            .unwrap();
        let row = db.get_shengsuanyun_account("id-1").unwrap().unwrap();
        assert_eq!(row.balance_assets, Some(50000.0));
        assert_eq!(row.voucher_assets, Some(0.0));
    }

    #[test]
    fn binding_remembers_selected_key() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "a1", "oauth", None, 100)
            .unwrap();
        // 用户显式选中 Bear Xiong
        assert!(db
            .set_shengsuanyun_binding_key("claude", "p1", "a1", Some(83949), 200)
            .unwrap());
        let b = db
            .get_shengsuanyun_binding("claude", "p1")
            .unwrap()
            .unwrap();
        assert_eq!(b.key_id, Some(83949));
        assert_eq!(b.updated_at, 200);

        // OAuth 重登（key_id = None）不应清掉用户的选择
        db.upsert_shengsuanyun_binding("claude", "p1", "a1", "oauth", None, 300)
            .unwrap();
        assert_eq!(
            db.get_shengsuanyun_binding("claude", "p1")
                .unwrap()
                .unwrap()
                .key_id,
            Some(83949)
        );

        // 未绑定的 provider 不会凭空创建记录
        assert!(!db
            .set_shengsuanyun_binding_key("claude", "p9", "a1", Some(1), 400)
            .unwrap());
        assert!(db
            .get_shengsuanyun_binding("claude", "p9")
            .unwrap()
            .is_none());
    }

    #[test]
    fn account_id_is_stable_across_relogin() {
        let db = Database::memory().unwrap();
        let first = db
            .upsert_shengsuanyun_account("id-1", &info("u1", "Alice"), false, None, Some(0.0), 100)
            .unwrap();
        assert_eq!(first.id, "id-1");
        // 重登前先反查复用同一 id
        let reused = db.find_shengsuanyun_account_id_by_uid("u1").unwrap();
        assert_eq!(reused.as_deref(), Some("id-1"));
        db.upsert_shengsuanyun_binding("claude", "p1", "id-1", "oauth", Some(83949), 100)
            .unwrap();
        db.save_shengsuanyun_credentials("id-1", "sk-1", "jwt-1")
            .unwrap();

        let id = reused.unwrap_or_else(|| "id-2".to_string());
        assert_eq!(id, "id-1");
        db.upsert_shengsuanyun_account(&id, &info("u1", "Alice2"), false, None, Some(0.0), 200)
            .unwrap();
        assert_eq!(db.list_shengsuanyun_accounts().unwrap().len(), 1);
        // 绑定与凭据都不应被级联删除
        assert_eq!(
            db.get_shengsuanyun_binding("claude", "p1")
                .unwrap()
                .unwrap()
                .key_id,
            Some(83949)
        );
        assert_eq!(db.load_shengsuanyun_credentials("id-1").unwrap().0, "sk-1");
    }

    /// 旧版本留下的空 uid 账号是**真实用户数据**：只能回填，不能删。
    /// （2026-09-16 事故：启动清理把用户账号连同 8 个宿主绑定一起删了）
    #[test]
    fn legacy_empty_uid_account_is_backfilled_not_deleted() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_account(
            "legacy",
            &info("", "user_jheayl"),
            false,
            None,
            Some(0.0),
            100,
        )
        .unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "legacy", "oauth", None, 100)
            .unwrap();
        db.save_shengsuanyun_credentials("legacy", "sk-old", "jwt-old")
            .unwrap();

        // 用已存凭据重新查 /user/info 后回填 uid
        db.update_shengsuanyun_account_identity("legacy", &info("62890", "熊叔"), 200)
            .unwrap();

        let accounts = db.list_shengsuanyun_accounts().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].uid, "62890");
        assert_eq!(accounts[0].display_name, "熊叔");
        // 凭据与绑定必须原样保留
        let (api_key, jwt) = db.load_shengsuanyun_credentials("legacy").unwrap();
        assert_eq!(api_key, "sk-old");
        assert_eq!(jwt, "jwt-old");
        assert!(db
            .get_shengsuanyun_binding("claude", "p1")
            .unwrap()
            .is_some());
    }

    /// uid 为空时的 upsert 不能级联删除其它空 uid 行。
    #[test]
    fn empty_uid_upsert_does_not_wipe_other_accounts() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_account("legacy-a", &info("", "A"), false, None, Some(0.0), 100)
            .unwrap();
        db.upsert_shengsuanyun_account("legacy-b", &info("", "B"), false, None, Some(0.0), 100)
            .unwrap();
        assert_eq!(db.list_shengsuanyun_accounts().unwrap().len(), 2);
    }

    /// 启动回填只挑 uid 为空的账号，正常账号不受影响。
    #[test]
    fn list_empty_uid_ids_returns_only_legacy_rows() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_account("legacy", &info("", "Legacy"), false, None, Some(0.0), 100)
            .unwrap();
        db.upsert_shengsuanyun_account("ok", &info("62890", "熊叔"), false, None, Some(0.0), 100)
            .unwrap();
        assert_eq!(
            db.list_empty_uid_shengsuanyun_account_ids().unwrap(),
            vec!["legacy".to_string()]
        );
    }

    // 让 AppError 在本文件类型上可用（避免未使用导入告警）
    #[allow(dead_code)]
    fn _unused(_: AppError) {}
}

// ============ 凭据存储（与 CC Switch 一致：Key 存本库，不依赖 OS Keychain） ============

impl Database {
    pub fn save_shengsuanyun_credentials(
        &self,
        account_id: &str,
        api_key: &str,
        jwt_token: &str,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO shengsuanyun_credentials (account_id, api_key, jwt_token, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(account_id) DO UPDATE SET
               api_key = excluded.api_key,
               jwt_token = excluded.jwt_token,
               updated_at = excluded.updated_at",
            params![account_id, api_key, jwt_token, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_shengsuanyun_credentials(
        &self,
        account_id: &str,
    ) -> Result<(String, String), String> {
        let conn = lock_conn!(self.conn);
        conn.query_row(
            "SELECT api_key, jwt_token FROM shengsuanyun_credentials WHERE account_id = ?1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())
    }

    pub fn delete_shengsuanyun_credentials(&self, account_id: &str) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM shengsuanyun_credentials WHERE account_id = ?1",
            params![account_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;

    #[test]
    fn credentials_crud_roundtrip() {
        let db = Database::memory().unwrap();
        db.save_shengsuanyun_credentials("a1", "sk-1", "jwt-1")
            .unwrap();
        assert_eq!(
            db.load_shengsuanyun_credentials("a1").unwrap(),
            ("sk-1".into(), "jwt-1".into())
        );
        // upsert 覆盖
        db.save_shengsuanyun_credentials("a1", "sk-2", "").unwrap();
        assert_eq!(
            db.load_shengsuanyun_credentials("a1").unwrap(),
            ("sk-2".into(), String::new())
        );
        db.delete_shengsuanyun_credentials("a1").unwrap();
        assert!(db.load_shengsuanyun_credentials("a1").is_err());
        // 删除幂等
        db.delete_shengsuanyun_credentials("a1").unwrap();
    }
}
