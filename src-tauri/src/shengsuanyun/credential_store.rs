//! API Key 的 OS 安全存储抽象。
//!
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: Secret Service (DBus)
//!
//! SQLite 中只保存账号元数据，永不落明文 Key（见 `models::ShengsuanyunAccountRow`）。

use keyring::Entry;

const SERVICE: &str = "com.shengsuanyun.ssy-switch";

fn entry(account_id: &str) -> keyring::Result<Entry> {
    Entry::new(SERVICE, account_id)
}

pub fn save_api_key(account_id: &str, api_key: &str) -> Result<(), String> {
    entry(account_id)
        .and_then(|e| e.set_password(api_key))
        .map_err(|e| format!("save api key to OS credential store failed: {e}"))
}

pub fn load_api_key(account_id: &str) -> Result<String, String> {
    entry(account_id)
        .and_then(|e| e.get_password())
        .map_err(|e| format!("load api key from OS credential store failed: {e}"))
}

pub fn delete_api_key(account_id: &str) -> Result<(), String> {
    match entry(account_id).and_then(|e| e.delete_credential()) {
        Ok(()) => Ok(()),
        // 条目本就不存在时视为成功，保证登出幂等
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("delete api key failed: {e}")),
    }
}
