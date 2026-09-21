<div align="center">

# SSY-Switch

### 胜算云官方桌面客户端 —— 登录一次，九大 AI 编程工具全部就绪

[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](../../releases)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-orange.svg)](https://tauri.app/)
[![SDK](https://img.shields.io/badge/SDK-ssy--core%20v0.2.2-green.svg)](https://github.com/SSYCloud/ssy-sdk-rust)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

基于 [CC Switch](https://github.com/farion1231/cc-switch) v3.20.2 构建 · 核心逻辑抽取为独立 SDK [ssy-core](https://github.com/SSYCloud/ssy-sdk-rust)

</div>

## 这是什么

SSY-Switch 是胜算云（Shengsuanyun）的官方桌面客户端。用胜算云账号登录一次，客户端自动把你的 API Key 绑定到 9 个 AI 编程工具，并在应用内完成余额查询、用量统计、账单流水和扫码充值 —— 不用再手动编辑任何 JSON / TOML / `.env` 配置文件。

**支持的 9 个宿主工具**：Claude Code · Claude Desktop · Codex · Gemini CLI · Grok Build · OpenCode · OpenClaw · Hermes · Pi

## 胜算云集成能力

### 一键登录与自动绑定

- **OAuth 登录**：Authorization Code + 本地 loopback 回调（随机端口、state 一次性消费、5 分钟 TTL），全程不经过第三方服务器
- **9 宿主自动绑定**：登录完成后自动把 API Key 写入全部 9 个工具的供应商配置；重复登录按 uid 幂等复用，绑定关系不丢失
- **多 Key 管理**：账号下多把 API Key 的脱敏列表 + 按需单把读取明文（列表永不返回明文，选择结果只落 `key_id` 标识）
- **从 CC Switch 迁移**：设置页一键导入已有 CC Switch 配置，老用户零成本切换

### 账户中心

- **余额与代金券**：钱包余额、代金券/体验券明细，实时刷新
- **用量统计**：按日/按模型的大模型调用统计、多模态调用统计，费用自动换算为元
- **账单流水**：充值与消费流水分页查询

### 应用内充值

- **扫码支付**：支付宝 / 微信两个通道，二维码 15 分钟倒计时，过期自动销毁订单
- **档位充值**：¥10 / ¥30 / ¥100 / ¥200 / ¥500 预设档位 + ¥30–¥5000 自定义金额
- **支付感知**：支付完成后自动刷新余额，无需手动查询

## 继承自 CC Switch 的核心能力

- **供应商管理**：50+ 预设，一键切换、系统托盘快捷切换、拖拽排序、导入导出
- **本地代理**：格式转换、自动故障转移、熔断器、供应商健康监控，支持按应用/按供应商接管
- **MCP / Prompts / Skills 统一管理**：跨应用双向同步，深链导入（`ssyswitch://`）
- **用量看板**：消费趋势、请求日志、按模型自定义价格
- **跨平台**：Windows / macOS / Linux，深色模式、i18n（zh/zh-TW/en/ja）、原子写入、自动备份

## 数据与安全

| 项目 | 位置 |
|---|---|
| 数据库（含凭据，与 CC Switch 同策略） | `~/.ssy-switch/cc-switch.db`（SQLite） |
| 本地设置 | `~/.ssy-switch/settings.json` |
| 自动备份 | `~/.ssy-switch/backups/`（每次启动与 schema 迁移各一份） |
| Skills | `~/.ssy-switch/skills/` |

凭据（API Key / JWT）存储在自家 SQLite 中（与 CC Switch 存储策略一致）；所有事件、日志、DTO 均为脱敏数据（uid/邮箱打码，Key 只显示掩码）。详见 [docs/ssy-oauth-contract.md](docs/ssy-oauth-contract.md)。

## 架构

```
┌──────────────────────────────────────────────┐
│           前端 React 18 + TypeScript          │
│   账户中心 · 充值对话框 · Key 选择器 · 供应商 UI  │
└──────────────────┬───────────────────────────┘
                   │ Tauri IPC
┌──────────────────▼───────────────────────────┐
│            后端 Tauri 2 + Rust                │
│  commands/shengsuanyun.rs —— 适配层（~700 行）  │
│  SQLite stores · 事件桥 · 用量脚本注入          │
└──────────────────┬───────────────────────────┘
                   │ 依赖（git tag 锁定）
┌──────────────────▼───────────────────────────┐
│   ssy-core SDK（独立仓库 SSYCloud/ssy-sdk-rust）│
│   OAuth · 双凭据 · 数据客户端 · 充值 · 规则层     │
└──────────────────────────────────────────────┘
```

胜算云的**全部核心逻辑**（OAuth、双凭据契约、余额/用量/账单数据客户端、充值流程、确定性规则层）都在独立 SDK [ssy-core](https://github.com/SSYCloud/ssy-sdk-rust) 中，本仓库只保留约 400 行宿主适配代码（SQLite 存储桥、事件桥、Tauri 命令）。API 契约的单一事实源是 [openapi/ssy-api.yaml](openapi/ssy-api.yaml)（OpenAPI 3.1）。

## 开发

### 环境要求

- Node.js 18+ · pnpm 8+
- Rust 1.85+ · Tauri CLI 2.8+

### 常用命令

```bash
pnpm install          # 安装依赖
pnpm dev              # 开发模式（热重载）
pnpm typecheck        # 类型检查
pnpm test:unit        # 前端测试（vitest + MSW）
pnpm build            # 构建应用

cd src-tauri
cargo test            # 后端测试（含 29 个胜算云相关测试）
cargo clippy          # Rust 静态检查
```

### 技术栈

**前端**：React 18 · TypeScript · Vite · TailwindCSS · TanStack Query v5 · react-i18next · shadcn/ui
**后端**：Tauri 2 · Rust · tokio · axum（loopback 回调）· SQLite
**SDK**：[ssy-core](https://github.com/SSYCloud/ssy-sdk-rust)（`git + tag` 依赖，含 wiremock 集成测试）

## 文档索引

| 文档 | 内容 |
|---|---|
| [openapi/ssy-api.yaml](openapi/ssy-api.yaml) | 胜算云 API 契约（OpenAPI 3.1，多语言单一事实源） |
| [docs/ssy-oauth-contract.md](docs/ssy-oauth-contract.md) | OAuth 接口契约、双凭据规则、安全约束、事故复盘 |
| [docs/ssy-local-recharge-spec.md](docs/ssy-local-recharge-spec.md) | 应用内充值规格（档位、支付通道、订单生命周期） |
| [docs/ssy-sdk-plan.md](docs/ssy-sdk-plan.md) | SDK 抽取与标准化计划 |
| [docs/proposal-compliance.md](docs/proposal-compliance.md) | 立项要求对照表 |
| [docs/known-issues.md](docs/known-issues.md) | 已知问题（含上游预存在问题） |
| [UPSTREAM_SYNC.md](UPSTREAM_SYNC.md) | 与上游 CC Switch 的同步维护流程 |

## 与上游 CC Switch 的关系

本项目 fork 自 [farion1231/cc-switch](https://github.com/farion1231/cc-switch) v3.20.2（MIT 协议，感谢作者 Jason Young），并在此基础上完成：

- 品牌与默认配置改造（胜算云入口、默认供应商、应用标识）
- 胜算云 OAuth 登录、9 宿主自动绑定、账户中心、应用内充值
- 核心逻辑抽取为独立 SDK `ssy-core`，宿主侧仅保留适配层

上游同步策略：保留完整 git 历史，`upstream` remote 指向原仓库，可随时 `git merge upstream/main` 合并上游更新，流程见 [UPSTREAM_SYNC.md](UPSTREAM_SYNC.md)。

## License

MIT © SSYCloud — 基于 [CC Switch](https://github.com/farion1231/cc-switch)（MIT © Jason Young）构建
