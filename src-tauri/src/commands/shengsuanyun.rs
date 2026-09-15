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
