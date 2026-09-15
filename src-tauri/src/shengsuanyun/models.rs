//! 胜算云模块的 DTO 与错误类型。

use serde::{Deserialize, Serialize};

/// 胜算云用户 API 基地址
pub const SSY_API_BASE: &str = "https://api.shengsuanyun.com";
/// 胜算云 OAuth 授权页
pub const SSY_AUTH_URL: &str = "https://router.shengsuanyun.com/auth";
/// 胜算云创作者市场 API 基地址
pub const SSY_LOOM_BASE: &str = "https://loomloom.shengsuanyun.com/loom/v1";

/// state 有效期（秒）
pub const PENDING_STATE_TTL_SECS: i64 = 300;

/// 数据库中的胜算云账号记录（不含 API Key）
#[derive(Clone, Debug, Serialize)]
pub struct ShengsuanyunAccountRow {
    pub id: String,
    pub uid: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: String,
    pub is_creator: bool,
    pub balance_assets: Option<f64>,
    pub balance_updated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 暴露给前端的脱敏账号 DTO
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShengsuanyunAccountView {
    pub id: String,
    pub uid_masked: String,
    pub display_name: String,
    pub email_masked: String,
    pub avatar_url: String,
    pub is_creator: bool,
    /// 余额（元），None 表示尚未获取
    pub balance_yuan: Option<f64>,
    pub balance_updated_at: Option<i64>,
    pub created_at: i64,
}

/// `POST /auth/keys` 的请求体
#[derive(Debug, Serialize)]
pub struct ExchangeCodeRequest<'a> {
    pub code: &'a str,
    pub callback_url: &'a str,
}

/// 上游用户信息（`GET /user/info` 的 data 部分，字段大小写做了兼容）
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SsyUserInfo {
    #[serde(default, alias = "id")]
    pub uid: String,
    #[serde(default, alias = "nickname")]
    pub display_name: String,
    #[serde(default, alias = "email")]
    pub email: String,
    #[serde(default, alias = "headImg")]
    pub avatar_url: String,
    /// 钱包资产（上游原始单位，展示层才换算为元）
    #[serde(default)]
    pub wallet_assets: f64,
}

/// 登录会话启动结果（不含任何密钥）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub authorization_url: String,
    pub session_id: String,
}

/// OAuth 完成事件 payload（脱敏，不含 API Key / code / callback query）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCompletePayload {
    pub account: ShengsuanyunAccountView,
    pub app_type: Option<String>,
    pub provider_id: Option<String>,
}

/// OAuth 失败事件 payload
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthFailedPayload {
    pub session_id: String,
    pub reason: String,
}

/// 账号与 Provider 的绑定记录
#[derive(Clone, Debug, Serialize)]
pub struct ShengsuanyunBindingRow {
    pub app_type: String,
    pub provider_id: String,
    pub account_id: String,
    pub credential_source: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 对 uid 做脱敏：保留首尾各 2 字符
pub fn mask_uid(uid: &str) -> String {
    let chars: Vec<char> = uid.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len().max(1));
    }
    format!(
        "{}{}{}",
        chars[..2].iter().collect::<String>(),
        "*".repeat(chars.len() - 4),
        chars[chars.len() - 2..].iter().collect::<String>()
    )
}

/// 对邮箱做脱敏：用户名保留前 2 位 + 域名完整保留
pub fn mask_email(email: &str) -> String {
    let Some((user, domain)) = email.split_once('@') else {
        return String::new();
    };
    let masked_user: String = user
        .chars()
        .enumerate()
        .map(|(i, c)| if i < 2 { c } else { '*' })
        .collect();
    format!("{masked_user}@{domain}")
}

/// 上游资产单位 → 元（仅展示层调用，全链路只换算一次）
pub fn assets_to_yuan(assets: f64) -> f64 {
    assets / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_uid_keeps_edges() {
        assert_eq!(mask_uid("12345678"), "12****78");
        assert_eq!(mask_uid("ab"), "**");
    }

    #[test]
    fn mask_email_masks_local_part() {
        assert_eq!(mask_email("alice@example.com"), "al***@example.com");
        assert_eq!(mask_email("not-an-email"), "");
    }

    #[test]
    fn assets_convert_once() {
        assert!((assets_to_yuan(235000.0) - 23.5).abs() < 1e-9);
        assert!((assets_to_yuan(1500.5) - 0.15005).abs() < 1e-9);
    }
}
