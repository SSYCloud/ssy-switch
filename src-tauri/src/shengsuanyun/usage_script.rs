//! 胜算云默认用量脚本（应用专属：依赖 provider::ProviderMeta，不属 SDK 域）。

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
