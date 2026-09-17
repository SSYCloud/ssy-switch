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

/// `GET /token/list` 返回的单条 Token（上游原始结构，字段按实际响应解析）。
///
/// 注意：`token` 是**完整明文**，只在「用户显式选择某一把 Key」时按需读取，
/// 不得整表落库、不得写日志、不得一次性灌进前端。
#[derive(Clone, Debug, Default)]
pub struct SsyToken {
    pub id: i64,
    pub name: String,
    /// 完整明文 Key
    pub token: String,
    pub desc: String,
    pub is_default: bool,
    pub is_banned: bool,
    pub is_expired: bool,
    /// 额度上限（上游原始单位，0 = 无上限）
    pub max_quota: f64,
    /// 已消耗（上游原始单位）
    pub consumed_amount: f64,
    pub created_at: i64,
    pub expires_at: i64,
    pub supported_models: Vec<String>,
}

impl SsyToken {
    /// 额度上限（元）；None 表示无上限（上游 `MaxQuota == 0`）
    pub fn max_quota_yuan(&self) -> Option<f64> {
        (self.max_quota > 0.0).then(|| assets_to_yuan(self.max_quota))
    }

    pub fn consumed_yuan(&self) -> f64 {
        assets_to_yuan(self.consumed_amount)
    }

    /// 是否可选：被禁用或已过期的 Key 不允许绑定
    pub fn selectable(&self) -> bool {
        !self.is_banned && !self.is_expired
    }

    pub fn view(&self) -> SsyTokenView {
        SsyTokenView {
            id: self.id,
            name: self.name.clone(),
            token_masked: mask_token(&self.token),
            desc: self.desc.clone(),
            is_default: self.is_default,
            is_banned: self.is_banned,
            is_expired: self.is_expired,
            selectable: self.selectable(),
            max_quota_yuan: self.max_quota_yuan(),
            consumed_yuan: self.consumed_yuan(),
            created_at: self.created_at,
            expires_at: self.expires_at,
            supported_models: self.supported_models.clone(),
        }
    }
}

/// 暴露给前端的 Token 视图（脱敏：只给掩码，不给明文）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SsyTokenView {
    pub id: i64,
    pub name: String,
    pub token_masked: String,
    pub desc: String,
    pub is_default: bool,
    pub is_banned: bool,
    pub is_expired: bool,
    pub selectable: bool,
    /// 额度上限（元）；None 表示无上限
    pub max_quota_yuan: Option<f64>,
    pub consumed_yuan: f64,
    pub created_at: i64,
    pub expires_at: i64,
    pub supported_models: Vec<String>,
}

/// 对 Key 掩码：保留前 6 位与后 4 位
pub fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 12 {
        return "*".repeat(chars.len());
    }
    format!(
        "{}……{}",
        chars[..6].iter().collect::<String>(),
        chars[chars.len() - 4..].iter().collect::<String>()
    )
}

/// 账号与 Provider 的绑定记录
#[derive(Clone, Debug, Serialize)]
pub struct ShengsuanyunBindingRow {
    pub app_type: String,
    pub provider_id: String,
    pub account_id: String,
    pub credential_source: String,
    /// 用户显式选中的上游 Token ID（`/token/list` 的 ID）。
    /// None 表示使用账号默认 Key（OAuth 下发的那把）。
    pub key_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 暴露给前端的绑定视图（脱敏，无明文 Key）
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShengsuanyunBindingView {
    pub app_type: String,
    pub provider_id: String,
    pub account_id: String,
    pub credential_source: String,
    pub key_id: Option<i64>,
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

/// 胜算云 Provider 出厂默认的用量查询脚本。
///
/// - `{{shengsuanyunJwt}}` 在每次查询时由应用注入当前登录凭据（网关 Key 不被 /user/info 接受）
/// - 仅在 Provider 尚无用量脚本时应用，绝不覆盖用户已保存的配置
pub fn default_usage_script_meta() -> crate::provider::ProviderMeta {
    let code = r#"({
  request: {
    url: "https://api.shengsuanyun.com/user/info",
    method: "GET",
    headers: {
      "x-token": "{{shengsuanyunJwt}}",
    },
  },
  extractor: function (response) {
    const data = response.data || response || {};
    const wallet = data.Wallet || data.wallet || {};
    const assets = Number(wallet.Assets ?? wallet.assets ?? 0);
    return {
      remaining: assets / 10000,
      unit: "CNY",
    };
  },
})"#;
    crate::provider::ProviderMeta {
        usage_script: Some(crate::provider::UsageScript {
            enabled: true,
            language: "javascript".to_string(),
            code: code.to_string(),
            timeout: Some(10),
            api_key: None,
            base_url: None,
            access_token: None,
            user_id: None,
            template_type: None,
            auto_query_interval: Some(5),
            coding_plan_provider: None,
            access_key_id: None,
            secret_access_key: None,
            team_organization_id: None,
            team_project_id: None,
        }),
        ..Default::default()
    }
}

/// Provider 是否已配置（任意）用量脚本。
pub fn has_usage_script(provider: &crate::provider::Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.usage_script.as_ref())
        .is_some()
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
    fn mask_token_keeps_edges() {
        assert_eq!(mask_token("T5U-t_abcdefghijklmnop"), "T5U-t_……mnop");
        assert_eq!(mask_token("short"), "*****");
        assert_eq!(mask_token(""), "");
    }

    #[test]
    fn max_quota_zero_means_unlimited() {
        let mut t = SsyToken {
            token: "k".repeat(90),
            max_quota: 0.0,
            ..Default::default()
        };
        assert_eq!(t.max_quota_yuan(), None);
        t.max_quota = 1_000_000.0;
        assert!((t.max_quota_yuan().unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn token_view_is_masked() {
        let t = SsyToken {
            id: 83949,
            name: "Bear Xiong".into(),
            token: "T5U-t_0123456789abcdefghijklmnopqrstuvwxyz".into(),
            max_quota: 1_000_000.0,
            consumed_amount: 0.0,
            ..Default::default()
        };
        let v = t.view();
        assert_eq!(v.id, 83949);
        assert!(!v.token_masked.contains("0123456789abcdef"));
        assert!(v.selectable);
        assert!(v.token_masked.starts_with("T5U-t_"));
    }

    #[test]
    fn assets_convert_once() {
        assert!((assets_to_yuan(235000.0) - 23.5).abs() < 1e-9);
        assert!((assets_to_yuan(1500.5) - 0.15005).abs() < 1e-9);
    }
}
