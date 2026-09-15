//! 胜算云 OAuth 认证管理器。
//!
//! 职责：
//! - 管理 pending 登录会话（state + TTL + 一次性消费，防 CSRF/重放）
//! - 登录完成后：code 换 Key → 查用户信息 → 探创作者角色 → 存 Keychain + SQLite
//! - 账号生命周期：列出 / 刷新余额 / 登出（清理 Keychain + DB + 绑定）
//!
//! 安全约束：
//! - API Key 只进 Keychain；DB、事件、日志、DTO 均为脱敏数据
//! - state 为 UUID v4，5 分钟过期，验证时原子取出（一次性）

use super::client::SsyClient;
use super::credential_store as creds;
use super::models::*;
use crate::database::Database;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

pub struct ShengsuanyunAuthManager {
    client: Arc<SsyClient>,
    db: Arc<Database>,
    pending: Mutex<HashMap<String, PendingLogin>>,
    /// 登录结果事件通过它推给前端（setup 阶段注入）
    app_handle: std::sync::Mutex<Option<AppHandle>>,
}

struct PendingLogin {
    callback_port: u16,
    target_app: Option<String>,
    target_provider_id: Option<String>,
    created_at: i64,
}

impl ShengsuanyunAuthManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            client: Arc::new(SsyClient::new()),
            db,
            pending: Mutex::new(HashMap::new()),
            app_handle: std::sync::Mutex::new(None),
        }
    }

    pub fn db_handle(&self) -> Arc<Database> {
        self.db.clone()
    }

    pub fn set_app_handle(&self, app: AppHandle) {
        *self.app_handle.lock().expect("app_handle poisoned") = Some(app);
    }

    fn emit(&self, event: &str, payload: &impl serde::Serialize) {
        if let Some(app) = self
            .app_handle
            .lock()
            .expect("app_handle poisoned")
            .as_ref()
        {
            let _ = app.emit(event, payload);
        }
    }

    pub fn client(&self) -> &Arc<SsyClient> {
        &self.client
    }

    /// 启动登录：生成 state，登记 pending 会话，返回授权 URL 与会话 ID。
    pub async fn start_login(
        &self,
        callback_port: u16,
        target_app: Option<String>,
        target_provider_id: Option<String>,
    ) -> LoginStart {
        let state = uuid::Uuid::new_v4().to_string();
        let callback_url = format!("http://127.0.0.1:{callback_port}/auth/shengsuanyun/callback");
        let authorization_url = SsyClient::build_authorization_url(&callback_url, &state);
        self.pending.lock().await.insert(
            state.clone(),
            PendingLogin {
                callback_port,
                target_app,
                target_provider_id,
                created_at: now_ts(),
            },
        );
        LoginStart {
            authorization_url,
            session_id: state,
        }
    }

    /// 取消登录：清理 pending 会话。
    pub async fn cancel_login(&self, session_id: &str) -> bool {
        self.pending.lock().await.remove(session_id).is_some()
    }

    /// 清理所有过期 pending 会话（由新的 start_login 顺带触发即可，这里显式提供）。
    pub async fn drop_expired(&self) {
        let cutoff = now_ts() - PENDING_STATE_TTL_SECS;
        self.pending
            .lock()
            .await
            .retain(|_, p| p.created_at > cutoff);
    }

    /// 消费 state（一次性）：成功返回该会话上下文，失败返回 None。
    /// 同时校验回调端口与登记端口一致，防止跨会话重放。
    pub async fn consume_state(
        &self,
        state: &str,
        callback_port: u16,
    ) -> Option<(Option<String>, Option<String>)> {
        let mut pending = self.pending.lock().await;
        let p = pending.get(state)?;
        // 先校验再消费：端口不符的请求不消耗 state（仍可能是合法重试）
        if p.created_at <= now_ts() - PENDING_STATE_TTL_SECS || p.callback_port != callback_port {
            return None;
        }
        let p = pending.remove(state)?;
        Some((p.target_app, p.target_provider_id))
    }

    /// OAuth 回调核心路径：code 换 Key → 拉资料 → 落库（Key 进 Keychain）。
    /// 返回脱敏 payload；任何一步失败都不会写入账号。
    pub async fn complete_login(
        &self,
        code: &str,
        state: &str,
        callback_port: u16,
    ) -> Result<OAuthCompletePayload, String> {
        let Some((target_app, target_provider_id)) = self.consume_state(state, callback_port).await
        else {
            return Err("invalid, expired, or already-used login state".into());
        };
        // 用 pending 登记的回调端口重建 callback_url（与授权时一致）
        let callback_url = format!("http://127.0.0.1:{callback_port}/auth/shengsuanyun/callback");

        let api_key = self.client.exchange_code(code, &callback_url).await?;
        let info = self.client.fetch_user_info(&api_key).await?;
        let is_creator = self.client.detect_creator_role(&api_key).await;

        let id = uuid::Uuid::new_v4().to_string();
        creds::save_api_key(&id, &api_key)?;
        let now = now_ts();
        // 以 uid 为幂等键：同一胜算云账号重复登录时覆盖旧记录（含旧 Key）
        let row = self.db.upsert_shengsuanyun_account(
            &id,
            &info,
            is_creator,
            Some(info.wallet_assets),
            now,
        )?;
        let view = account_view(&row);
        let payload = OAuthCompletePayload {
            account: view,
            app_type: target_app,
            provider_id: target_provider_id,
        };
        self.emit("shengsuanyun-oauth-complete", &payload);
        Ok(payload)
    }

    /// 回调失败时推送失败事件（payload 不含 code 等敏感参数）。
    pub async fn notify_login_failed(&self, session_id: &str, reason: &str) {
        self.emit(
            "shengsuanyun-oauth-failed",
            &OAuthFailedPayload {
                session_id: session_id.to_string(),
                reason: reason.to_string(),
            },
        );
    }

    /// 列出账号（脱敏 DTO）。
    pub fn list_accounts(&self) -> Result<Vec<ShengsuanyunAccountView>, String> {
        Ok(self
            .db
            .list_shengsuanyun_accounts()?
            .iter()
            .map(account_view)
            .collect())
    }

    /// 刷新余额（写缓存，返回元）。
    pub async fn refresh_balance(&self, account_id: &str) -> Result<f64, String> {
        let api_key = creds::load_api_key(account_id)?;
        let info = self.client.fetch_user_info(&api_key).await?;
        self.db
            .update_shengsuanyun_balance(account_id, info.wallet_assets, now_ts())?;
        Ok(assets_to_yuan(info.wallet_assets))
    }

    /// 登出：删除 Keychain 凭据 + DB 账号 + 绑定记录。幂等。
    pub fn logout(&self, account_id: &str) -> Result<(), String> {
        creds::delete_api_key(account_id)?;
        self.db.delete_shengsuanyun_account(account_id)?;
        Ok(())
    }

    /// 读取某账号的 API Key（仅供 Provider 写入 live config 使用，不对外暴露给前端）。
    pub fn api_key_for(&self, account_id: &str) -> Result<String, String> {
        creds::load_api_key(account_id)
    }
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn account_view(row: &ShengsuanyunAccountRow) -> ShengsuanyunAccountView {
    ShengsuanyunAccountView {
        id: row.id.clone(),
        uid_masked: mask_uid(&row.uid),
        display_name: row.display_name.clone(),
        email_masked: mask_email(&row.email),
        avatar_url: row.avatar_url.clone(),
        is_creator: row.is_creator,
        balance_yuan: row.balance_assets.map(assets_to_yuan),
        balance_updated_at: row.balance_updated_at,
        created_at: row.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn state_is_single_use_and_port_bound() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let mgr = ShengsuanyunAuthManager::new(db);
        mgr.start_login(8090, None, None).await;

        let snapshot: Vec<String> = mgr.pending.lock().await.keys().cloned().collect();
        let state = snapshot[0].clone();

        // 端口不一致 → 拒绝且不消费
        assert!(mgr.consume_state(&state, 9999).await.is_none());
        // 端口一致 → 消费成功，第二次失败
        assert!(mgr.consume_state(&state, 8090).await.is_some());
        assert!(mgr.consume_state(&state, 8090).await.is_none());
    }

    #[tokio::test]
    async fn unknown_state_rejected() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let mgr = ShengsuanyunAuthManager::new(db);
        assert!(mgr.consume_state("nope", 8090).await.is_none());
        // complete_login 也不会写账号
        let err = mgr.complete_login("code", "nope", 8090).await;
        assert!(err.is_err());
        assert!(mgr.list_accounts().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancel_login_clears_state() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let mgr = ShengsuanyunAuthManager::new(db);
        let start = mgr.start_login(8090, None, None).await;
        assert!(mgr.cancel_login(&start.session_id).await);
        assert!(!mgr.cancel_login(&start.session_id).await);
        assert!(mgr.consume_state(&start.session_id, 8090).await.is_none());
    }
}
