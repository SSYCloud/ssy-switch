//! Grok Build（Grok CLI）供应商配置的胜算云适配。
//!
//! Grok CLI 用**原生** `~/.grok/config.toml`：`[models].default` 指向
//! `[model."<profile>"]`，Key 写在模型表的 `api_key` 字段里。这与 Codex 的
//! `auth.OPENAI_API_KEY` + `[model_providers.custom]` 是两套完全不同的结构。
//!
//! 早期实现把 GrokBuild 当成 Codex 处理（模板复用 Codex 的 `[model_providers.custom]`、
//! Key 写 JSON pointer `/auth/OPENAI_API_KEY`），结果是绑定必然失败（Grok 侧校验要求
//! `[models]`），Key 也落在 Grok CLI 不认的位置。本模块是唯一转换入口：模板、Key
//! 读写、seed 卡片都从这里出，形状与前端 `src/utils/grokBuildConfig.ts` 一致。

use serde_json::{json, Value};
use toml_edit::TableLike;

/// Grok CLI 里对用户可见的 profile 名（与前端 `GROK_BUILD_DEFAULT_MODEL` 一致）。
pub const DEFAULT_PROFILE: &str = "grok-4.5";
/// 胜算云路由到 xAI 的真实模型 id（前端 `OPENROUTER_STYLE_GROK_MODEL`）。
pub const SSY_UPSTREAM_MODEL: &str = "x-ai/grok-4.5";
pub const SSY_BASE_URL: &str = "https://router.shengsuanyun.com/api/v1";
pub const SSY_PROVIDER_NAME: &str = "Shengsuanyun";
/// 与前端 `GROK_BUILD_DEFAULT_API_BACKEND` / `..._CONTEXT_WINDOW` 一致。
pub const DEFAULT_API_BACKEND: &str = "responses";
pub const DEFAULT_CONTEXT_WINDOW: i64 = 500_000;

const API_KEY_PLACEHOLDER: &str = "{API_KEY}";

/// 胜算云 Grok Build 的 `config.toml` 模板。
///
/// 手写文本而不是 `toml` 序列化：输出顺序稳定、可读，且与用户最终在
/// `~/.grok/config.toml` 里看到的一致。模板里的取值由 `template_matches_constants`
/// 测试锁定，避免和上面的常量漂移。
const CONFIG_TEMPLATE: &str = r#"[models]
default = "grok-4.5"

[model."grok-4.5"]
model = "x-ai/grok-4.5"
base_url = "https://router.shengsuanyun.com/api/v1"
name = "Shengsuanyun"
api_key = "{API_KEY}"
api_backend = "responses"
context_window = 500000
"#;

/// 种子卡片（`providers_seed.rs`）用的 JSON 文本，Key 为空。
///
/// `OfficialProviderSeed.settings_config_json` 是 `&'static str`，没法运行时拼装，
/// 因此这里保留一份编译期常量；`empty_seed_json_matches_builder` 测试锁定它与
/// `build_settings_config("")` 的序列化结果逐字节一致，防止两边漂移。
pub const EMPTY_SETTINGS_JSON: &str = r#"{"config":"[models]\ndefault = \"grok-4.5\"\n\n[model.\"grok-4.5\"]\nmodel = \"x-ai/grok-4.5\"\nbase_url = \"https://router.shengsuanyun.com/api/v1\"\nname = \"Shengsuanyun\"\napi_key = \"\"\napi_backend = \"responses\"\ncontext_window = 500000\n"}"#;

/// TOML 基本字符串转义（Key 由服务端下发，理论上不含引号，防御性处理）。
fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 完整的胜算云 Grok Build `config.toml` 文本。
pub fn config_toml(api_key: &str) -> String {
    CONFIG_TEMPLATE.replace(API_KEY_PLACEHOLDER, &escape_toml_string(api_key))
}

/// 胜算云 Grok Build 供应商的 `settingsConfig`（`{ "config": "<toml>" }`）。
pub fn build_settings_config(api_key: &str) -> Value {
    json!({ "config": config_toml(api_key) })
}

/// 从 `settingsConfig` 读出当前 Key（官方态 / 无自定义模型表时返回空串）。
pub fn read_api_key(settings: &Value) -> String {
    let Some(config) = settings.get("config").and_then(Value::as_str) else {
        return String::new();
    };
    let Ok(document) = config.parse::<toml::Value>() else {
        return String::new();
    };
    let Some(root) = document.as_table() else {
        return String::new();
    };
    let profile = root
        .get("models")
        .and_then(|v| v.as_table())
        .and_then(|models| models.get("default"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_PROFILE);
    root.get("model")
        .and_then(|v| v.as_table())
        .and_then(|models| models.get(profile))
        .and_then(|v| v.as_table())
        .and_then(|selected| selected.get("api_key"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 只在字段缺失（或空值）时补默认值，用户已有取值一律不动。
fn ensure_str(table: &mut dyn TableLike, field: &str, fallback: &str) {
    let present = table
        .get(field)
        .and_then(toml_edit::Item::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if !present {
        let _ = table.insert(field, toml_edit::value(fallback));
    }
}

/// 同 `ensure_str`，用于 `context_window` 这类必须为正整数的字段。
fn ensure_positive_int(table: &mut dyn TableLike, field: &str, fallback: i64) {
    let present = table
        .get(field)
        .and_then(toml_edit::Item::as_integer)
        .is_some_and(|value| value > 0);
    if !present {
        let _ = table.insert(field, toml_edit::value(fallback));
    }
}

/// 把 Key 写进 `settingsConfig`：保留用户已有的其它模型表、`[mcp_servers]` 与排版，
/// 只把 Key 落到当前默认模型表的 `api_key`，并补齐缺失的必填字段。
pub fn write_api_key(settings: &mut Value, api_key: &str) -> Result<(), String> {
    let object = settings
        .as_object_mut()
        .ok_or("Grok Build 配置必须是 JSON 对象")?;
    let existing = object
        .get("config")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // 官方态（空快照 / 空文件）没有自定义模型表：直接按胜算云模板重建。
    if existing.trim().is_empty() {
        object.insert("config".into(), Value::String(config_toml(api_key)));
        return Ok(());
    }

    let mut document = existing
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("Grok Build config.toml 解析失败: {error}"))?;

    let profile = document
        .get("models")
        .and_then(|item| item.get("default"))
        .and_then(toml_edit::Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_PROFILE)
        .to_string();

    if !document.contains_key("models") {
        let _ = document.insert("models", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let models = document
        .get_mut("models")
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or("Grok Build 配置的 [models] 不是 TOML 表结构")?;
    ensure_str(models, "default", &profile);

    if !document.contains_key("model") {
        let _ = document.insert("model", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let model_tables = document
        .get_mut("model")
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or("Grok Build 配置的 [model] 不是 TOML 表结构")?;
    if !model_tables.contains_key(&profile) {
        let _ = model_tables.insert(&profile, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let selected = model_tables
        .get_mut(&profile)
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or_else(|| format!("Grok Build 配置的 [model.\"{profile}\"] 不是 TOML 表结构"))?;

    ensure_str(selected, "model", SSY_UPSTREAM_MODEL);
    ensure_str(selected, "base_url", SSY_BASE_URL);
    ensure_str(selected, "name", SSY_PROVIDER_NAME);
    ensure_str(selected, "api_backend", DEFAULT_API_BACKEND);
    ensure_positive_int(selected, "context_window", DEFAULT_CONTEXT_WINDOW);
    let _ = selected.insert("api_key", toml_edit::value(api_key));

    object.insert("config".into(), Value::String(document.to_string()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grok_config::validate_config_toml;

    #[test]
    fn template_passes_grok_cli_validation() {
        let toml = config_toml("sk-ssy-test");
        validate_config_toml(&toml).expect("胜算云模板必须能通过 Grok CLI 的强校验");
        assert!(toml.contains("[models]"));
        assert!(toml.contains("[model.\"grok-4.5\"]"));
        // 不能残留 Codex 的字段（曾把 GrokBuild 当 Codex 处理）
        assert!(!toml.contains("model_providers"));
    }

    /// 模板文本与常量单一来源：改常量忘改模板会在这里失败。
    #[test]
    fn template_matches_constants() {
        let toml = config_toml("k");
        assert!(toml.contains(&format!("default = \"{DEFAULT_PROFILE}\"")));
        assert!(toml.contains(&format!("[model.\"{DEFAULT_PROFILE}\"]")));
        assert!(toml.contains(&format!("model = \"{SSY_UPSTREAM_MODEL}\"")));
        assert!(toml.contains(&format!("base_url = \"{SSY_BASE_URL}\"")));
        assert!(toml.contains(&format!("name = \"{SSY_PROVIDER_NAME}\"")));
        assert!(toml.contains(&format!("api_backend = \"{DEFAULT_API_BACKEND}\"")));
        assert!(toml.contains(&format!("context_window = {DEFAULT_CONTEXT_WINDOW}")));
    }

    /// 种子卡片常量与运行时生成的 JSON 必须逐字节一致。
    #[test]
    fn empty_seed_json_matches_builder() {
        let built = serde_json::to_string(&build_settings_config("")).expect("serialize");
        assert_eq!(EMPTY_SETTINGS_JSON, built);
    }

    #[test]
    fn empty_template_still_parses_as_grok_provider() {
        let toml = config_toml("");
        let document: toml::Value = toml.parse().expect("valid toml");
        let root = document.as_table().unwrap();
        assert_eq!(
            root.get("models")
                .and_then(|v| v.as_table())
                .and_then(|m| m.get("default"))
                .and_then(|v| v.as_str()),
            Some(DEFAULT_PROFILE)
        );
        assert_eq!(read_api_key(&build_settings_config("")), "");
    }

    #[test]
    fn key_roundtrips_through_the_default_model_table() {
        let mut settings = build_settings_config("");
        write_api_key(&mut settings, "sk-ssy-123").unwrap();
        assert_eq!(read_api_key(&settings), "sk-ssy-123");
        let toml = settings.get("config").and_then(Value::as_str).unwrap();
        validate_config_toml(toml).expect("写入 Key 后仍满足 Grok CLI 校验");
        assert!(toml.contains(SSY_BASE_URL));
        assert!(toml.contains(SSY_UPSTREAM_MODEL));
    }

    /// Key 里出现引号/反斜杠也不能破坏 TOML。
    #[test]
    fn key_with_quotes_stays_valid_toml() {
        let mut settings = build_settings_config("");
        write_api_key(&mut settings, "sk-\"weird\"\\key").unwrap();
        let toml = settings.get("config").and_then(Value::as_str).unwrap();
        validate_config_toml(toml).unwrap();
        assert_eq!(read_api_key(&settings), "sk-\"weird\"\\key");
    }

    /// 官方态快照（`{"config":""}`）写 Key 时直接重建为胜算云模板。
    #[test]
    fn writing_key_into_official_snapshot_rebuilds_ssy_template() {
        let mut settings = json!({ "config": "" });
        write_api_key(&mut settings, "sk-ssy-456").unwrap();
        assert_eq!(read_api_key(&settings), "sk-ssy-456");
        let toml = settings.get("config").and_then(Value::as_str).unwrap();
        validate_config_toml(toml).unwrap();
    }

    /// 用户自己的其它模型表与 MCP 段不能被覆盖。
    #[test]
    fn writing_key_preserves_other_model_tables_and_mcp() {
        let existing = r#"[models]
default = "grok-4.5"

[model."grok-4.5"]
model = "grok-4.5"
base_url = "https://api.x.ai/v1"
name = "xAI"
api_key = "old"
api_backend = "responses"
context_window = 500000

[model."local"]
model = "local-model"
base_url = "http://127.0.0.1:8080/v1"
name = "Local"
env_key = "LOCAL_KEY"
api_backend = "chat_completions"
context_window = 8192

[mcp_servers.demo]
command = "demo"
"#;
        let mut settings = json!({ "config": existing });
        write_api_key(&mut settings, "sk-ssy-789").unwrap();
        let toml = settings.get("config").and_then(Value::as_str).unwrap();
        validate_config_toml(toml).unwrap();
        let document: toml::Value = toml.parse().unwrap();
        let root = document.as_table().unwrap();
        let models = root.get("model").and_then(|v| v.as_table()).unwrap();
        // 其它模型表原样保留
        assert!(models.contains_key("local"));
        assert!(root.get("mcp_servers").is_some());
        // 只替换了默认模型表的 Key
        assert_eq!(read_api_key(&settings), "sk-ssy-789");
    }

    /// 自建卡片只填了 Key、缺 base_url 等字段时，绑定要能补齐到「可激活」状态。
    #[test]
    fn writing_key_fills_missing_required_fields() {
        let mut settings = json!({
            "config": "[models]\ndefault = \"grok-4.5\"\n\n[model.\"grok-4.5\"]\napi_key = \"\"\n"
        });
        write_api_key(&mut settings, "sk-ssy-000").unwrap();
        let toml = settings.get("config").and_then(Value::as_str).unwrap();
        validate_config_toml(toml).expect("补齐后必须能通过 Grok CLI 校验");
        assert_eq!(read_api_key(&settings), "sk-ssy-000");
        assert!(toml.contains(SSY_BASE_URL));
    }

    /// 占位种子卡片 + 绑定写入 = 可激活配置。
    ///
    /// 占位卡片 Key 为空（和 Claude/Codex 的种子一样）：Grok 侧强校验要求有凭据，
    /// 所以未绑定时激活会明确报错，绑定写入 Key 之后即可激活。
    /// （种子条目本身在 `providers_seed` 的测试里断言。）
    #[test]
    fn seed_placeholder_becomes_activatable_after_binding() {
        let mut settings: Value =
            serde_json::from_str(EMPTY_SETTINGS_JSON).expect("seed json is valid");
        let toml = settings
            .get("config")
            .and_then(Value::as_str)
            .expect("config");
        crate::grok_config::validate_config_toml_syntax(toml).expect("种子卡片语法合法");
        assert_eq!(read_api_key(&settings), "");
        write_api_key(&mut settings, "sk-ssy-seed").unwrap();
        let bound = settings.get("config").and_then(Value::as_str).unwrap();
        validate_config_toml(bound).expect("绑定后必须能被 Grok CLI 接受");
        assert_eq!(read_api_key(&settings), "sk-ssy-seed");
    }
}
