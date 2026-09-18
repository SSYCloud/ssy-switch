//! 登录事件 SPI：SDK 把结果推给宿主，宿主决定如何呈现（Tauri event / 回调 / 埋点）。
//!
//! 隐私红线：payload 均为脱敏数据；`reason`/`result_class` 不含 code 等敏感参数。

use crate::models::OAuthCompletePayload;

pub trait LoginEvents: Send + Sync {
    fn oauth_complete(&self, _payload: &OAuthCompletePayload) {}
    fn oauth_failed(&self, _session_id: &str, _reason: &str) {}
    /// 回调处理结果分类：`ok` / `invalid_state` / `token_invalid` / `upstream_error`
    fn login_callback(&self, _result_class: &str) {}
}

/// 无操作实现（不需要事件通知的消费方）
pub struct NoopEvents;
impl LoginEvents for NoopEvents {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(AtomicUsize);
    impl LoginEvents for Counting {
        fn oauth_complete(&self, _p: &OAuthCompletePayload) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn events_dispatch_through_trait() {
        let c = Counting(AtomicUsize::new(0));
        c.oauth_complete(&OAuthCompletePayload {
            account: crate::models::AccountView {
                id: String::new(),
                uid_masked: String::new(),
                display_name: String::new(),
                email_masked: String::new(),
                avatar_url: String::new(),
                is_creator: false,
                balance_yuan: None,
                voucher_yuan: None,
                balance_updated_at: None,
                created_at: 0,
            },
            app_type: None,
            provider_id: None,
        });
        assert_eq!(c.0.load(Ordering::SeqCst), 1);
    }
}
