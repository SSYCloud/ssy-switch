//! 测试/演示用内存实现（`InMemoryCredentialStore` + `InMemoryAccountStore`）。
//!
//! 供消费方单测与本地实验使用；线程安全（Mutex），无持久化。

use crate::credential::{CredentialStore, StoredCredentials};
use crate::models::{AccountRecord, SsyUserInfo};
use crate::store::{AccountStore, AccountUpsert};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryCredentialStore(pub Mutex<HashMap<String, StoredCredentials>>);

impl CredentialStore for InMemoryCredentialStore {
    fn save(&self, account_id: &str, creds: &StoredCredentials) -> Result<(), String> {
        self.0
            .lock()
            .expect("locked")
            .insert(account_id.to_string(), creds.clone());
        Ok(())
    }
    fn load(&self, account_id: &str) -> Result<StoredCredentials, String> {
        self.0
            .lock()
            .expect("locked")
            .get(account_id)
            .cloned()
            .ok_or_else(|| format!("credentials not found: {account_id}"))
    }
    fn delete(&self, account_id: &str) -> Result<(), String> {
        self.0.lock().expect("locked").remove(account_id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryAccountStore(pub Mutex<Vec<AccountRecord>>);

impl AccountStore for InMemoryAccountStore {
    fn find_account_id_by_uid(&self, uid: &str) -> Result<Option<String>, String> {
        if uid.is_empty() {
            return Ok(None);
        }
        Ok(self
            .0
            .lock()
            .expect("locked")
            .iter()
            .find(|a| a.uid == uid)
            .map(|a| a.id.clone()))
    }
    fn upsert_account(&self, upsert: AccountUpsert) -> Result<AccountRecord, String> {
        let AccountUpsert {
            id,
            info,
            is_creator,
            balance_assets,
            voucher_assets,
            now,
        } = upsert;
        let row = AccountRecord {
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
        };
        let mut all = self.0.lock().expect("locked");
        if let Some(slot) = all.iter_mut().find(|a| a.id == id) {
            *slot = row.clone();
        } else {
            all.push(row.clone());
        }
        Ok(row)
    }
    fn list_accounts(&self) -> Result<Vec<AccountRecord>, String> {
        Ok(self.0.lock().expect("locked").clone())
    }
    fn update_balance(&self, id: &str, assets: f64, voucher: f64, ts: i64) -> Result<(), String> {
        let mut all = self.0.lock().expect("locked");
        let a = all
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("account not found: {id}"))?;
        a.balance_assets = Some(assets);
        a.voucher_assets = Some(voucher);
        a.balance_updated_at = Some(ts);
        a.updated_at = ts;
        Ok(())
    }
    fn list_empty_uid_account_ids(&self) -> Result<Vec<String>, String> {
        Ok(self
            .0
            .lock()
            .expect("locked")
            .iter()
            .filter(|a| a.uid.is_empty())
            .map(|a| a.id.clone())
            .collect())
    }
    fn update_account_identity(&self, id: &str, info: &SsyUserInfo, ts: i64) -> Result<(), String> {
        let mut all = self.0.lock().expect("locked");
        let a = all
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("account not found: {id}"))?;
        a.uid = info.uid.clone();
        a.display_name = info.display_name.clone();
        a.email = info.email.clone();
        a.avatar_url = info.avatar_url.clone();
        a.updated_at = ts;
        Ok(())
    }
    fn delete_account(&self, id: &str) -> Result<(), String> {
        self.0.lock().expect("locked").retain(|a| a.id != id);
        Ok(())
    }
}
