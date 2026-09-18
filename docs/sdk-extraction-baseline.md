# SSY SDK 抽离 · 测试基线记录（Phase 0）

> 分支：`ssy-sdk`（自 `ssy-switch/mvp` @ `7f2b7738` 切出）
> 日期：2026-09-18 · 依据：`docs/ssy-sdk-plan.md` v2 实施手册

## 1. 基线门禁结果（全部通过）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --check` | ✅ |
| `cargo clippy` | ✅ 0 警告 |
| `cargo test` | ✅ 2872 passed / 0 failed |
| `pnpm typecheck` | ✅ |
| `pnpm test:unit` | ✅ 129 文件 / 910 用例 |
| 应用构建 | ✅（macOS AS，16:59 构建，已装 /Applications） |

**回归标准**：抽离过程中任一阶段结束，以上门禁必须全绿且用例数不减
（迁移的用例在目标 crate 中等价存在）。

## 2. 搬移清单（manifest）

### 移入 `crates/ssy-core`（无 tauri/rusqlite 依赖）
| 文件 | 行数 | 处理 |
|---|---:|---|
| `shengsuanyun/client.rs` | ~300 | 直搬（对齐 `openapi/ssy-api.yaml`）|
| `shengsuanyun/callback_server.rs` | ~130 | 泛型化（manager 经 trait 注入）|
| `shengsuanyun/models.rs`（DTO/脱敏/换算） | ~300 | 直搬；默认用量脚本部分拆出留壳 |
| `shengsuanyun/auth_manager.rs` | ~400 | 泛型化 `<S, A, E>` |
| `shengsuanyun/credential_store.rs` → trait | 61 | trait + StoredCredentials |

### 留在壳层
| 文件 | 原因 |
|---|---|
| `database/dao/shengsuanyun.rs` | 实现 AccountStore（rusqlite）|
| `commands/shengsuanyun.rs` | Tauri 薄壳 + bind（ProviderService 依赖）|
| `shengsuanyun/usage_script.rs`（新）| 默认用量脚本 meta（依赖 ProviderMeta）|
| `shengsuanyun/grok_build.rs` | Grok 专属，决策：留壳（SDK 仅胜算云域）|
| `lib.rs` 启动对账 | ProviderService 依赖 |
| 前端全部组件/flow/rechargeTracker | 表现层 + Tauri IPC |

## 3. 阶段决策记录

1. **错误类型**：v0.1 保持 `Result<_, String>` 与现状 1:1；`SsyError` 类型 v0.2 引入
2. **grok_build.rs**：留壳（Grok 专属，不属胜算云通用域）
3. **契约源**：`openapi/ssy-api.yaml` 为 client 行为的对照标准，Phase 4 增加契约测试
4. **发布**：workspace path 依赖开发；tag `ssy-core-v0.1.0` 后按需转 git 依赖
5. **组件/前端不抽**：表现层明确排除在 SDK 外

## 4. 阶段计划与门禁

| 阶段 | 内容 | 提交边界 |
|---|---|---|
| P1 | workspace 骨架 | `cargo check --workspace` 绿 |
| P2 | 零依赖模块直搬 | ssy-core 单测绿 |
| P3 | trait 反转 + 壳层适配 | 全量门禁绿 |
| P4 | 测试迁移 + 契约测试（对照 openapi） | 全量门禁绿 |
| P5 | tag v0.1.0 + 重打包冒烟 | 安装版验证 |

每阶段一个提交，失败即 `git revert` 至上一提交。
