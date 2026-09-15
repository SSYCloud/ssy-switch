//! API Key / jwt_token 的 OS 安全存储抽象。
//!
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: Secret Service (DBus)
//!
//! SQLite 中只保存账号元数据，永不落明文凭据（见 `models::ShengsuanyunAccountRow`）。
//! 一个账号条目存 JSON（api_key + jwt_token），便于整体读写与删除。

use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "com.shengsuanyun.ssy-switch";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct StoredCredentials {
    pub api_key: String,
    #[serde(default)]
    pub jwt_token: String,
}

impl StoredCredentials {
    /// 用户信息查询用的 token：优先 jwt_token（main.go:543），缺失时退回 api_key
    pub fn identity_token(&self) -> &str {
        if self.jwt_token.is_empty() {
            &self.api_key
        } else {
            &self.jwt_token
        }
    }
}

fn entry(account_id: &str) -> keyring::Result<Entry> {
    Entry::new(SERVICE, account_id)
}

pub fn save_credentials(account_id: &str, creds: &StoredCredentials) -> Result<(), String> {
    let payload = serde_json::to_string(creds).map_err(|e| e.to_string())?;
    entry(account_id)
        .and_then(|e| e.set_password(&payload))
        .map_err(|e| format!("save credentials to OS credential store failed: {e}"))
}

pub fn load_credentials(account_id: &str) -> Result<StoredCredentials, String> {
    let raw = entry(account_id)
        .and_then(|e| e.get_password())
        .map_err(|e| format!("load credentials from OS credential store failed: {e}"))?;
    // 兼容只存过裸 api_key 的旧条目
    if raw.starts_with('{') {
        serde_json::from_str(&raw).map_err(|e| format!("decode stored credentials: {e}"))
    } else {
        Ok(StoredCredentials {
            api_key: raw,
            jwt_token: String::new(),
        })
    }
}

pub fn delete_credentials(account_id: &str) -> Result<(), String> {
    match entry(account_id).and_then(|e| e.delete_credential()) {
        Ok(()) => Ok(()),
        // 条目本就不存在时视为成功，保证登出幂等
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("delete credentials failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_token_prefers_jwt() {
        let c = StoredCredentials {
            api_key: "ak".into(),
            jwt_token: "jwt".into(),
        };
        assert_eq!(c.identity_token(), "jwt");
        let c2 = StoredCredentials {
            api_key: "ak".into(),
            jwt_token: String::new(),
        };
        assert_eq!(c2.identity_token(), "ak");
    }
}
