//! SDK 存储契约的 SQLite 实现（委托既有 DAO 方法）。

use crate::database::Database;
use ssy_core::credential::{CredentialStore, StoredCredentials};
use ssy_core::models::{AccountRecord, SsyUserInfo};
use ssy_core::store::{AccountStore, AccountUpsert};
use std::sync::Arc;

#[derive(Clone)]
pub struct SqliteCredentialStore(pub Arc<Database>);

impl CredentialStore for SqliteCredentialStore {
    fn save(&self, account_id: &str, creds: &StoredCredentials) -> Result<(), String> {
        self.0
            .save_shengsuanyun_credentials(account_id, &creds.api_key, &creds.jwt_token)
    }
    fn load(&self, account_id: &str) -> Result<StoredCredentials, String> {
        let (api_key, jwt_token) = self.0.load_shengsuanyun_credentials(account_id)?;
        Ok(StoredCredentials { api_key, jwt_token })
    }
    fn delete(&self, account_id: &str) -> Result<(), String> {
        self.0.delete_shengsuanyun_credentials(account_id)
    }
}

#[derive(Clone)]
pub struct SqliteAccountStore(pub Arc<Database>);

impl AccountStore for SqliteAccountStore {
    fn find_account_id_by_uid(&self, uid: &str) -> Result<Option<String>, String> {
        if uid.is_empty() {
            return Ok(None); // 空 uid 不作为查找键（2026-09-16 事故规则）
        }
        self.0.find_shengsuanyun_account_id_by_uid(uid)
    }
    fn upsert_account(&self, u: AccountUpsert) -> Result<AccountRecord, String> {
        let AccountUpsert {
            id,
            info,
            is_creator,
            balance_assets,
            voucher_assets,
            now,
        } = u;
        self.0.upsert_shengsuanyun_account(
            id,
            info,
            is_creator,
            balance_assets,
            voucher_assets,
            now,
        )
    }
    fn list_accounts(&self) -> Result<Vec<AccountRecord>, String> {
        self.0.list_shengsuanyun_accounts()
    }
    fn update_balance(&self, id: &str, assets: f64, voucher: f64, ts: i64) -> Result<(), String> {
        self.0.update_shengsuanyun_balance(id, assets, voucher, ts)
    }
    fn list_empty_uid_account_ids(&self) -> Result<Vec<String>, String> {
        self.0.list_empty_uid_shengsuanyun_account_ids()
    }
    fn update_account_identity(&self, id: &str, info: &SsyUserInfo, ts: i64) -> Result<(), String> {
        self.0.update_shengsuanyun_account_identity(id, info, ts)
    }
    fn delete_account(&self, id: &str) -> Result<(), String> {
        self.0.delete_shengsuanyun_account(id)
    }
}
