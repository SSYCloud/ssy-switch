# SSY SDK 抽离蓝图（v2 · 基于实测代码面）

> 2026-09-18 更新。当前 SSY 相关代码总量约 **4,900 行**（Rust ~1,700 + TS ~3,200）。

## 1. 实测耦合面（抽离难度依据）

| 模块 | 行数 | 对壳依赖 | 抽离难度 |
|---|---:|---|---|
| `shengsuanyun/client.rs` | ~300 | **0**（纯 reqwest+serde） | 直搬 |
| `shengsuanyun/callback_server.rs` | ~130 | 0（axum，泛型化 manager 即可） | 直搬 |
| `shengsuanyun/models.rs` | 361 | 1 处（`provider::ProviderMeta` 默认用量脚本——壳专属，拆出） | 低 |
| `shengsuanyun/auth_manager.rs` | ~400 | 2 处（`Database` + `AppHandle` 事件） | 中（trait 反转） |
| `shengsuanyun/credential_store.rs` | 61 | `Database` | 中（trait 反转） |
| `shengsuanyun/grok_build.rs` | 336 | 0 | 直搬（或留壳：Grok 专属） |
| `database/dao/shengsuanyun.rs` | 635 | rusqlite/Database | 留壳（实现 store trait） |
| `commands/shengsuanyun.rs` | 687 | tauri/ProviderService/analytics | 留壳 |
| 前端 api/flow/rechargeTracker | ~400 | Tauri IPC | 留壳（或抽 @ssy/api-ts） |
| 前端组件（5 个） | ~1,275 | Tauri/组件库 | 留壳 |

外部反向依赖仅 4 处（lib.rs/commands 引用 `shengsuanyun::`）——对外暴露面极小。

## 2. 目标架构（Cargo workspace）

```
crates/ssy-core/
├── src/
│   ├── client.rs          # 全部 HTTP：auth/keys、user/info、userusage、
│   │                      #   modalities、userlog、billlist、loom balance、创作者探测
│   ├── models.rs          # DTO/脱敏/单位换算（ProviderMeta 部分留壳）
│   ├── auth_manager.rs    # 泛型 <S: CredentialStore, A: AccountStore, E: EventSink>
│   ├── callback_server.rs # axum loopback（泛型 manager）
│   ├── credential.rs      # trait CredentialStore + StoredCredentials
│   ├── store.rs           # trait AccountStore（账号/绑定元数据）
│   └── error.rs
└── Cargo.toml             # reqwest/axum(feature)/uuid/serde/chrono；无 tauri/rusqlite

src-tauri/
├── src/shengsuanyun/
│   ├── sqlite_stores.rs   # 两个 trait 的 rusqlite 实现
│   ├── events.rs          # EventSink → Tauri emitter
│   └── usage_script.rs    # 默认用量脚本 meta（依赖 provider::ProviderMeta，壳专属）
└── commands/shengsuanyun.rs  # 不变（改调 sdk 句柄）
```

## 3. 关键 trait 设计

```rust
pub trait CredentialStore: Send + Sync {
    fn save(&self, account_id: &str, c: &StoredCredentials) -> Result<(), String>;
    fn load(&self, account_id: &str) -> Result<StoredCredentials, String>;
    fn delete(&self, account_id: &str) -> Result<(), String>;
}

pub trait AccountStore: Send + Sync {
    fn upsert_account(&self, a: NewAccount) -> Result<AccountRow, String>;
    fn list_accounts(&self) -> Result<Vec<AccountRow>, String>;
    fn get_account(&self, id: &str) -> Result<Option<AccountRow>, String>;
    fn delete_account(&self, id: &str) -> Result<(), String>;   // 级联删绑定
    fn update_balance(&self, id: &str, assets: f64, voucher: f64, ts: i64) -> Result<(), String>;
    fn upsert_binding(&self, b: NewBinding) -> Result<(), String>;
    fn list_bindings(&self) -> Result<Vec<BindingRow>, String>;
}

pub trait LoginEvents: Send + Sync {
    fn oauth_complete(&self, payload: &OAuthCompletePayload);
    fn oauth_failed(&self, session_id: &str, reason: &str);
}
```

壳层组合：`ShengsuanyunAuthManager::new(SqliteStores::new(db.clone()), TauriEvents::new(handle))`。

## 4. 迁移步骤（2-3 天）

1. **半天**：建 workspace（`crates/ssy-core` + `src-tauri` path 依赖）；搬 client/models/callback_server（零改动编译）
2. **半天**：credential_store → trait；auth_manager 泛型化（把 Database/Event 调用改为 trait 方法）
3. **半天**：壳层实现 stores/events；commands 改构造；删旧模块
4. **1 天**：测试迁移（auth_manager/queue 现有单测随代码走；DAO 测试留壳；补 SDK mockHTTP 集成测试）；CI matrix
5. **可选半天**：`@ssy/api-ts`（shengsuanyun.ts 的 invoke 层换 SDK 类型定义，前端组件仍留壳）

## 5. 发布与版本策略

- 内部：私有 git 仓库 + tag（`ssy-core v0.1.0`），workspace path 依赖开发期无缝切换 git 依赖
- 对外：crates.io（开源决策后）；semver：trait 变更 = minor 破坏
- SDK 内置 feature flags：`callback-server`（默认开）、`grok`（Grok Build 模板，默认关）

## 6. 风险与对策

| 风险 | 对策 |
|---|---|
| auth_manager 的启动对账逻辑在壳层（lib.rs），SDK 化后职责边界 | 对账留壳（它依赖 ProviderService），SDK 只保证原语完整 |
| 单测目前用 `Database::memory()` | trait 化后用 `InMemoryCredentialStore/InMemoryAccountStore`，更快更纯 |
| schema v21 的 uid 迁移属壳层 | 留壳不动 |
| 前端组件强耦合 CC Switch 表单体系 | 组件不抽；抽的只有 API 类型层（可选）|

## 7. 触发条件（满足其一）

- [ ] 第二个消费方出现（Cline 插件后端 / 其他 Rust 工具）
- [ ] 上游合并冲突连续两次波及 `shengsuanyun/`
- [ ] 需要独立发版节奏
