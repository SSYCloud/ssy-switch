//! 埋点事件队列（SSY-Switch）。
//!
//! 设计要点：
//! - 匿名 install_id（首启生成 UUID，存 settings），不含任何用户标识
//! - 事件先入 SQLite 队列，批量上报到可配置端点（settings `analytics_endpoint`）
//! - 端点为空 = 仅本地模式（队列自动裁剪，防止无限增长）
//! - 隐私红线：事件名/属性为固定结构，禁止携带 API Key/jwt/code/邮箱/完整 uid/提示词

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use rusqlite::params;
use serde_json::Value;

impl Database {
    /// 匿名安装 ID：首次生成 UUID 并持久化，之后稳定不变
    pub fn analytics_install_id(&self) -> Result<String, AppError> {
        if let Some(id) = self.get_setting("analytics_install_id")? {
            if !id.is_empty() {
                return Ok(id);
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.set_setting("analytics_install_id", &id)?;
        Ok(id)
    }

    pub fn analytics_track(&self, event: &str, props: &Value) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO analytics_events (ts, event, props) VALUES (?1, ?2, ?3)",
            params![
                chrono::Utc::now().timestamp_millis(),
                event,
                props.to_string()
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 读取未上报事件（最旧优先，限量）
    pub fn analytics_unflushed(&self, limit: i64) -> Result<Vec<(i64, String, Value)>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, ts, event, props FROM analytics_events WHERE flushed = 0 ORDER BY id ASC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            let (id, ts, event, props) = r.map_err(|e| e.to_string())?;
            let props: Value =
                serde_json::from_str(&props).unwrap_or_else(|_| serde_json::json!({}));
            out.push((
                id,
                event.clone(),
                serde_json::json!({ "ts": ts, "event": event, "props": props }),
            ));
        }
        Ok(out)
    }

    pub fn analytics_mark_flushed(&self, ids: &[i64]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let conn = lock_conn!(self.conn);
        let list = ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        conn.execute(
            &format!("UPDATE analytics_events SET flushed = 1 WHERE id IN ({list})"),
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 本地模式下裁剪队列：保留最近 N 条，删除更旧的（防止无限增长）
    pub fn analytics_trim(&self, keep: i64) -> Result<i64, String> {
        let conn = lock_conn!(self.conn);
        let deleted = conn
            .execute(
                "DELETE FROM analytics_events WHERE id NOT IN (
                   SELECT id FROM analytics_events ORDER BY id DESC LIMIT ?1
                 )",
                params![keep],
            )
            .map_err(|e| e.to_string())?;
        Ok(deleted as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn install_id_is_stable() {
        let db = Database::memory().unwrap();
        let id1 = db.analytics_install_id().unwrap();
        let id2 = db.analytics_install_id().unwrap();
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 36); // uuid v4
    }

    #[test]
    fn track_queue_and_trim() {
        let db = Database::memory().unwrap();
        for i in 0..10 {
            db.analytics_track("evt", &json!({ "i": i })).unwrap();
        }
        assert_eq!(db.analytics_unflushed(100).unwrap().len(), 10);
        db.analytics_trim(5).unwrap();
        let remain = db.analytics_unflushed(100).unwrap();
        assert_eq!(remain.len(), 5);
        // 保留的是最新的
        assert_eq!(remain.last().unwrap().0, 10);
        // 标记后不再出现
        let ids: Vec<i64> = remain.iter().map(|(id, _, _)| *id).collect();
        db.analytics_mark_flushed(&ids).unwrap();
        assert!(db.analytics_unflushed(100).unwrap().is_empty());
    }
}
