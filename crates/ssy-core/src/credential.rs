//! 凭据存储契约。SDK 不关心存哪里（Keychain / SQLite / 文件），宿主实现本 trait。

use serde::{Deserialize, Serialize};

/// `/auth/keys` 下发的双凭据（一个 JSON 条目整体存取）
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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

/// 凭据存储 SPI。
pub trait CredentialStore: Send + Sync {
    fn save(&self, account_id: &str, creds: &StoredCredentials) -> Result<(), String>;
    fn load(&self, account_id: &str) -> Result<StoredCredentials, String>;
    /// 不存在时也应返回 Ok（幂等登出）
    fn delete(&self, account_id: &str) -> Result<(), String>;
}
