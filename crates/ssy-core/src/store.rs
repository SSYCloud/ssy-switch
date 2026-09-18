//! 账号元数据存储契约（SDK 只定义“存什么”，持久化由宿主实现）。

use crate::models::{AccountRecord, SsyUserInfo};

/// 新账号 upsert 入参
pub struct AccountUpsert<'a> {
    pub id: &'a str,
    pub info: &'a SsyUserInfo,
    pub is_creator: bool,
    pub balance_assets: Option<f64>,
    pub voucher_assets: Option<f64>,
    pub now: i64,
}

/// 账号元数据存储 SPI。
///
/// 实现约束（2026-09-16 事故复盘）：
/// - `uid` 为空是合法历史状态，**不得**作为删除键
/// - 同 uid 重复登录时复用既有账号 id（`find_account_id_by_uid`）
pub trait AccountStore: Send + Sync {
    /// 同 uid 已存在的账号 id（uid 为空时必须返回 None）
    fn find_account_id_by_uid(&self, uid: &str) -> Result<Option<String>, String>;
    /// 插入或覆盖账号，返回落库后的记录
    fn upsert_account(&self, upsert: AccountUpsert) -> Result<AccountRecord, String>;
    fn list_accounts(&self) -> Result<Vec<AccountRecord>, String>;
    /// 余额/体验券缓存更新（附带 updated_at）
    fn update_balance(&self, id: &str, assets: f64, voucher: f64, ts: i64) -> Result<(), String>;
    fn list_empty_uid_account_ids(&self) -> Result<Vec<String>, String>;
    /// 回填账号身份字段（uid/昵称/邮箱等），不改凭据
    fn update_account_identity(&self, id: &str, info: &SsyUserInfo, ts: i64) -> Result<(), String>;
    /// 删除账号；实现方须级联删除该账号的绑定记录
    fn delete_account(&self, id: &str) -> Result<(), String>;
}
