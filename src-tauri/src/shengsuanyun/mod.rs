//! 胜算云（Shengsuanyun）OAuth 与账号管理模块。
//!
//! Authorization Code + loopback HTTP 回调模式（参考 LoomLoom webapp 的 auth_ssy.go），
//! 与上游 CC Switch 现有三家 OAuth（Device Code 模式）相互独立。

pub mod auth_manager;
pub mod callback_server;
pub mod client;
pub mod credential_store;
// SSY-Switch: Grok CLI 是原生 `[models]` 结构，与 Codex 的 `[model_providers]` 不同，
// 单独一个模块承载它的模板与 Key 读写（唯一转换入口）。
pub mod grok_build;
pub mod models;

pub use auth_manager::ShengsuanyunAuthManager;
