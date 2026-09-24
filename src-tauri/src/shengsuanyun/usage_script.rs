//! 胜算云默认用量脚本（应用专属：依赖 provider::ProviderMeta，不属 SDK 域）。
//!
//! 认证用 **api_key**（Bearer，网关余额端点），不用 jwt：jwt 约 6.9 天过期且
//! 无续期路径（2026-09-24 实锤：过期后所有卡片余额显示 0.00）；api_key 是
//! 模型网关凭据，不随登录态过期，手动填 Key 的用户同样可用。
//! 端点契约见 ssy-core 仓库 openapi/ssy-api.yaml（/api/v1/balance），
//! account_balance_cny 已为元，无需换算。

/// 当前出厂脚本（api_key 版）。
pub fn factory_script_code() -> &'static str {
    r#"({
  request: {
    url: "https://router.shengsuanyun.com/api/v1/balance",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
    },
  },
  extractor: function (response) {
    const data = (response && response.data) || {};
    return {
      remaining: Number(data.account_balance_cny ?? 0),
      unit: "CNY",
    };
  },
})"#
}

/// 旧出厂脚本（jwt 版，2026-09-24 前出厂）。启动对账按此精确识别并升级。
pub fn legacy_factory_script_code() -> &'static str {
    r#"({
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
})"#
}

pub fn default_usage_script_meta() -> crate::provider::ProviderMeta {
    crate::provider::ProviderMeta {
        usage_script: Some(crate::provider::UsageScript {
            enabled: true,
            language: "javascript".to_string(),
            code: factory_script_code().to_string(),
            timeout: Some(10),
            api_key: None,
            base_url: None,
            access_token: None,
            user_id: None,
            template_type: Some("shengsuanyun".to_string()),
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

#[allow(dead_code)] // reconcile 内联判断；保留作公共 API
pub fn has_usage_script(provider: &crate::provider::Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.usage_script.as_ref())
        .is_some()
}
