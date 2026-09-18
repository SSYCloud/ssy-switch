//! OAuth loopback 回调服务器。
//!
//! 独立于本地代理：应用启动登录时在 `127.0.0.1:0` 绑定随机端口，
//! 只注册 `GET /auth/shengsuanyun/callback`。不绑定非回环地址、不开 CORS。

use super::auth_manager::ShengsuanyunAuthManager;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;

type HandlerState = (Arc<ShengsuanyunAuthManager>, u16);

pub struct CallbackServer {
    pub port: u16,
    shutdown: oneshot::Sender<()>,
}

impl CallbackServer {
    /// 在 127.0.0.1 随机端口启动回调服务器，返回句柄（含实际端口）。
    pub async fn start(manager: Arc<ShengsuanyunAuthManager>) -> Result<Self, String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("bind loopback callback listener failed: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("get callback port failed: {e}"))?
            .port();

        let app = Router::new()
            .route("/auth/shengsuanyun/callback", get(handle_callback))
            .with_state((manager, port));

        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });

        Ok(Self { port, shutdown: tx })
    }

    /// 停止服务器（登录完成/取消/超时后调用）。
    pub fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

async fn handle_callback(
    State((manager, port)): State<HandlerState>,
    Query(params): Query<HashMap<String, String>>,
) -> Html<String> {
    let (code, state) = match (params.get("code"), params.get("state")) {
        (Some(c), Some(s)) if !c.is_empty() && !s.is_empty() => (c.clone(), s.clone()),
        _ => return Html(error_page("缺少授权参数，请返回 SSY-Switch 重新登录。")),
    };

    let db = manager.db_handle();
    match manager.complete_login(&code, &state, port).await {
        Ok(_) => {
            crate::analytics::track(
                &db,
                "login_callback",
                &serde_json::json!({ "result": "ok" }),
            );
            Html(success_page())
        }
        Err(e) => {
            let class = if e.contains("state") {
                "invalid_state"
            } else if e.contains("token") || e.contains("401") {
                "token_invalid"
            } else {
                "upstream_error"
            };
            crate::analytics::track(
                &db,
                "login_callback",
                &serde_json::json!({ "result": "failed", "reason_class": class }),
            );
            Html(error_page(&format!("登录失败：{e}")))
        }
    }
}

fn page(title: &str, ok: bool, message: &str) -> String {
    let (bg, glyph) = if ok {
        ("#16a34a", "✓")
    } else {
        ("#dc2626", "✕")
    };
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head><meta charset="utf-8"><title>{title}</title>
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#f8fafc;color:#1f2937;font:15px/1.7 -apple-system,sans-serif;}}
.box{{width:min(440px,calc(100vw - 40px));padding:28px 24px;text-align:center;background:#fff;border:1px solid #e5e7eb;border-radius:8px;box-shadow:0 8px 24px rgba(15,23,42,.06);}}
.g{{width:48px;height:48px;margin:0 auto 16px;border-radius:50%;background:{bg};color:#fff;font-size:28px;line-height:48px;}}
h1{{margin:0 0 8px;font-size:20px;font-weight:650;}} p{{margin:8px 0;color:#475467;word-break:break-all;}}</style>
</head>
<body><main class="box">
<div class="g">{glyph}</div><h1>{title}</h1>
<p>{message}</p>
<p>正在返回 SSY-Switch，可关闭本页。</p>
</main>
<script>setTimeout(function(){{window.close();}},1800);</script>
</body></html>"#
    )
}

fn success_page() -> String {
    page("登录成功", true, "已绑定胜算云账号。")
}

fn error_page(message: &str) -> String {
    page("登录失败", false, message)
}
