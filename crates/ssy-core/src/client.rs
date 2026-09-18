//! 胜算云上游 API client。
//!
//! 协议参考 `参考/auth_ssy.go`（LoomLoom webapp）：
//! - `POST /auth/keys` 用 OAuth code 换 api_key（响应三层兜底）
//! - `GET /user/info` 用 `x-token` 头查用户信息与钱包余额
//! - `GET /token/list` 用 `x-token` 头列出该账号名下全部 Token（含明文，需按需取用）
//! - 创作者角色通过 `/creators/me/marketListings` 异步探测

use crate::models::*;
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

impl Default for SsyClient {
    fn default() -> Self {
        Self::new()
    }
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
            // 上游 `data.ID` 是**数字**（实测 62890）；直接 as_str 会静默得到空串，
            // 导致「同 uid 幂等」失效并误删其它账号。这里统一转成字符串。
            uid: pick_id_str(data, &["ID", "id"]),
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
            voucher_assets: data
                .pointer("/Wallet/VoucherBalance")
                .or_else(|| data.pointer("/Wallet/voucherBalance"))
                .or_else(|| data.pointer("/wallet/voucherBalance"))
                .map(flexible_f64)
                .unwrap_or(0.0),
        })
    }

    /// 查询大模型调用记录（按日/按模型聚合）。
    /// `total_amount` 单位为 1e-7 元（与 loom balance 的 *T 字段同口径）。
    /// 仅接受 jwt 认证（网关 Key 返回 20003）。
    pub async fn fetch_user_usage(
        &self,
        start_date: &str,
        end_date: &str,
        token: &str,
    ) -> Result<Value, String> {
        let resp = self
            .http
            .get(format!("{SSY_API_BASE}/modelrouter/userusage"))
            .query(&[("startDate", start_date), ("endDate", end_date)])
            .header("x-token", token)
            .send()
            .await
            .map_err(|e| format!("user usage request failed: {e}"))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("401 token invalid".to_string());
        }
        if !resp.status().is_success() {
            return Err(format!("user usage HTTP {}", resp.status()));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode user usage: {e}"))?;
        // 业务码校验（上游失败时 code != 0 且 data 里没有 usages）
        let biz_code = v.get("code").and_then(Value::as_i64).unwrap_or(0);
        if biz_code != 0 {
            return Err(format!(
                "user usage failed: {}",
                v.pointer("/msg")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
            ));
        }
        Ok(v.get("data").cloned().unwrap_or(v))
    }

    /// 充值/账单流水（POST，分页）。Asset/Balance 单位 1e-4 元。
    /// 注意：该接口只接受 POST，GET 会 404。
    pub async fn fetch_bill_list(
        &self,
        page: i64,
        page_size: i64,
        token: &str,
    ) -> Result<Value, String> {
        let resp = self
            .http
            .post(format!("{SSY_API_BASE}/userorder/billlist"))
            .header("x-token", token)
            .json(&serde_json::json!({ "page": page, "page_size": page_size }))
            .send()
            .await
            .map_err(|e| format!("bill list request failed: {e}"))?;
        Self::check_auth_and_status(
            resp.status() == reqwest::StatusCode::UNAUTHORIZED,
            resp.status(),
        )
        .await?;
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode bill list: {e}"))?;
        Self::unwrap_business_data(v)
    }

    /// 多模态调用统计（图片/视频等，参数为 startDate/endDate）
    pub async fn fetch_modality_usage(
        &self,
        start_date: &str,
        end_date: &str,
        token: &str,
    ) -> Result<Value, String> {
        let resp = self
            .http
            .get(format!("{SSY_API_BASE}/modelrouter/modalities/userusage"))
            .query(&[("startDate", start_date), ("endDate", end_date)])
            .header("x-token", token)
            .send()
            .await
            .map_err(|e| format!("modality usage request failed: {e}"))?;
        Self::check_auth_and_status(
            resp.status() == reqwest::StatusCode::UNAUTHORIZED,
            resp.status(),
        )
        .await?;
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode modality usage: {e}"))?;
        Self::unwrap_business_data(v)
    }

    /// 代金券/体验券明细（官方控制台"卡券"页同款接口）。
    /// data.voucher_records[] 的金额字段单位 1e-4 元；scope 标记可用范围
    /// （all=通用，loomloom 等为产品专属——专属券不计入 /user/info 的 VoucherBalance）。
    pub async fn fetch_voucher_list(&self, token: &str) -> Result<Value, String> {
        let resp = self
            .http
            .get(format!("{SSY_API_BASE}/voucher/user_voucher_list"))
            .header("x-token", token)
            .send()
            .await
            .map_err(|e| format!("voucher list request failed: {e}"))?;
        Self::check_auth_and_status(
            resp.status() == reqwest::StatusCode::UNAUTHORIZED,
            resp.status(),
        )
        .await?;
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode voucher list: {e}"))?;
        Self::unwrap_business_data(v)
    }

    async fn check_auth_and_status(
        unauthorized: bool,
        status: reqwest::StatusCode,
    ) -> Result<(), String> {
        if unauthorized {
            return Err("401 token invalid".to_string());
        }
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }
        Ok(())
    }

    /// 上游通用响应解包：code != 0 视为失败；成功返回 data 节点
    fn unwrap_business_data(v: Value) -> Result<Value, String> {
        let biz_code = v.get("code").and_then(Value::as_i64).unwrap_or(0);
        if biz_code != 0 {
            return Err(format!(
                "upstream error: {}",
                v.pointer("/msg")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
            ));
        }
        Ok(v.get("data").cloned().unwrap_or(v))
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

    /// 列出账号名下的全部 Token（`GET /token/list`）。
    ///
    /// 必须用 jwt_token（用 api_key 调用上游返回 `20003 That's not even a token`）。
    /// 返回结构里的 `Token` 字段是**完整明文**，调用方需按需取用、不得落库或打日志。
    pub async fn list_tokens(&self, token: &str) -> Result<Vec<SsyToken>, String> {
        let resp = self
            .http
            .get(format!("{SSY_API_BASE}/token/list"))
            .header("x-token", token)
            .send()
            .await
            .map_err(|e| format!("token list request failed: {e}"))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("token invalid or expired".to_string());
        }
        if !resp.status().is_success() {
            return Err(format!("token list HTTP {}", resp.status()));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("decode token list: {e}"))?;
        if v_f64(v.get("code")) != 0.0 {
            return Err(format!("token list failed: {}", extract_error_message(&v)));
        }
        let items = token_list_items(&v);
        Ok(items.iter().map(parse_token).collect())
    }
}

/// `/token/list` 的 data 层级不稳定：数组 / data.list / data.items / data.data
fn token_list_items(v: &Value) -> Vec<Value> {
    let data = v.get("data").unwrap_or(v);
    if let Some(arr) = data.as_array() {
        return arr.clone();
    }
    for key in ["list", "items", "data", "tokens"] {
        if let Some(arr) = data.get(key).and_then(Value::as_array) {
            return arr.clone();
        }
    }
    Vec::new()
}

/// 单条 Token：逐字段柔性解析（上游数字/字符串混用）
fn parse_token(v: &Value) -> SsyToken {
    SsyToken {
        id: pick_i64(v, &["ID", "id"]),
        name: pick_str(v, &["Name", "name"]),
        token: pick_str(v, &["Token", "token"]),
        desc: pick_str(v, &["Desc", "desc"]),
        is_default: pick_bool(v, &["IsDefault", "isDefault"]),
        is_banned: pick_bool(v, &["IsBanned", "isBanned"]),
        is_expired: pick_bool(v, &["IsExpired", "isExpired"]),
        max_quota: pick_f64(v, &["MaxQuota", "maxQuota"]),
        consumed_amount: pick_f64(v, &["ConsumedAmount", "consumedAmount"]),
        created_at: pick_i64(v, &["CreatedAt", "createdAt"]),
        expires_at: pick_i64(v, &["ExpiresAt", "expiresAt"]),
        supported_models: pick_models(v),
    }
}

/// `SupportedModels` 是 JSON 字符串（也可能是数组），解析失败按空集合处理
fn pick_models(v: &Value) -> Vec<String> {
    for key in ["SupportedModels", "supportedModels"] {
        let Some(raw) = v.get(key) else { continue };
        match raw {
            Value::Array(items) => {
                return items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect();
            }
            Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() || trimmed == "null" {
                    return Vec::new();
                }
                if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) {
                    return items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect();
                }
                // 逗号分隔兜底
                return trimmed
                    .split(',')
                    .map(|m| m.trim().trim_matches('"').to_string())
                    .filter(|m| !m.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    Vec::new()
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

/// 数字或数字字符串 → String（ID 类字段用，避免 as_str 静默丢失）
fn pick_id_str(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        match v.get(*k) {
            Some(Value::Number(n)) => return n.to_string(),
            Some(Value::String(s)) if !s.is_empty() => return s.clone(),
            _ => {}
        }
    }
    String::new()
}

/// 数字或数字字符串 → i64（缺失/非法为 0）
fn pick_i64(v: &Value, keys: &[&str]) -> i64 {
    for k in keys {
        match v.get(*k) {
            Some(Value::Number(n)) => {
                if let Some(i) = n.as_i64() {
                    return i;
                }
                if let Some(f) = n.as_f64() {
                    return f as i64;
                }
            }
            Some(Value::String(s)) => {
                if let Ok(i) = s.trim().parse::<i64>() {
                    return i;
                }
                if let Ok(f) = s.trim().parse::<f64>() {
                    return f as i64;
                }
            }
            _ => {}
        }
    }
    0
}

/// 数字或数字字符串 → f64（缺失/非法为 0）
///
/// 命中第一个「存在且可解析」的字段即返回：`0` 是合法取值
/// （`MaxQuota = 0` 表示无上限），不能当成「没找到」而继续找下一个候选键。
fn pick_f64(v: &Value, keys: &[&str]) -> f64 {
    for k in keys {
        match v.get(*k) {
            Some(Value::Number(n)) => return n.as_f64().unwrap_or(0.0),
            Some(Value::String(s)) => {
                if let Ok(parsed) = s.trim().parse::<f64>() {
                    return parsed;
                }
            }
            _ => {}
        }
    }
    0.0
}

/// bool / 0-1 / "true" → bool
fn pick_bool(v: &Value, keys: &[&str]) -> bool {
    for k in keys {
        match v.get(*k) {
            Some(Value::Bool(b)) => return *b,
            Some(Value::Number(n)) => return n.as_f64().unwrap_or(0.0) != 0.0,
            Some(Value::String(s)) => {
                let t = s.trim().to_ascii_lowercase();
                return t == "true" || t == "1";
            }
            _ => {}
        }
    }
    false
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
    fn uid_accepts_numeric_id() {
        // 上游 /user/info 的 ID 是数字；旧实现 as_str 会得到空串
        assert_eq!(pick_id_str(&json!({"ID": 62890}), &["ID", "id"]), "62890");
        assert_eq!(pick_id_str(&json!({"ID": "62890"}), &["ID"]), "62890");
        assert_eq!(pick_id_str(&json!({}), &["ID", "id"]), "");
    }

    #[test]
    fn zero_quota_is_a_real_value() {
        // MaxQuota = 0 是「无上限」，不能被当成「字段缺失」而跳到下一个候选键
        assert_eq!(
            pick_f64(&json!({"MaxQuota": 0}), &["MaxQuota", "maxQuota"]),
            0.0
        );
        assert_eq!(pick_f64(&json!({"MaxQuota": "0"}), &["MaxQuota"]), 0.0);
        assert_eq!(
            pick_f64(
                &json!({"MaxQuota": 0, "maxQuota": 9}),
                &["MaxQuota", "maxQuota"]
            ),
            0.0
        );
        assert_eq!(pick_f64(&json!({}), &["MaxQuota", "maxQuota"]), 0.0);
        assert_eq!(
            pick_f64(&json!({"MaxQuota": "1000000"}), &["MaxQuota"]),
            1_000_000.0
        );
    }

    #[test]
    fn token_list_parses_upstream_shapes() {
        // data 层级：数组 / data.list 两种都要能解析
        let arr = json!({"code": 0, "data": [
            {"ID": 68545, "Name": "default", "Token": "k".repeat(86), "IsDefault": true,
             "MaxQuota": 0, "ConsumedAmount": 2264263,
             "SupportedModels": "[\"anthropic/claude-opus-5\"]"}
        ]});
        let items = token_list_items(&arr);
        assert_eq!(items.len(), 1);
        let t = parse_token(&items[0]);
        assert_eq!(t.id, 68545);
        assert!(t.is_default);
        assert_eq!(t.max_quota_yuan(), None, "MaxQuota=0 表示无上限");
        assert!((t.consumed_yuan() - 226.4263).abs() < 1e-9);
        assert_eq!(t.supported_models, vec!["anthropic/claude-opus-5"]);
        assert!(t.selectable());

        let nested = json!({"code": 0, "data": {"list": [
            {"ID": "83949", "Name": "Bear Xiong", "Token": "x".repeat(90),
             "IsBanned": false, "IsExpired": true, "MaxQuota": "1000000"}
        ]}});
        let items = token_list_items(&nested);
        assert_eq!(items.len(), 1);
        let t = parse_token(&items[0]);
        assert_eq!(t.id, 83949);
        assert!(t.is_expired);
        assert!(!t.selectable(), "过期 Key 不可选");
        assert!((t.max_quota_yuan().unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn token_list_business_error_is_rejected() {
        let body = json!({"code": 20003, "msg": "That's not even a token"});
        assert_ne!(v_f64(body.get("code")), 0.0);
        assert_eq!(extract_error_message(&body), "That's not even a token");
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
