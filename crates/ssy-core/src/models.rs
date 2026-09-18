//! 数据模型：DTO、脱敏与单位换算。
//!
//! 单位约定（与 `openapi/ssy-api.yaml` 一致）：
//! - `AmountT1e4`（钱包 Assets / 账单）：1e-4 元，10,000 = 1 元
//! - `AmountT1e7`（loom 余额 / 调用消费）：1e-7 元，10,000,000 = 1 元
//! - 换算只在展示层（[`assets_to_yuan`]）执行一次

use serde::Deserialize;
use serde::Serialize;

/// 胜算云用户 API 基地址
pub const SSY_API_BASE: &str = "https://api.shengsuanyun.com";
/// 胜算云 OAuth 授权页
pub const SSY_AUTH_URL: &str = "https://router.shengsuanyun.com/auth";
/// 胜算云 Loom 市场 API 基地址
pub const SSY_LOOM_BASE: &str = "https://loomloom.shengsuanyun.com/loom/v1";
/// 登录 state 有效期（秒）
pub const PENDING_STATE_TTL_SECS: i64 = 300;

/// OAuth 授权成功后下发的双凭据。
///
/// 用途严格区分（2026-09-18 实测）：
/// - `api_key`：模型网关 `Authorization: Bearer`
/// - `jwt_token`：账户类接口 `x-token`（/user/info、/token/list…）
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SsyCredentials {
    pub api_key: String,
    pub jwt_token: String,
}

impl SsyCredentials {
    /// 账户类接口用的 token：优先 jwt（缺失时上游报 20003），退回 api_key
    pub fn identity_token(&self) -> &str {
        if self.jwt_token.is_empty() {
            &self.api_key
        } else {
            &self.jwt_token
        }
    }
}

/// 账号记录（SDK 契约；持久化方式由 `AccountStore` 实现方决定）
#[derive(Clone, Debug, Serialize)]
pub struct AccountRecord {
    pub id: String,
    pub uid: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: String,
    pub is_creator: bool,
    pub balance_assets: Option<f64>,
    pub voucher_assets: Option<f64>,
    pub balance_updated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 账号下单条 Key（`/token/list` 条目，字段大小写双兼容）
#[derive(Clone, Debug, Deserialize)]
pub struct SsyToken {
    #[serde(default, alias = "id", rename = "ID")]
    pub id: i64,
    #[serde(default, rename = "Name")]
    pub name: String,
    /// 完整明文 Key（仅在单条查询时返回）
    #[serde(default, rename = "Token")]
    pub token: String,
    #[serde(default, rename = "Desc", alias = "desc")]
    pub desc: String,
    #[serde(default, rename = "IsDefault")]
    pub is_default: bool,
    #[serde(default, rename = "IsBanned")]
    pub is_banned: bool,
    #[serde(default, rename = "IsExpired")]
    pub is_expired: bool,
    /// 额度上限（上游原始单位，0 = 无上限）
    #[serde(default, rename = "MaxQuota")]
    pub max_quota: f64,
    /// 已消耗（上游原始单位）
    #[serde(default, rename = "ConsumedAmount")]
    pub consumed_amount: f64,
    #[serde(default, rename = "CreatedAt")]
    pub created_at: i64,
    #[serde(default, rename = "ExpiresAt")]
    pub expires_at: i64,
    #[serde(default, rename = "SupportedModels")]
    pub supported_models: Vec<String>,
}

impl SsyToken {
    /// 额度上限（元）；None 表示无上限（上游 `MaxQuota == 0`）
    pub fn max_quota_yuan(&self) -> Option<f64> {
        (self.max_quota > 0.0).then(|| assets_to_yuan(self.max_quota))
    }

    /// 已消耗（元）
    pub fn consumed_yuan(&self) -> f64 {
        assets_to_yuan(self.consumed_amount)
    }

    /// 是否可选（未封禁、未过期）
    pub fn selectable(&self) -> bool {
        !self.is_banned && !self.is_expired
    }

    /// 脱敏视图
    pub fn view(&self) -> SsyTokenView {
        SsyTokenView {
            id: self.id,
            name: self.name.clone(),
            desc: self.desc.clone(),
            is_default: self.is_default,
            is_banned: self.is_banned,
            is_expired: self.is_expired,
            max_quota_yuan: self.max_quota_yuan(),
            consumed_yuan: Some(assets_to_yuan(self.consumed_amount)),
            created_at: self.created_at,
            expires_at: self.expires_at,
            supported_models: self.supported_models.clone(),
        }
    }
}
/// Key 脱敏视图（下发前端的形态，永不含明文）
#[derive(Clone, Debug, Serialize)]
pub struct SsyTokenView {
    pub id: i64,
    pub name: String,
    pub desc: String,
    pub is_default: bool,
    pub is_banned: bool,
    pub is_expired: bool,
    /// 额度上限（元）；None 表示无上限（上游 MaxQuota == 0）
    pub max_quota_yuan: Option<f64>,
    /// 已消耗（元）
    pub consumed_yuan: Option<f64>,
    pub created_at: i64,
    pub expires_at: i64,
    pub supported_models: Vec<String>,
}

/// 上游用户信息（`/user/info` 的 data，字段大小写双兼容）
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
    #[serde(default)]
    pub wallet_assets: f64,
    #[serde(default)]
    pub voucher_assets: f64,
}

/// `/auth/keys` 请求体
#[derive(Serialize)]
pub struct ExchangeCodeRequest<'a> {
    pub code: &'a str,
    pub callback_url: &'a str,
}

/// 登录会话启动结果（不含任何密钥）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub authorization_url: String,
    pub session_id: String,
}

/// OAuth 完成事件 payload（脱敏；不含 API Key / code / callback query）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCompletePayload {
    pub account: AccountView,
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

/// 账号卡片视图（脱敏）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub uid_masked: String,
    pub display_name: String,
    pub email_masked: String,
    pub avatar_url: String,
    pub is_creator: bool,
    /// 余额（元），None 表示尚未获取
    pub balance_yuan: Option<f64>,
    /// 体验券（元），None 表示尚未获取
    pub voucher_yuan: Option<f64>,
    pub balance_updated_at: Option<i64>,
    pub created_at: i64,
}

/// [`AccountRecord`] → 脱敏视图（体验券字段一并输出，可为 ¥0.00）
pub fn account_view(row: &AccountRecord) -> AccountView {
    AccountView {
        id: row.id.clone(),
        uid_masked: mask_uid(&row.uid),
        display_name: row.display_name.clone(),
        email_masked: mask_email(&row.email),
        avatar_url: row.avatar_url.clone(),
        is_creator: row.is_creator,
        balance_yuan: row.balance_assets.map(assets_to_yuan),
        voucher_yuan: row.voucher_assets.map(assets_to_yuan),
        balance_updated_at: row.balance_updated_at,
        created_at: row.created_at,
    }
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

/// 上游资产单位（1e-4 元）→ 元。仅展示层调用，全链路只换算一次。
pub fn assets_to_yuan(assets: f64) -> f64 {
    assets / 10_000.0
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
        assert!((assets_to_yuan(235_000.0) - 23.5).abs() < 1e-9);
        assert!((assets_to_yuan(1_500.5) - 0.150_05).abs() < 1e-9);
    }

    #[test]
    fn account_view_masks_and_converts() {
        let row = AccountRecord {
            id: "id".into(),
            uid: "12345678".into(),
            display_name: "A".into(),
            email: "al***@example.com".into(),
            avatar_url: String::new(),
            is_creator: true,
            balance_assets: Some(100_000.0),
            voucher_assets: Some(0.0),
            balance_updated_at: Some(1),
            created_at: 1,
            updated_at: 1,
        };
        let v = account_view(&row);
        assert_eq!(v.balance_yuan, Some(10.0));
        assert_eq!(v.voucher_yuan, Some(0.0));
        assert_eq!(v.uid_masked, "12****78");
    }
}
