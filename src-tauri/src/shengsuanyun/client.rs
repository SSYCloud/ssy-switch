//! 胜算云上游 API client。
//!
//! 协议参考 `参考/auth_ssy.go`（LoomLoom webapp）：
//! - `POST /auth/keys` 用 OAuth code 换 api_key（响应三层兜底）
//! - `GET /user/info` 用 `x-token` 头查用户信息与钱包余额
//! - 创作者角色通过 `/creators/me/marketListings` 异步探测

use super::models::*;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

pub struct SsyClient {
    http: Client,
}

impl SsyClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build reqwest client");
        Self { http }
    }

    /// 拼授权 URL（callback_url 由调用方基于实际监听端口生成）
    pub fn build_authorization_url(callback_url: &str, state: &str) -> String {
        let mut url = url::Url::parse(SSY_AUTH_URL).expect("parse SSY_AUTH_URL");
        url.query_pairs_mut()
            .append_pair("callback_url", callback_url)
            .append_pair("state", state)
            .append_pair("from", "SSY_SWITCH");
        url.to_string()
    }

    /// OAuth code 换 api_key。
    /// 上游响应层级不稳定（`data.data.api_key` / `data.api_key` / `api_key`），三层兜底。
    pub async fn exchange_code(&self, code: &str, callback_url: &str) -> Result<String, String> {
        let resp = self
            .http
            .post(format!("{SSY_API_BASE}/auth/keys"))
            .header("Accept", "application/json")
            .json(&ExchangeCodeRequest { code, callback_url })
            .send()
            .await
            .map_err(|e| format!("exchange code request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("exchange code HTTP {}", resp.status()));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode exchange response: {e}"))?;
        extract_api_key(&v).ok_or_else(|| "response missing api_key".to_string())
    }

    /// 查询用户信息（`x-token` 认证，注意不是 Bearer）
    pub async fn fetch_user_info(&self, api_key: &str) -> Result<SsyUserInfo, String> {
        let resp = self
            .http
            .get(format!("{SSY_API_BASE}/user/info"))
            .header("x-token", api_key)
            .send()
            .await
            .map_err(|e| format!("user info request failed: {e}"))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("token invalid or expired".to_string());
        }
        if !resp.status().is_success() {
            return Err(format!("user info HTTP {}", resp.status()));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode user info: {e}"))?;
        let data = v.get("data").unwrap_or(&v);
        Ok(SsyUserInfo {
            uid: pick_str(data, &["ID", "id"]),
            display_name: pick_str(data, &["Nickname", "nickname"]),
            email: pick_str(data, &["Email", "email"]),
            avatar_url: pick_str(data, &["HeadImg", "photoUrl"]),
            wallet_assets: data
                .pointer("/Wallet/Assets")
                .or_else(|| data.pointer("/wallet/assets"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        })
    }

    /// 异步探测创作者角色：marketListings 非空即创作者。失败按非创作者处理。
    pub async fn detect_creator_role(&self, api_key: &str) -> bool {
        let Ok(resp) = self
            .http
            .get(format!("{SSY_LOOM_BASE}/creators/me/marketListings"))
            .query(&[("pageSize", "100")])
            .header("x-token", api_key)
            .send()
            .await
        else {
            return false;
        };
        if !resp.status().is_success() {
            return false;
        }
        let Ok(v) = resp.json::<Value>().await else {
            return false;
        };
        v.get("items")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
    }
}

fn extract_api_key(v: &Value) -> Option<String> {
    v.pointer("/data/data/api_key")
        .or_else(|| v.pointer("/data/api_key"))
        .or_else(|| v.get("api_key"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn pick_str(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(Value::as_str) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn api_key_three_layer_fallback() {
        assert_eq!(
            extract_api_key(&json!({"data":{"data":{"api_key":"k1"}}})),
            Some("k1".into())
        );
        assert_eq!(
            extract_api_key(&json!({"data":{"api_key":"k2"}})),
            Some("k2".into())
        );
        assert_eq!(extract_api_key(&json!({"api_key":"k3"})), Some("k3".into()));
        assert_eq!(extract_api_key(&json!({"data":{}})), None);
    }

    #[test]
    fn authorization_url_carries_params() {
        let url = SsyClient::build_authorization_url(
            "http://127.0.0.1:1234/auth/shengsuanyun/callback",
            "st-1",
        );
        assert!(url.contains(
            "callback_url=http%3A%2F%2F127.0.0.1%3A1234%2Fauth%2Fshengsuanyun%2Fcallback"
        ));
        assert!(url.contains("state=st-1"));
        assert!(url.contains("from=SSY_SWITCH"));
    }
}
