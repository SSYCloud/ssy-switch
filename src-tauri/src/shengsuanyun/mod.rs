//! 胜算云（Shengsuanyun）OAuth 与账号管理模块。
//!
//! Authorization Code + loopback HTTP 回调模式（参考 LoomLoom webapp 的 auth_ssy.go），
//! 与上游 CC Switch 现有三家 OAuth（Device Code 模式）相互独立。

pub mod auth_manager;
pub mod callback_server;
pub mod client;
pub mod credential_store;
pub mod models;

pub use auth_manager::ShengsuanyunAuthManager;
