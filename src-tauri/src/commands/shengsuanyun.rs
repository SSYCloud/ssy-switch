//! 胜算云 OAuth 的 Tauri commands。
//!
//! 约定：
//! - 所有返回 DTO 均为脱敏数据（无 API Key / code / state）
//! - OAuth 结果通过 `shengsuanyun-oauth-complete` / `shengsuanyun-oauth-failed` 事件推送

use crate::shengsuanyun::auth_manager::ShengsuanyunAuthManager;
use crate::shengsuanyun::callback_server::CallbackServer;
use crate::shengsuanyun::models::*;
use std::sync::Arc;
use tauri::State;

pub struct ShengsuanyunState {
    pub manager: Arc<ShengsuanyunAuthManager>,
    /// 活跃的回调服务器（同一时刻至多一个登录流程）
    pub active_callback: tokio::sync::Mutex<Option<CallbackServer>>,
}

impl ShengsuanyunState {
    pub fn new(manager: Arc<ShengsuanyunAuthManager>) -> Self {
        Self {
            manager,
            active_callback: tokio::sync::Mutex::new(None),
        }
    }
}

/// 发起登录：启动 loopback 回调服务器 + 生成 state，返回授权 URL。
/// 前端拿到 URL 后用系统浏览器打开。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_start_login(
    state: State<'_, ShengsuanyunState>,
    target_app: Option<String>,
    target_provider_id: Option<String>,
) -> Result<LoginStart, String> {
    let mut active = state.active_callback.lock().await;
    // 已有登录流程则先终止（端口随旧 server 作废，state 同时失效）
    if let Some(old) = active.take() {
        old.stop();
    }
    let server = CallbackServer::start(state.manager.clone()).await?;
    let port = server.port;
    let start = state
        .manager
        .start_login(port, target_app, target_provider_id)
        .await;
    state.manager.drop_expired().await;
    *active = Some(server);
    log::info!("shengsuanyun oauth started (loopback port {port})");
    Ok(start)
}

/// 取消登录：停回调服务器 + 清 pending state。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_cancel_login(
    state: State<'_, ShengsuanyunState>,
    session_id: String,
) -> Result<bool, String> {
    let mut active = state.active_callback.lock().await;
    if let Some(server) = active.take() {
        server.stop();
    }
    Ok(state.manager.cancel_login(&session_id).await)
}

/// 列出账号（脱敏）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_list_accounts(
    state: State<'_, ShengsuanyunState>,
) -> Result<Vec<ShengsuanyunAccountView>, String> {
    state.manager.list_accounts()
}

/// 查询目标 app/provider 的绑定账号（脱敏）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_get_status(
    state: State<'_, ShengsuanyunState>,
    app_type: String,
    provider_id: String,
) -> Result<Option<ShengsuanyunAccountView>, String> {
    let db = state.manager.db_handle();
    let Some(binding) = db
        .list_shengsuanyun_bindings()?
        .into_iter()
        .find(|b| b.app_type == app_type && b.provider_id == provider_id)
    else {
        return Ok(None);
    };
    let accounts = state.manager.list_accounts()?;
    Ok(accounts.into_iter().find(|a| a.id == binding.account_id))
}

/// 刷新余额（返回元）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_refresh_balance(
    state: State<'_, ShengsuanyunState>,
    account_id: String,
) -> Result<f64, String> {
    state.manager.refresh_balance(&account_id).await
}

/// 登出：清理 Keychain 凭据、DB 账号与绑定。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_logout(
    state: State<'_, ShengsuanyunState>,
    account_id: String,
) -> Result<(), String> {
    state.manager.logout(&account_id)
}

/// 绑定核心逻辑（同步，供命令层与启动对账复用）。
///
/// 行为与 `shengsuanyun_bind_account` 命令一致：查找或创建目标 App 的胜算云
/// Provider，写入凭据；`overwrite=false` 时不覆盖已有不同 Key（返回 conflict）。
///
/// 凭据取自 shengsuanyun_credentials 表（与 CC Switch 一致的自家存储）。
pub fn bind_account_internal(
    app_state: &crate::store::AppState,
    app_type: &str,
    account_id: &str,
    activate: bool,
    overwrite: bool,
) -> Result<ShengsuanyunBindResult, String> {
    {
        let creds =
            crate::shengsuanyun::credential_store::load_credentials(&app_state.db, account_id)?;
        run_bind(
            app_state,
            app_type,
            account_id,
            &creds.api_key,
            activate,
            overwrite,
        )
    }
}

fn run_bind(
    app_state: &crate::store::AppState,
    app_type: &str,
    account_id: &str,
    api_key: &str,
    activate: bool,
    overwrite: bool,
) -> Result<ShengsuanyunBindResult, String> {
    {
        use std::str::FromStr;
        let at = crate::app_config::AppType::from_str(app_type).map_err(|e| e.to_string())?;

        let providers = crate::services::provider::ProviderService::list(app_state, at.clone())
            .map_err(|e| e.to_string())?;
        let (provider_id, mut provider, existing_key) =
            match find_shengsuanyun_provider(providers, &at) {
                Some(found) => found,
                // 查找或创建（幂等）：不存在时按官方 preset 模板创建胜算云 Provider，
                // base URL / 模型用 preset 值，Key 用 OAuth 凭据。
                None => {
                    let created = new_shengsuanyun_provider(&at, api_key);
                    crate::services::provider::ProviderService::add(
                        app_state,
                        at.clone(),
                        created.clone(),
                        false,
                    )
                    .map_err(|e| e.to_string())?;
                    (created.id.clone(), created, String::new())
                }
            };

        if !existing_key.is_empty() && existing_key != api_key && !overwrite {
            return Ok(ShengsuanyunBindResult {
                status: "conflict".into(),
                provider_id: Some(provider_id.clone()),
                account_id: None,
            });
        }
        write_token(&mut provider, &at, api_key)?;
        crate::services::provider::ProviderService::update(
            app_state,
            at.clone(),
            Some(&provider_id),
            provider,
        )
        .map_err(|e| e.to_string())?;

        let now = chrono::Utc::now().timestamp();
        app_state.db.upsert_shengsuanyun_binding(
            at.as_str(),
            &provider_id,
            account_id,
            "oauth",
            now,
        )?;

        if activate {
            crate::services::provider::ProviderService::switch(app_state, at, &provider_id)
                .map_err(|e| e.to_string())?;
        }

        Ok(ShengsuanyunBindResult {
            status: "ok".into(),
            provider_id: Some(provider_id),
            account_id: Some(account_id.to_string()),
        })
    }
}

/// 单 App 绑定命令（前端按钮 / 自动绑定共用）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_bind_account(
    app_handle: tauri::AppHandle,
    app_type: String,
    account_id: String,
    activate: Option<bool>,
    overwrite: Option<bool>,
) -> Result<ShengsuanyunBindResult, String> {
    let activate = activate.unwrap_or(true);
    let overwrite = overwrite.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let app_state = app_handle.state::<crate::store::AppState>();
        bind_account_internal(
            app_state.inner(),
            &app_type,
            &account_id,
            activate,
            overwrite,
        )
    })
    .await
    .map_err(|e| format!("绑定任务执行失败: {e}"))?
}

/// 一键绑定 Claude / Codex / Gemini 三端并激活（登录后无目标 App 上下文时使用）。
///
/// 单端失败不中断其余两端，失败项以 status = "error" 返回。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_bind_all_apps(
    app_handle: tauri::AppHandle,
    account_id: String,
) -> Result<Vec<ShengsuanyunBindResult>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let app_state = app_handle.state::<crate::store::AppState>();
        ["claude", "codex", "gemini"]
            .iter()
            .map(|app| {
                bind_account_internal(app_state.inner(), app, &account_id, true, false).unwrap_or(
                    ShengsuanyunBindResult {
                        status: "error".into(),
                        provider_id: None,
                        account_id: None,
                    },
                )
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("绑定任务执行失败: {e}"))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShengsuanyunBindResult {
    pub status: String,
    pub provider_id: Option<String>,
    pub account_id: Option<String>,
}

/// 在已有 Provider 列表中识别胜算云（按 settingsConfig 中的 base URL）。
fn find_shengsuanyun_provider(
    providers: indexmap::IndexMap<String, crate::provider::Provider>,
    at: &crate::app_config::AppType,
) -> Option<(String, crate::provider::Provider, String)> {
    for (id, p) in providers {
        let cfg = p.settings_config.to_string();
        let is_ssy =
            cfg.contains("router.shengsuanyun.com") || p.name.eq_ignore_ascii_case("shengsuanyun");
        if !is_ssy {
            continue;
        }
        let key = read_token(&p, at);
        return Some((id, p, key));
    }
    None
}

fn token_field_for(at: &crate::app_config::AppType) -> &'static str {
    match at {
        crate::app_config::AppType::Claude => "ANTHROPIC_AUTH_TOKEN",
        crate::app_config::AppType::Codex => "OPENAI_API_KEY",
        _ => "GEMINI_API_KEY",
    }
}

fn read_token(p: &crate::provider::Provider, at: &crate::app_config::AppType) -> String {
    let field = token_field_for(at);
    match at {
        crate::app_config::AppType::Codex => p
            .settings_config
            .pointer(&format!("/auth/{field}"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => p
            .settings_config
            .pointer(&format!("/env/{field}"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

fn write_token(
    p: &mut crate::provider::Provider,
    at: &crate::app_config::AppType,
    api_key: &str,
) -> Result<(), String> {
    let field = token_field_for(at);
    let (container, leaf) = match at {
        crate::app_config::AppType::Codex => ("auth", field),
        _ => ("env", field),
    };
    let root = p
        .settings_config
        .as_object()
        .cloned()
        .ok_or("settingsConfig 格式异常")?;
    let mut root = serde_json::Map::from_iter(root);
    let inner = root
        .entry(container.to_string())
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    if !inner.is_object() {
        return Err("settingsConfig 路径冲突".into());
    }
    inner.as_object_mut().expect("checked above").insert(
        leaf.to_string(),
        serde_json::Value::String(api_key.to_string()),
    );
    p.settings_config = serde_json::Value::Object(root);
    Ok(())
}

/// 按 preset 模板构造目标 App 的胜算云 Provider（与前端 *ProviderPresets.ts 保持一致）。
fn new_shengsuanyun_provider(
    at: &crate::app_config::AppType,
    api_key: &str,
) -> crate::provider::Provider {
    use crate::app_config::AppType;

    let (settings, name) = match at {
        AppType::Claude => (
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://router.shengsuanyun.com/api",
                    "ANTHROPIC_AUTH_TOKEN": api_key,
                    "ANTHROPIC_MODEL": "anthropic/claude-sonnet-5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "anthropic/claude-haiku-4.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "anthropic/claude-sonnet-5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "anthropic/claude-opus-5"
                }
            }),
            "Shengsuanyun",
        ),
        AppType::Codex => (
            serde_json::json!({
                "auth": { "OPENAI_API_KEY": api_key },
                "config": "model_provider = \"custom\"\nmodel = \"openai/gpt-5.6-sol\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n\n[model_providers.custom]\nname = \"shengsuanyun\"\nbase_url = \"https://router.shengsuanyun.com/api/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true"
            }),
            "Shengsuanyun",
        ),
        _ => (
            serde_json::json!({
                "env": {
                    "GOOGLE_GEMINI_BASE_URL": "https://router.shengsuanyun.com/api",
                    "GEMINI_API_KEY": api_key,
                    "GEMINI_MODEL": "google/gemini-3.6-flash"
                }
            }),
            "Shengsuanyun",
        ),
    };
    crate::provider::Provider {
        id: format!("ssy-{}", uuid::Uuid::new_v4()),
        name: name.into(),
        settings_config: settings,
        website_url: Some("https://www.shengsuanyun.com".into()),
        category: Some("aggregator".into()),
        created_at: None,
        sort_index: None,
        notes: None,
        icon: Some("shengsuanyun".into()),
        icon_color: None,
        in_failover_queue: false,
        meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;
    use crate::provider::Provider;

    fn provider(env_key: Option<&str>) -> Provider {
        let mut env = serde_json::Map::new();
        env.insert(
            "ANTHROPIC_BASE_URL".into(),
            "https://router.shengsuanyun.com/api".into(),
        );
        if let Some(k) = env_key {
            env.insert("ANTHROPIC_AUTH_TOKEN".into(), k.into());
        }
        Provider {
            id: "p1".into(),
            name: "Shengsuanyun".into(),
            settings_config: serde_json::json!({ "env": env }),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
            meta: None,
        }
    }

    fn codex_provider() -> Provider {
        Provider {
            id: "c1".into(),
            name: "Shengsuanyun".into(),
            settings_config: serde_json::json!({
                "auth": { "OPENAI_API_KEY": "old" },
                "config": { "base_url": "https://router.shengsuanyun.com/api/v1" }
            }),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
            meta: None,
        }
    }

    #[test]
    fn write_token_claude_and_codex() {
        let mut p = provider(None);
        write_token(&mut p, &AppType::Claude, "sk-1").unwrap();
        assert_eq!(
            p.settings_config
                .pointer("/env/ANTHROPIC_AUTH_TOKEN")
                .unwrap(),
            "sk-1"
        );
        let mut c = codex_provider();
        write_token(&mut c, &AppType::Codex, "sk-2").unwrap();
        assert_eq!(
            c.settings_config.pointer("/auth/OPENAI_API_KEY").unwrap(),
            "sk-2"
        );
        // 其它字段不丢失
        assert!(c.settings_config.pointer("/config/base_url").is_some());
    }

    #[test]
    fn template_carries_key_and_base_url() {
        for at in [AppType::Claude, AppType::Codex, AppType::Gemini] {
            let p = new_shengsuanyun_provider(&at, "sk-x");
            let cfg = p.settings_config.to_string();
            assert!(cfg.contains("router.shengsuanyun.com"), "{at:?}");
            assert!(cfg.contains("sk-x"), "{at:?}");
            // 写入 token 后仍可再定位
            assert!(!read_token(&p, &at).is_empty());
        }
    }

    #[test]
    fn read_token_roundtrip() {
        let p = provider(Some("old"));
        assert_eq!(read_token(&p, &AppType::Claude), "old");
    }
}
