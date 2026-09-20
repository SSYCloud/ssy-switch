//! 胜算云 OAuth 的 Tauri commands。
//!
//! 约定：
//! - 所有返回 DTO 均为脱敏数据（无 API Key / code / state）
//! - OAuth 结果通过 `shengsuanyun-oauth-complete` / `shengsuanyun-oauth-failed` 事件推送

use crate::shengsuanyun::{
    ShengsuanyunAccountView, ShengsuanyunAuthManager, ShengsuanyunBindingRow,
};
use ssy_core::callback_server::CallbackServer;
use ssy_core::models::*;
use std::sync::Arc;
use tauri::State;

pub struct ShengsuanyunState {
    pub manager: Arc<ShengsuanyunAuthManager>,
    pub db: Arc<crate::database::Database>,
    /// 活跃的回调服务器（同一时刻至多一个登录流程）
    pub active_callback: tokio::sync::Mutex<Option<CallbackServer>>,
}

impl ShengsuanyunState {
    pub fn new(manager: Arc<ShengsuanyunAuthManager>, db: Arc<crate::database::Database>) -> Self {
        Self {
            manager,
            db,
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
    let db = state.db.clone();
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

/// 充值/账单流水（分页；Asset/Balance 单位 1e-4 元）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_bill_list(
    state: State<'_, ShengsuanyunState>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<serde_json::Value, String> {
    state
        .manager
        .bill_list(
            page.unwrap_or(1).max(1),
            page_size.unwrap_or(10).clamp(1, 50),
        )
        .await
}

/// 多模态调用统计（金额已在命令层换算为元，前端零业务计算）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_modality_usage(
    state: State<'_, ShengsuanyunState>,
    start_date: String,
    end_date: String,
) -> Result<serde_json::Value, String> {
    const AMOUNT_DIVISOR: f64 = 10_000_000.0; // 1e-7 元
    let raw = state.manager.modality_usage(&start_date, &end_date).await?;
    let mut usages = Vec::new();
    if let Some(list) = raw.pointer("/usages").and_then(serde_json::Value::as_array) {
        for u in list {
            let date = u
                .get("date")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let mut details = Vec::new();
            if let Some(ds) = u.get("details").and_then(serde_json::Value::as_array) {
                for d in ds {
                    details.push(serde_json::json!({
                        "model": d.get("model").cloned().unwrap_or_default(),
                        "amount_yuan": d.get("total_amount").and_then(serde_json::Value::as_f64).unwrap_or(0.0)
                            / AMOUNT_DIVISOR,
                    }));
                }
            }
            usages.push(serde_json::json!({ "date": date, "details": details }));
        }
    }
    Ok(serde_json::json!({ "usages": usages }))
}

/// 创建应用内充值订单（返回收款码链接与服务端订单号；金额单位元）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_create_recharge_order(
    state: State<'_, ShengsuanyunState>,
    yuan: f64,
    pay_way: Option<String>,
) -> Result<serde_json::Value, String> {
    let way = pay_way.unwrap_or_else(|| "alipay".to_string());
    let (url, order_id) = state.manager.create_recharge_order(yuan, &way).await?;
    Ok(serde_json::json!({ "url": url, "order_id": order_id }))
}

/// 查询充值订单支付状态（规则层结论：unpaid/paid/unknown）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_pay_status(
    state: State<'_, ShengsuanyunState>,
    order_id: String,
) -> Result<serde_json::Value, String> {
    let status = state.manager.pay_status(&order_id).await;
    Ok(serde_json::json!({ "status": status }))
}

/// 调用统计结论（规则层聚合，金额单位元；模板直接渲染，前端零业务计算）。/// 调用统计结论（规则层聚合，金额单位元；模板直接渲染，前端零业务计算）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_usage_summary(
    state: State<'_, ShengsuanyunState>,
    start_date: String,
    end_date: String,
) -> Result<ssy_core::rules::UsageSummary, String> {
    state
        .manager
        .user_usage(&start_date, &end_date)
        .await
        .map(|raw| ssy_core::rules::summarize_usage(&raw))
}

/// 查询大模型调用记录（按日/按模型聚合；返回原始 JSON，单位 1e-7 元）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_user_usage(
    state: State<'_, ShengsuanyunState>,
    start_date: String,
    end_date: String,
) -> Result<serde_json::Value, String> {
    state.manager.user_usage(&start_date, &end_date).await
}

/// 代金券/体验券明细（voucher_records 金额单位 1e-4 元）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_voucher_list(
    state: State<'_, ShengsuanyunState>,
) -> Result<serde_json::Value, String> {
    state.manager.voucher_list().await
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
        let (api_key, _) = app_state.db.load_shengsuanyun_credentials(account_id)?;
        run_bind(
            app_state, app_type, account_id, &api_key, activate, overwrite,
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

        // 用户此前显式选中的 Key 优先：重登 / 启动对账时不能悄悄改回账号默认 Key。
        let stored_key_id = app_state
            .db
            .get_shengsuanyun_binding(at.as_str(), &provider_id)
            .ok()
            .flatten()
            .and_then(|b| b.key_id);
        let effective_key = match stored_key_id {
            Some(key_id) => resolve_token_plaintext(app_state, account_id, key_id)
                .unwrap_or_else(|| api_key.to_string()),
            None => api_key.to_string(),
        };

        if !existing_key.is_empty() && existing_key != effective_key && !overwrite {
            return Ok(ShengsuanyunBindResult {
                status: "conflict".into(),
                provider_id: Some(provider_id.clone()),
                account_id: None,
            });
        }
        write_token(&mut provider, &at, &effective_key)?;
        crate::services::provider::ProviderService::update(
            app_state,
            at.clone(),
            Some(&provider_id),
            provider,
        )
        .map_err(|e| e.to_string())?;

        let now = chrono::Utc::now().timestamp();
        // key_id 传 None = 保留已记录的选中 Key（COALESCE 语义）
        app_state.db.upsert_shengsuanyun_binding(
            at.as_str(),
            &provider_id,
            account_id,
            "oauth",
            stored_key_id,
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

/// 解析「用户选中的上游 Token」明文。
///
/// 仅在绑定记录里存了 key_id 时才会走到这里（一次网络往返）。
/// 调用方都在 `spawn_blocking` 里，故这里可以安全地 block_on。
fn resolve_token_plaintext(
    app_state: &crate::store::AppState,
    account_id: &str,
    key_id: i64,
) -> Option<String> {
    let creds = app_state
        .db
        .load_shengsuanyun_credentials(account_id)
        .ok()?;
    let client = ssy_core::client::SsyClient::new();
    let identity = if creds.1.is_empty() {
        &creds.0
    } else {
        &creds.1
    };
    let tokens = tauri::async_runtime::block_on(client.list_tokens(identity)).ok()?;
    tokens
        .into_iter()
        .find(|t| t.id == key_id && !t.token.is_empty())
        .map(|t| t.token)
}

/// 绑定视图（下发前端；不含任何凭据）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShengsuanyunBindingView {
    pub app_type: String,
    pub provider_id: String,
    pub account_id: String,
    pub credential_source: String,
    pub key_id: Option<i64>,
    pub updated_at: i64,
}

fn binding_view(row: ShengsuanyunBindingRow) -> ShengsuanyunBindingView {
    ShengsuanyunBindingView {
        app_type: row.app_type,
        provider_id: row.provider_id,
        account_id: row.account_id,
        credential_source: row.credential_source,
        key_id: row.key_id,
        updated_at: row.updated_at,
    }
}

/// 列出某账号名下的全部 API Key（脱敏视图：只有名称、掩码、用量）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_list_keys(
    state: State<'_, ShengsuanyunState>,
    account_id: String,
) -> Result<Vec<SsyTokenView>, String> {
    state.manager.list_tokens(&account_id).await
}

/// 按需取**单把** Key 的明文（用户在下拉里点中某一个时才调用）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_reveal_key(
    state: State<'_, ShengsuanyunState>,
    account_id: String,
    key_id: i64,
) -> Result<String, String> {
    state.manager.reveal_token(&account_id, key_id).await
}

/// 查询目标 app/provider 的绑定详情（含用户选中的 Key ID）。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_get_binding(
    state: State<'_, ShengsuanyunState>,
    app_type: String,
    provider_id: String,
) -> Result<Option<ShengsuanyunBindingView>, String> {
    let db = state.db.clone();
    Ok(db
        .get_shengsuanyun_binding(&app_type, &provider_id)?
        .map(binding_view))
}

/// 记录「该供应商使用哪一把 Key」（仅元数据，不落明文）。
///
/// 绑定不存在时创建一条，避免启动对账把它当成未绑定再改回默认 Key。
#[tauri::command(rename_all = "camelCase")]
pub async fn shengsuanyun_set_binding_key(
    state: State<'_, ShengsuanyunState>,
    app_type: String,
    provider_id: String,
    account_id: String,
    key_id: Option<i64>,
) -> Result<bool, String> {
    let db = state.db.clone();
    let now = chrono::Utc::now().timestamp();
    if db.set_shengsuanyun_binding_key(&app_type, &provider_id, &account_id, key_id, now)? {
        return Ok(true);
    }
    db.upsert_shengsuanyun_binding(&app_type, &provider_id, &account_id, "key", key_id, now)?;
    Ok(true)
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
        SSY_ALL_APPS
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

/// 胜算云一键绑定覆盖的全部宿主（与 AppType 一一对应，共 9 个）
pub const SSY_ALL_APPS: &[&str] = &[
    "claude",
    "claude-desktop",
    "codex",
    "gemini",
    "grokbuild",
    "opencode",
    "openclaw",
    "hermes",
    "pi",
];

/// 各宿主 settingsConfig 中胜算云 Key 的 JSON pointer 路径（与前端 preset 一一对应）。
///
/// GrokBuild 返回 `None`：它的 Key 落在 TOML 文档的 `[model."<profile>"].api_key` 里，
/// 没有 JSON pointer，走 `shengsuanyun::grok_build` 的专用读写。
fn token_pointer(at: &crate::app_config::AppType) -> Option<&'static str> {
    use crate::app_config::AppType;
    match at {
        AppType::Claude | AppType::ClaudeDesktop => Some("/env/ANTHROPIC_AUTH_TOKEN"),
        AppType::Codex => Some("/auth/OPENAI_API_KEY"),
        AppType::GrokBuild => None,
        AppType::Gemini => Some("/env/GEMINI_API_KEY"),
        AppType::OpenCode => Some("/options/apiKey"),
        AppType::OpenClaw => Some("/apiKey"),
        AppType::Hermes => Some("/api_key"),
        AppType::Pi => Some("/apiKey"),
    }
}

fn read_token(p: &crate::provider::Provider, at: &crate::app_config::AppType) -> String {
    use crate::app_config::AppType;
    // Grok CLI 的原生结构：Key 在 config.toml 文本里，没有 JSON pointer。
    if matches!(at, AppType::GrokBuild) {
        return crate::shengsuanyun::grok_build::read_api_key(&p.settings_config);
    }
    token_pointer(at)
        .and_then(|pointer| p.settings_config.pointer(pointer))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn write_token(
    p: &mut crate::provider::Provider,
    at: &crate::app_config::AppType,
    api_key: &str,
) -> Result<(), String> {
    use crate::app_config::AppType;
    // Grok CLI 的原生结构：写进 `[model."<profile>"].api_key`，保留用户其它内容。
    if matches!(at, AppType::GrokBuild) {
        return crate::shengsuanyun::grok_build::write_api_key(&mut p.settings_config, api_key);
    }
    let pointer = token_pointer(at).ok_or("该宿主没有 JSON pointer 形式的 Key 字段")?;
    let segments: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let root_obj = p
        .settings_config
        .as_object()
        .cloned()
        .ok_or("settingsConfig 格式异常")?;
    let mut root = serde_json::Map::from_iter(root_obj);
    // 沿 pointer 逐级确保对象存在
    let mut cur = &mut root;
    for seg in &segments[..segments.len() - 1] {
        let node = cur
            .entry(seg.to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if !node.is_object() {
            return Err("settingsConfig 路径冲突".into());
        }
        cur = node.as_object_mut().expect("checked above");
    }
    cur.insert(
        segments[segments.len() - 1].to_string(),
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

    let settings = match at {
        AppType::Claude | AppType::ClaudeDesktop => serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://router.shengsuanyun.com/api",
                "ANTHROPIC_AUTH_TOKEN": api_key,
                "ANTHROPIC_MODEL": "anthropic/claude-sonnet-5",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "anthropic/claude-haiku-4.5",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "anthropic/claude-sonnet-5",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "anthropic/claude-opus-5"
            }
        }),
        AppType::Codex => serde_json::json!({
            "auth": { "OPENAI_API_KEY": api_key },
            "config": "model_provider = \"custom\"\nmodel = \"openai/gpt-5.6-sol\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n\n[model_providers.custom]\nname = \"shengsuanyun\"\nbase_url = \"https://router.shengsuanyun.com/api/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true"
        }),
        // Grok CLI 不是 Codex：用原生 `[models]` + `[model."<profile>"].api_key`
        // （模板与 Key 写入统一在 shengsuanyun::grok_build，避免两套结构互相污染）。
        AppType::GrokBuild => crate::shengsuanyun::grok_build::build_settings_config(api_key),
        AppType::Gemini => serde_json::json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://router.shengsuanyun.com/api",
                "GEMINI_API_KEY": api_key,
                "GEMINI_MODEL": "google/gemini-3.6-flash"
            }
        }),
        // OpenCode：settingsConfig 根即 provider 定义（npm/options/models）
        AppType::OpenCode => serde_json::json!({
            "npm": "@ai-sdk/anthropic",
            "name": "Shengsuanyun",
            "options": {
                "baseURL": "https://router.shengsuanyun.com/api/v1",
                "apiKey": api_key,
                "setCacheKey": true
            },
            "models": {
                "anthropic/claude-opus-5": { "name": "Claude Opus 5" },
                "anthropic/claude-sonnet-5": { "name": "Claude Sonnet 5" }
            }
        }),
        // OpenClaw / Pi：扁平 { baseUrl, apiKey, api, models }
        AppType::OpenClaw | AppType::Pi => serde_json::json!({
            "name": "Shengsuanyun",
            "baseUrl": "https://router.shengsuanyun.com/api",
            "apiKey": api_key,
            "api": "anthropic-messages",
            "models": [
                { "id": "anthropic/claude-opus-5", "name": "Claude Opus 5" },
                { "id": "anthropic/claude-sonnet-5", "name": "Claude Sonnet 5" }
            ]
        }),
        // Hermes：snake_case { base_url, api_key, api_mode, models }
        AppType::Hermes => serde_json::json!({
            "name": "shengsuanyun",
            "base_url": "https://router.shengsuanyun.com/api/v1",
            "api_key": api_key,
            "api_mode": "chat_completions",
            "models": [ { "id": "openai/gpt-5.6-sol", "name": "GPT-5.6 Sol" } ]
        }),
    };
    let name = "Shengsuanyun";
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
        meta: Some(crate::shengsuanyun::usage_script::default_usage_script_meta()),
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
    fn template_carries_key_and_base_url_for_all_hosts() {
        use std::str::FromStr;
        for app in SSY_ALL_APPS {
            let at = AppType::from_str(app).expect("valid app");
            let p = new_shengsuanyun_provider(&at, "sk-x");
            let cfg = p.settings_config.to_string();
            assert!(cfg.contains("router.shengsuanyun.com"), "{app}");
            assert!(cfg.contains("sk-x"), "{app}");
            // 模板自带的 Key 可通过 pointer 读回（写入路径正确性验证）
            assert_eq!(read_token(&p, &at), "sk-x", "{app}");
            // 出厂自带用量脚本（启用，5 分钟自动查询）
            let usage = p.meta.expect("meta").usage_script.expect("usage");
            assert!(usage.enabled, "{app}");
            assert!(usage.code.contains("{{shengsuanyunJwt}}"), "{app}");
        }
    }

    #[test]
    fn read_token_roundtrip() {
        let p = provider(Some("old"));
        assert_eq!(read_token(&p, &AppType::Claude), "old");
    }
}
