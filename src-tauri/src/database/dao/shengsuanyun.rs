//! 胜算云账号与 Provider 绑定数据访问对象。
//!
//! 只存元数据与余额缓存；API Key 永远在 OS Keychain（见 `shengsuanyun::credential_store`）。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::shengsuanyun::models::{ShengsuanyunAccountRow, ShengsuanyunBindingRow, SsyUserInfo};
use rusqlite::params;

impl Database {
    /// 新增或按胜算云 uid 覆盖账号记录（同一上游账号重复登录时旧记录连同旧 Key 一起被替换）。
    /// 返回落库后的行。
    pub fn upsert_shengsuanyun_account(
        &self,
        id: &str,
        info: &SsyUserInfo,
        is_creator: bool,
        balance_assets: Option<i64>,
        now: i64,
    ) -> Result<ShengsuanyunAccountRow, String> {
        let conn = lock_conn!(self.conn);
        // 同 uid 旧记录直接删除（Keychain 侧由调用方保证）
        conn.execute(
            "DELETE FROM shengsuanyun_accounts WHERE uid = ?1 AND id != ?2",
            params![info.uid, id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO shengsuanyun_accounts
             (id, uid, display_name, email, avatar_url, is_creator, balance_assets, balance_updated_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                id,
                info.uid,
                info.display_name,
                info.email,
                info.avatar_url,
                is_creator as i64,
                balance_assets,
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
            balance_updated_at: balance_assets.map(|_| now),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn list_shengsuanyun_accounts(&self) -> Result<Vec<ShengsuanyunAccountRow>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, uid, display_name, email, avatar_url, is_creator,
                        balance_assets, balance_updated_at, created_at, updated_at
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
                    balance_updated_at: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
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
                        balance_assets, balance_updated_at, created_at, updated_at
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
                    balance_updated_at: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
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
        balance_assets: i64,
        now: i64,
    ) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE shengsuanyun_accounts
             SET balance_assets = ?2, balance_updated_at = ?3, updated_at = ?3
             WHERE id = ?1",
            params![id, balance_assets, now],
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
    pub fn upsert_shengsuanyun_binding(
        &self,
        app_type: &str,
        provider_id: &str,
        account_id: &str,
        credential_source: &str,
        now: i64,
    ) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO shengsuanyun_provider_bindings
             (app_type, provider_id, account_id, credential_source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(app_type, provider_id) DO UPDATE SET
               account_id = excluded.account_id,
               credential_source = excluded.credential_source,
               updated_at = excluded.updated_at",
            params![app_type, provider_id, account_id, credential_source, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_shengsuanyun_bindings(&self) -> Result<Vec<ShengsuanyunBindingRow>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT app_type, provider_id, account_id, credential_source, created_at, updated_at
                 FROM shengsuanyun_provider_bindings",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ShengsuanyunBindingRow {
                    app_type: row.get(0)?,
                    provider_id: row.get(1)?,
                    account_id: row.get(2)?,
                    credential_source: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shengsuanyun::models::SsyUserInfo;

    fn info(uid: &str, name: &str) -> SsyUserInfo {
        SsyUserInfo {
            uid: uid.into(),
            display_name: name.into(),
            email: "alice@example.com".into(),
            avatar_url: String::new(),
            wallet_assets: 235000,
        }
    }

    #[test]
    fn upsert_replaces_same_uid() {
        let db = Database::memory().unwrap();
        let a = db
            .upsert_shengsuanyun_account("id-1", &info("u1", "Alice"), false, Some(1), 100)
            .unwrap();
        assert_eq!(a.display_name, "Alice");
        db.upsert_shengsuanyun_account("id-2", &info("u1", "Alice2"), false, Some(2), 200)
            .unwrap();
        let accounts = db.list_shengsuanyun_accounts().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "id-2");
        assert_eq!(accounts[0].display_name, "Alice2");
    }

    #[test]
    fn delete_removes_account_and_bindings() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_account("id-1", &info("u1", "Alice"), false, None, 100)
            .unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "id-1", "oauth", 100)
            .unwrap();
        db.delete_shengsuanyun_account("id-1").unwrap();
        assert!(db.list_shengsuanyun_accounts().unwrap().is_empty());
        assert!(db.list_shengsuanyun_bindings().unwrap().is_empty());
    }

    #[test]
    fn binding_upsert_conflicts_on_app_and_provider() {
        let db = Database::memory().unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "a1", "oauth", 100)
            .unwrap();
        db.upsert_shengsuanyun_binding("claude", "p1", "a2", "oauth", 200)
            .unwrap();
        db.upsert_shengsuanyun_binding("codex", "p1", "a1", "oauth", 200)
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
        db.upsert_shengsuanyun_account("id-1", &info("u1", "A"), false, None, 100)
            .unwrap();
        db.update_shengsuanyun_balance("id-1", 50000, 300).unwrap();
        let row = db.get_shengsuanyun_account("id-1").unwrap().unwrap();
        assert_eq!(row.balance_assets, Some(50000));
    }

    // 让 AppError 在本文件类型上可用（避免未使用导入告警）
    #[allow(dead_code)]
    fn _unused(_: AppError) {}
}
