//! 胜算云凭据存储。
//!
//! 存储位置：自家 SQLite（`shengsuanyun_credentials` 表），与 CC Switch 上游
//! 对 Provider Key 的处理方式保持一致（用户决策，2026-09-16）。
//! SQLite 中其余表仍只存脱敏元数据；导出/同步含凭据的事实已记入
//! `docs/ssy-oauth-contract.md` 的安全约束章节。

use crate::database::Database;

#[derive(Clone, Debug, Default)]
pub struct StoredCredentials {
    pub api_key: String,
    pub jwt_token: String,
}

impl StoredCredentials {
    /// 用户信息查询用的 token：优先 jwt_token（参考 main.go:543），缺失时退回 api_key
    pub fn identity_token(&self) -> &str {
        if self.jwt_token.is_empty() {
            &self.api_key
        } else {
            &self.jwt_token
        }
    }
}

pub fn save_credentials(
    db: &Database,
    account_id: &str,
    creds: &StoredCredentials,
) -> Result<(), String> {
    db.save_shengsuanyun_credentials(account_id, &creds.api_key, &creds.jwt_token)
}

pub fn load_credentials(db: &Database, account_id: &str) -> Result<StoredCredentials, String> {
    let (api_key, jwt_token) = db.load_shengsuanyun_credentials(account_id)?;
    Ok(StoredCredentials { api_key, jwt_token })
}

pub fn delete_credentials(db: &Database, account_id: &str) -> Result<(), String> {
    db.delete_shengsuanyun_credentials(account_id)
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
