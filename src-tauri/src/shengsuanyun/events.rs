//! SDK 登录事件 → Tauri event + 埋点。
use crate::analytics;
use crate::database::Database;
use ssy_core::events::LoginEvents;
use ssy_core::models::{OAuthCompletePayload, OAuthFailedPayload};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub struct TauriEvents {
    pub app: AppHandle,
    pub db: Arc<Database>,
}

impl LoginEvents for TauriEvents {
    fn oauth_complete(&self, payload: &OAuthCompletePayload) {
        let _ = self.app.emit("shengsuanyun-oauth-complete", payload);
    }
    fn oauth_failed(&self, session_id: &str, reason: &str) {
        let _ = self.app.emit(
            "shengsuanyun-oauth-failed",
            &OAuthFailedPayload {
                session_id: session_id.into(),
                reason: reason.into(),
            },
        );
    }
    fn login_callback(&self, result_class: &str) {
        analytics::track(
            &self.db,
            "login_callback",
            &serde_json::json!({ "result": result_class }),
        );
    }
}
