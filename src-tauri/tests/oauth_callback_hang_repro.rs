//! 复现 2026-09-20 的 OAuth 回调挂死：
//! 现象——login_started 有埋点、无 login_callback、账号未落库、
//! Chrome 到回调端口的连接长期 ESTABLISHED（无 HTTP 响应）。
//!
//! 本测试在干净运行时里走完整链路（真实回调服务器 + 真实上游 HTTP，
//! code 为假值，上游应快速拒绝），验证回调是否能返回任何页面。
//! 若超时则复现挂死。

use std::sync::Arc;
use std::time::Duration;

use ssy_core::callback_server::CallbackServer;
use ssy_core::{AuthManager, NoopEvents};
use ssy_switch_lib::shengsuanyun::sqlite_stores::{SqliteAccountStore, SqliteCredentialStore};
use ssy_switch_lib::Database;

type Manager = AuthManager<SqliteCredentialStore, SqliteAccountStore, NoopEvents>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oauth_callback_returns_page_within_timeout() {
    let db = Arc::new(Database::memory().expect("memory db"));
    let manager: Manager = AuthManager::new(
        SqliteCredentialStore(db.clone()),
        SqliteAccountStore(db),
        NoopEvents,
    );
    let flow = Arc::new(manager);
    let server = CallbackServer::start(flow.clone()).await.expect("server");
    let start = flow.start_login(server.port, None, None).await;

    let url = format!(
        "http://127.0.0.1:{port}/auth/shengsuanyun/callback?code=fake-code-for-repro&state={state}",
        port = server.port,
        state = start.session_id,
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        // 本测试访问的是 127.0.0.1 回调端口，必须绕过系统代理
        // （macOS 系统代理开启时，reqwest 会把 loopback 请求也发给代理 → 502）
        .no_proxy()
        .build()
        .expect("client");
    // 整体留足上游往返 + 30s 请求超时的余量
    let resp = tokio::time::timeout(Duration::from_secs(45), client.get(&url).send()).await;

    match resp {
        Err(_) => panic!("REPRODUCED: callback handler did not respond within 45s (hang)"),
        Ok(Err(e)) => panic!("request failed: {e}"),
        Ok(Ok(resp)) => {
            let status = resp.status();
            let headers = format!("{:?}", resp.headers());
            let body = resp.text().await.expect("body");
            eprintln!("status={status} headers={headers} body_len={}", body.len());
            eprintln!("body={body}");
            assert!(
                body.contains("登录失败") || body.contains("登录成功"),
                "unexpected page: {body}"
            );
        }
    }
}
