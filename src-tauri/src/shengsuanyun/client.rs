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

/// /auth/keys 返回的双凭据
#[derive(Clone, Debug, Default)]
pub struct SsyCredentials {
    pub api_key: String,
    pub jwt_token: String,
}

impl SsyCredentials {
    /// 用户信息查询优先用 jwt_token（main.go:543），缺失时退回 api_key
    pub fn identity_token(&self) -> &str {
        if self.jwt_token.is_empty() {
            &self.api_key
        } else {
            &self.jwt_token
        }
    }
}

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

    /// OAuth code 换凭据（api_key + jwt_token）。
    /// 参照 auth_ssy.go parseSSYCredentials：
    /// - 业务码 `code != 0` 视为失败，错误信息按 `message`/`msg`/`error` 提取
    /// - 响应层级不稳定（`data.data.*` / `data.*` / 顶层），逐层兜底
    /// - `api_key` 缺失时用 `jwt_token` 兜底（二者至少要有其一）
    pub async fn exchange_code(
        &self,
        code: &str,
        callback_url: &str,
    ) -> Result<SsyCredentials, String> {
        let resp = self
            .http
            .post(format!("{SSY_API_BASE}/auth/keys?from=SSY_SWITCH"))
            .header("Accept", "application/json")
            .json(&ExchangeCodeRequest { code, callback_url })
            .send()
            .await
            .map_err(|e| format!("exchange code request failed: {e}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode exchange response: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "exchange code HTTP {status}: {}",
                extract_error_message(&body)
            ));
        }
        // 业务响应码非 0 → 失败（auth_ssy.go:124）
        let biz_code = v_f64(body.get("code"));
        if biz_code != 0.0 {
            return Err(format!(
                "exchange code failed: {}",
                extract_error_message(&body)
            ));
        }
        let mut api_key = extract_nested(&body, "api_key").unwrap_or_default();
        let jwt_token = extract_nested(&body, "jwt_token").unwrap_or_default();
        if api_key.is_empty() {
            api_key = jwt_token.clone();
        }
        if api_key.is_empty() {
            return Err("response missing api_key".to_string());
        }
        Ok(SsyCredentials { api_key, jwt_token })
    }

    /// 查询用户信息（`x-token` 认证，注意不是 Bearer）。
    /// 参照 main.go:543：必须用 jwt_token（identity token）调用，
    /// 用 api_key 可能认证通过但缺少 Wallet 余额数据。
    pub async fn fetch_user_info(&self, token: &str) -> Result<SsyUserInfo, String> {
        let resp = self
            .http
            .get(format!("{SSY_API_BASE}/user/info"))
            .header("x-token", token)
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
                .or_else(|| data.pointer("/Wallet/assets"))
                .or_else(|| data.pointer("/wallet/Assets"))
                .or_else(|| data.pointer("/wallet/assets"))
                .map(flexible_f64)
                .unwrap_or(0.0),
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

/// 按 `data.data.{field}` / `data.{field}` / `{field}` 三层取字符串
fn extract_nested(v: &Value, field: &str) -> Option<String> {
    v.pointer(&format!("/data/data/{field}"))
        .or_else(|| v.pointer(&format!("/data/{field}")))
        .or_else(|| v.get(field))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 错误信息提取（auth_ssy.go ssyErrorFrom：message / msg / error）
fn extract_error_message(v: &Value) -> String {
    for key in ["message", "msg", "error"] {
        if let Some(s) = v.get(key).and_then(Value::as_str).map(str::trim) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    "服务端未提供错误信息".to_string()
}

/// 数字或字符串 → f64（业务码兼容）
fn v_f64(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// 上游 Assets 类型不稳定（Go 参考实现 ssyFloat 同时兼容数字与字符串）
fn flexible_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
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
    fn wallet_assets_accepts_string_and_number() {
        use serde_json::json;
        let num = json!({"Wallet": {"Assets": 235000}});
        let s = json!({"Wallet": {"Assets": "235000"}});
        let lower = json!({"wallet": {"assets": 1500.5}});
        for v in [&num, &s] {
            assert_eq!(
                v.pointer("/Wallet/Assets").map(flexible_f64),
                Some(235000.0)
            );
        }
        assert_eq!(
            lower.pointer("/wallet/assets").map(flexible_f64),
            Some(1500.5)
        );
    }

    #[test]
    fn api_key_three_layer_fallback() {
        assert_eq!(
            extract_nested(&json!({"data":{"data":{"api_key":"k1"}}}), "api_key"),
            Some("k1".into())
        );
        assert_eq!(
            extract_nested(&json!({"data":{"api_key":"k2"}}), "api_key"),
            Some("k2".into())
        );
        assert_eq!(
            extract_nested(&json!({"api_key":"k3"}), "api_key"),
            Some("k3".into())
        );
        assert_eq!(extract_nested(&json!({"data":{}}), "api_key"), None);
    }

    #[test]
    fn error_message_extraction() {
        assert_eq!(
            extract_error_message(&json!({"message":"code expired"})),
            "code expired"
        );
        assert_eq!(
            extract_error_message(&json!({"msg": 1})),
            "服务端未提供错误信息"
        );
    }

    #[test]
    fn biz_code_detection() {
        assert_eq!(v_f64(Some(&json!(0))), 0.0);
        assert_eq!(v_f64(Some(&json!("40001"))), 40001.0);
        assert_eq!(v_f64(None), 0.0);
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
