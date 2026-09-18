# SSY SDK 抽取方案（执行蓝图）

> 状态：**未实施**（触发条件见 §4）· 2026-09-18
>
> 背景：胜算云核心逻辑目前内嵌在 `src-tauri/src/shengsuanyun/`，与 CC Switch
> 壳（Tauri/SQLite/ProviderService）耦合有限但存在。本蓝图描述抽取为独立
> Rust SDK（`ssy-core`）的目标形态，供触发重构时直接执行。

## 1. 现状耦合盘点

| 文件 | 对壳的依赖 | 可下沉 SDK |
|---|---|---|
| `shengsuanyun/client.rs` | 无（纯 reqwest） | ✅ 直接搬 |
| `shengsuanyun/callback_server.rs` | 无（axum + tokio） | ✅ 直接搬 |
| `shengsuanyun/models.rs` | `crate::provider::ProviderMeta`（默认用量脚本） | ✅ 拆出（meta 部分留壳） |
| `shengsuanyun/auth_manager.rs` | `Arc<Database>` + `tauri::AppHandle`（事件） | ✅ 反转依赖后搬 |
| `shengsuanyun/credential_store.rs` | `Database` | ✅ 已是 trait 候选 |
| `database/dao/shengsuanyun.rs` | rusqlite/`Database`/`AppError` | ❌ 留壳（实现 store trait） |
| `commands/shengsuanyun.rs` | tauri::State/ProviderService | ❌ 留壳（薄壳 commands） |
| `lib.rs` 启动对账 | AppState | ❌ 留壳 |
| 前端 `shengsuanyunFlow.ts` 等 | Tauri IPC | ❌ 留壳 |

## 2. 目标形态

```
crates/ssy-core/            # 纯 Rust，无 tauri 依赖
├── src/
│   ├── client.rs           # auth/keys、user/info(jwt)、loom balance(api key)、创作者探测
│   ├── auth_manager.rs     # state/TTL/一次性消费 + 账号生命周期（泛型存储）
│   ├── callback_server.rs  # loopback axum listener
│   ├── models.rs           # DTO、脱敏、assets→yuan（仅一次）
│   └── store.rs            # trait CredentialStore + trait AccountStore
└── Cargo.toml              # deps: reqwest, axum(可选 feature), uuid, serde, chrono

src-tauri/
├── src/shengsuanyun/
│   ├── stores.rs           # 两个 trait 的 SQLite 实现
│   └── events.rs           # AppHandle 事件发射器实现（sdk 回调 trait）
└── commands/shengsuanyun.rs # 薄壳：State → sdk 调用
```

关键设计：SDK 通过两个 trait 定义存储契约，事件通过 `trait LoginEvents`（complete/
failed 回调）交给壳层决定如何推送（Tauri event / 其他）。

```rust
pub trait CredentialStore: Send + Sync {
    fn save(&self, account_id: &str, creds: &StoredCredentials) -> Result<(), String>;
    fn load(&self, account_id: &str) -> Result<StoredCredentials, String>;
    fn delete(&self, account_id: &str) -> Result<(), String>;
}

pub trait AccountStore: Send + Sync {
    fn upsert_account(&self, row: NewAccount) -> Result<AccountRow, String>;
    fn list_accounts(&self) -> Result<Vec<AccountRow>, String>;
    fn delete_account(&self, id: &str) -> Result<(), String>; // 级联删绑定
    fn upsert_binding(&self, b: NewBinding) -> Result<(), String>;
    fn list_bindings(&self) -> Result<Vec<BindingRow>, String>;
}
```

## 3. 迁移步骤（预计 2-3 天）

1. 建 Cargo workspace：`crates/ssy-core` + `src-tauri`（半天）
2. 搬 client/callback_server/models；auth_manager 改泛型 `<S: CredentialStore, A: AccountStore>`（半天）
3. 壳层实现 SQLite stores + events；commands 改调 SDK（半天）
4. 测试迁移：auth_manager/DAO 现有单测拆到两侧；补 SDK 集成测试（mockHTTP）（1 天）
5. 发布：私有 git tag `ssy-core v0.1`；crates.io 视开源决策

## 4. 触发条件（满足其一即执行）

- [ ] 出现第二个消费方（其他 Rust 项目要接胜算云 OAuth/余额）
- [ ] 上游合并时 `shengsuanyun/` 相关冲突 ≥ 2 次
- [ ] 需要 SDK 独立发版（壳与协议演化解耦）

## 5. 已知的接口事实（本蓝图引用，详见 ssy-oauth-contract.md）

- `/user/info` 只认 jwt（网关 Key 报 code 20003）；余额单位 1e-4 元
- loom `GET /users/me/balance` 接受 API Key（Bearer），单位 1e-7 元；
  **Key 归属独立账户**（不同 Key 余额不同是预期行为，2026-09-17 实测）
- 默认用量脚本：jwt 方案（`{{shengsuanyunJwt}}` + /user/info），balance API
  迁移已评估并于 2026-09-17 决定暂缓（Key↔账户归属问题待后端澄清）
