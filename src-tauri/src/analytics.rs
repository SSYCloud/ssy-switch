//! 埋点：事件采集与批量上报。
//!
//! - 事件字典见 `docs/proposal-compliance.md` 与立项报告 §九
//! - 上报端点：settings `analytics_endpoint`；为空 = 仅本地模式（默认），
//!   后端接收端点就绪后填入 URL 即启用，客户端零改动
//! - 上报 payload：`{ install_id, events: [{ts, event, props}] }`
//! - 隐私红线：不携带 API Key / jwt / code / 邮箱 / 完整 uid / 提示词

use crate::database::Database;
use serde_json::{json, Value};

/// 本地模式队列上限（超出裁剪最旧的）
const LOCAL_QUEUE_CAP: i64 = 500;
/// 每批上报条数
const FLUSH_BATCH: i64 = 50;

pub fn install_id(db: &Database) -> String {
    db.analytics_install_id().unwrap_or_default()
}

/// 记录事件（入队；本地模式下顺带裁剪队列）
pub fn track(db: &Database, event: &str, props: &Value) {
    if let Err(e) = db.analytics_track(event, props) {
        log::debug!("analytics track failed: {e}");
    }
    if let Err(e) = db.analytics_trim(LOCAL_QUEUE_CAP) {
        log::debug!("analytics trim failed: {e}");
    }
}

/// 单次批量上报尝试。返回是否成功（成功则事件已标记 flushed）。
pub async fn flush_once(db: &Database, endpoint: &str) -> Result<usize, String> {
    if endpoint.is_empty() {
        return Ok(0);
    }
    let install = install_id(db);
    let batch = db.analytics_unflushed(FLUSH_BATCH)?;
    if batch.is_empty() {
        return Ok(0);
    }
    let ids: Vec<i64> = batch.iter().map(|(id, _, _)| *id).collect();
    let events: Vec<Value> = batch
        .iter()
        .map(|(_, _, payload)| payload.clone())
        .collect();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(endpoint)
        .json(&json!({ "install_id": install, "events": events }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("analytics endpoint HTTP {}", resp.status()));
    }
    db.analytics_mark_flushed(&ids)?;
    Ok(ids.len())
}

/// 周期性 flush worker（lib.rs setup 时启动）
pub async fn flush_worker(db: std::sync::Arc<Database>) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        ticker.tick().await;
        let endpoint = db
            .get_setting("analytics_endpoint")
            .ok()
            .flatten()
            .unwrap_or_default();
        if endpoint.is_empty() {
            continue;
        }
        // 每轮最多清 4 批，防止积压时无限占用
        for _ in 0..4 {
            match flush_once(&db, &endpoint).await {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) => {
                    log::debug!("analytics flush failed: {e}");
                    break;
                }
            }
        }
    }
}
