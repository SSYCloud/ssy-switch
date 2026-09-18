//! 胜算云模块（SSY-Switch 壳层适配）。
//!
//! 核心逻辑在 `ssy-core` crate（workspace 成员）；本模块提供：
//! - SQLite 存储适配（实现 SDK 的 CredentialStore / AccountStore）
//! - Tauri 事件适配（实现 SDK 的 LoginEvents）
//! - 应用专属：默认用量脚本、Provider 绑定、Grok Build 配置

pub mod events;
pub mod grok_build;
pub mod sqlite_stores;
pub mod usage_script;

// 兼容旧引用路径：`shengsuanyun::client::SsyClient`
pub use crate::database::dao::shengsuanyun::ShengsuanyunBindingRow;
pub use ssy_core::models::AccountView as ShengsuanyunAccountView;

pub use ssy_core::AuthManager;

/// 壳层使用的具体管理器类型
pub type ShengsuanyunAuthManager = AuthManager<
    sqlite_stores::SqliteCredentialStore,
    sqlite_stores::SqliteAccountStore,
    events::TauriEvents,
>;
