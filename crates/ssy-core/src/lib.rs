//! # ssy-core
//!
//! 胜算云开放平台核心 SDK：OAuth（Authorization Code + loopback 回调）、
//! 账号/双凭据契约、钱包余额与调用数据查询。
//!
//! - 契约规范：仓库 `openapi/ssy-api.yaml`（本 crate client 与其 1:1 对应）
//! - 设计原则：薄层 1:1 对应 REST；持久化与事件由宿主通过
//!   [`credential::CredentialStore`]、[`store::AccountStore`]、[`events::LoginEvents`]
//!   三个 SPI 注入；**不含任何表现层**
//!
//! ```no_run
//! # async fn example() -> Result<(), String> {
//! use ssy_core::prelude::*;
//! # #[derive(Clone, Copy, Default)]
//! # struct MyStore;
//! # impl CredentialStore for MyStore {
//! #     fn save(&self, _: &str, _: &StoredCredentials) -> Result<(), String> { Ok(()) }
//! #     fn load(&self, _: &str) -> Result<StoredCredentials, String> { Ok(Default::default()) }
//! #     fn delete(&self, _: &str) -> Result<(), String> { Ok(()) }
//! # }
//! # impl AccountStore for MyStore {
//! #     fn find_account_id_by_uid(&self, _: &str) -> Result<Option<String>, String> { Ok(None) }
//! #     fn upsert_account(&self, _: AccountUpsert) -> Result<AccountRecord, String> { unimplemented!() }
//! #     fn list_accounts(&self) -> Result<Vec<AccountRecord>, String> { Ok(vec![]) }
//! #     fn update_balance(&self, _: &str, _: f64, _: f64, _: i64) -> Result<(), String> { Ok(()) }
//! #     fn list_empty_uid_account_ids(&self) -> Result<Vec<String>, String> { Ok(vec![]) }
//! #     fn update_account_identity(&self, _: &str, _: &SsyUserInfo, _: i64) -> Result<(), String> { Ok(()) }
//! #     fn delete_account(&self, _: &str) -> Result<(), String> { Ok(()) }
//! # }
//! let mgr = AuthManager::new(MyStore, MyStore, NoopEvents);
//! let start = mgr.start_login(0, None, None).await; // 实际使用时传回调端口
//! let _ = start.authorization_url; // 用系统浏览器打开
//! # Ok(())
//! # }
//! ```

pub mod auth_manager;
#[cfg(feature = "callback-server")]
pub mod callback_server;
pub mod client;
pub mod credential;
pub mod events;
pub mod memory;
pub mod models;
pub mod store;

pub use auth_manager::AuthManager;
#[cfg(feature = "callback-server")]
pub use callback_server::{CallbackServer, OAuthFlow};
pub use credential::{CredentialStore, StoredCredentials};
pub use events::{LoginEvents, NoopEvents};
pub use memory::{InMemoryAccountStore, InMemoryCredentialStore};
pub use models::{
    account_view, assets_to_yuan, mask_email, mask_uid, AccountRecord, AccountView, LoginStart,
    OAuthCompletePayload, OAuthFailedPayload, SsyCredentials, SsyToken, SsyTokenView, SsyUserInfo,
    PENDING_STATE_TTL_SECS, SSY_API_BASE, SSY_AUTH_URL, SSY_LOOM_BASE,
};
pub use store::{AccountStore, AccountUpsert};

/// 常用类型与 trait 的 prelude
pub mod prelude {
    pub use crate::auth_manager::AuthManager;
    #[cfg(feature = "callback-server")]
    pub use crate::callback_server::{CallbackServer, OAuthFlow};
    pub use crate::credential::{CredentialStore, StoredCredentials};
    pub use crate::events::{LoginEvents, NoopEvents};
    pub use crate::models::*;
    pub use crate::store::{AccountStore, AccountUpsert};
}
