//! 胜算云 OAuth 认证管理器（泛型实现）。
//!
//! - `S: CredentialStore` —— 双凭据存取（宿主决定存哪里）
//! - `A: AccountStore` —— 账号元数据 CRUD（宿主决定存哪里）
//! - `E: LoginEvents` —— 结果事件（宿主决定如何呈现）
//!
//! 安全约束：state 为 UUID v4、5 分钟 TTL、一次性消费且绑定回调端口；
//! `amounts`/凭据/敏感参数永不出现在事件或错误消息。

use crate::client::SsyClient;
use crate::credential::CredentialStore;
use crate::events::LoginEvents;
use crate::models::*;
use crate::store::{AccountStore, AccountUpsert};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 胜算云认证管理器（所有方法 `&self`，内部细粒度锁）。
pub struct AuthManager<S, A, E>
where
    S: CredentialStore,
    A: AccountStore,
    E: LoginEvents,
{
    client: Arc<SsyClient>,
    credentials: S,
    accounts: A,
    events: E,
    pending: Mutex<HashMap<String, PendingLogin>>,
}

struct PendingLogin {
    callback_port: u16,
    target_app: Option<String>,
    target_provider_id: Option<String>,
    created_at: i64,
}

impl<S, A, E> AuthManager<S, A, E>
where
    S: CredentialStore,
    A: AccountStore,
    E: LoginEvents,
{
    pub fn new(credentials: S, accounts: A, events: E) -> Self {
        Self {
            client: Arc::new(SsyClient::new()),
            credentials,
            accounts,
            events,
            pending: Mutex::new(HashMap::new()),
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

    /// 清理所有过期 pending 会话。
    pub async fn drop_expired(&self) {
        let cutoff = now_ts() - PENDING_STATE_TTL_SECS;
        self.pending
            .lock()
            .await
            .retain(|_, p| p.created_at > cutoff);
    }

    /// 消费 state（一次性）：校验 TTL 与回调端口后原子取出。
    pub async fn consume_state(
        &self,
        state: &str,
        callback_port: u16,
    ) -> Option<(Option<String>, Option<String>)> {
        let mut pending = self.pending.lock().await;
        let p = pending.get(state)?;
        if p.created_at <= now_ts() - PENDING_STATE_TTL_SECS || p.callback_port != callback_port {
            return None;
        }
        let p = pending.remove(state)?;
        Some((p.target_app, p.target_provider_id))
    }

    /// OAuth 回调核心路径：code 换凭据 → 拉资料 → 存凭据 → upsert 账号。
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
        let callback_url = format!("http://127.0.0.1:{callback_port}/auth/shengsuanyun/callback");

        let credentials = self.client.exchange_code(code, &callback_url).await?;
        let info = self
            .client
            .fetch_user_info(credentials.identity_token())
            .await?;
        let is_creator = self.client.detect_creator_role(&credentials.api_key).await;

        // 以 uid 为幂等键：同账号重复登录复用同一本地 id，绑定记录不被级联删除。
        let id = self
            .accounts
            .find_account_id_by_uid(&info.uid)?
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.credentials.save(
            &id,
            &crate::credential::StoredCredentials {
                api_key: credentials.api_key.clone(),
                jwt_token: credentials.jwt_token.clone(),
            },
        )?;
        let now = now_ts();
        let row = self.accounts.upsert_account(AccountUpsert {
            id: &id,
            info: &info,
            is_creator,
            balance_assets: Some(info.wallet_assets),
            voucher_assets: Some(info.voucher_assets),
            now,
        })?;
        let payload = OAuthCompletePayload {
            account: crate::models::account_view(&row),
            app_type: target_app,
            provider_id: target_provider_id,
        };
        self.events.oauth_complete(&payload);
        Ok(payload)
    }

    /// 回调失败事件（payload 不含 code 等敏感参数）。
    pub fn notify_login_failed(&self, session_id: &str, reason: &str) {
        self.events.oauth_failed(session_id, reason);
    }

    /// 列出账号（脱敏 DTO）。
    pub fn list_accounts(&self) -> Result<Vec<AccountView>, String> {
        Ok(self
            .accounts
            .list_accounts()?
            .iter()
            .map(crate::models::account_view)
            .collect())
    }

    /// 刷新余额（写缓存，返回元）。
    pub async fn refresh_balance(&self, account_id: &str) -> Result<f64, String> {
        let credentials = self.credentials.load(account_id)?;
        let info = self
            .client
            .fetch_user_info(credentials.identity_token())
            .await?;
        self.accounts.update_balance(
            account_id,
            info.wallet_assets,
            info.voucher_assets,
            now_ts(),
        )?;
        Ok(assets_to_yuan(info.wallet_assets))
    }

    /// 回填历史空 uid 账号的 uid（只回填，绝不删除）。
    pub async fn backfill_empty_uid_accounts(&self) -> Result<usize, String> {
        let ids = self.accounts.list_empty_uid_account_ids()?;
        let mut filled = 0usize;
        for id in ids {
            let credentials = match self.credentials.load(&id) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("账号 {id} 凭据读取失败，跳过 uid 回填: {e}");
                    continue;
                }
            };
            match self
                .client
                .fetch_user_info(credentials.identity_token())
                .await
            {
                Ok(info) if !info.uid.is_empty() => {
                    match self.accounts.update_account_identity(&id, &info, now_ts()) {
                        Ok(()) => {
                            filled += 1;
                            log::info!("已回填账号 {id} 的 uid");
                        }
                        Err(e) => log::warn!("账号 {id} uid 回填写库失败: {e}"),
                    }
                }
                Ok(_) => log::warn!("账号 {id} 上游仍未返回 uid，保留本地记录不删"),
                Err(e) => log::warn!("账号 {id} 查询 /user/info 失败: {e}"),
            }
        }
        Ok(filled)
    }

    fn first_account_credentials(&self) -> Result<crate::credential::StoredCredentials, String> {
        let account = self
            .accounts
            .list_accounts()?
            .first()
            .cloned()
            .ok_or_else(|| "not logged in".to_string())?;
        self.credentials.load(&account.id)
    }

    /// 大模型调用统计（按日/按模型，total_amount 单位 1e-7 元）。
    pub async fn user_usage(&self, start_date: &str, end_date: &str) -> Result<Value, String> {
        let credentials = self.first_account_credentials()?;
        self.client
            .fetch_user_usage(start_date, end_date, credentials.identity_token())
            .await
    }

    /// 充值/账单流水（分页）。
    pub async fn bill_list(&self, page: i64, page_size: i64) -> Result<Value, String> {
        let credentials = self.first_account_credentials()?;
        self.client
            .fetch_bill_list(page, page_size, credentials.identity_token())
            .await
    }

    /// 多模态调用统计。
    pub async fn modality_usage(&self, start_date: &str, end_date: &str) -> Result<Value, String> {
        let credentials = self.first_account_credentials()?;
        self.client
            .fetch_modality_usage(start_date, end_date, credentials.identity_token())
            .await
    }

    /// 代金券/体验券明细（1e-4 元）。
    pub async fn voucher_list(&self) -> Result<Value, String> {
        let credentials = self.first_account_credentials()?;
        self.client
            .fetch_voucher_list(credentials.identity_token())
            .await
    }

    /// 登出：删除凭据 + 账号（级联绑定）。幂等。
    pub fn logout(&self, account_id: &str) -> Result<(), String> {
        self.credentials.delete(account_id)?;
        self.accounts.delete_account(account_id)
    }

    /// 读取某账号的 API Key（宿主写入 live 配置用；不下发前端）。
    pub fn api_key_for(&self, account_id: &str) -> Result<String, String> {
        Ok(self.credentials.load(account_id)?.api_key)
    }

    /// 账号名下全部 Token（脱敏视图）。
    pub async fn list_tokens(&self, account_id: &str) -> Result<Vec<SsyTokenView>, String> {
        let credentials = self.credentials.load(account_id)?;
        let tokens = self
            .client
            .list_tokens(credentials.identity_token())
            .await?;
        Ok(tokens.iter().map(SsyToken::view).collect())
    }

    /// 按 Token ID 取单把明文 Key（用户显式选中时才调用）。
    pub async fn reveal_token(&self, account_id: &str, key_id: i64) -> Result<String, String> {
        let credentials = self.credentials.load(account_id)?;
        let tokens = self
            .client
            .list_tokens(credentials.identity_token())
            .await?;
        tokens
            .into_iter()
            .find(|t| t.id == key_id && !t.token.is_empty())
            .map(|t| t.token)
            .ok_or_else(|| format!("未找到 ID 为 {key_id} 的 API Key"))
    }
}

/// [`AuthManager`] 天然满足回调处理契约（callback-server feature 用）。
impl<S, A, E> crate::callback_server::OAuthFlow for AuthManager<S, A, E>
where
    S: CredentialStore + 'static,
    A: AccountStore + 'static,
    E: LoginEvents + 'static,
{
    async fn complete_login(
        &self,
        code: &str,
        state: &str,
        callback_port: u16,
    ) -> Result<OAuthCompletePayload, String> {
        Self::complete_login(self, code, state, callback_port).await
    }

    fn notify_login_failed(&self, session_id: &str, reason: &str) {
        Self::notify_login_failed(self, session_id, reason)
    }

    fn login_callback(&self, result_class: &str) {
        self.events.login_callback(result_class)
    }
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}
