# SSY-Switch 上游同步策略

## 基线

- 上游仓库：`https://github.com/farion1231/cc-switch`（remote：`upstream`）
- 基线版本：v3.20.2
- 基线提交：`f3b18df1`（main）
- 开发分支：`ssy-switch/mvp`
- 备注：公司 fork 的 remote 地址确认后，将 `origin` 指向 fork（当前 `origin` 仍指向上游，仅作占位）。

## 同步频率

- 每周检查一次上游 release；有安全修复或重大 bugfix 时立即评估 cherry-pick。
- 同步命令：

```bash
git fetch upstream
git checkout ssy-switch/mvp
git merge upstream/main   # 或 rebase，见下
```

## 冲突处理原则

- SSY-Switch 改动尽量**新增文件**（`shengsuanyun/` 模块、独立 command、独立 DB 表），少改上游核心（`proxy/`、`services/`）。
- 必须修改上游文件时，改动保持最小、局部、带 `// SSY-Switch:` 注释标记，便于同步时定位。
- 冲突优先保留上游行为，再重新套 SSY 差异。

## cherry-pick 原则

- 上游 bugfix / provider preset 更新 / 翻译更新：cherry-pick。
- 上游大重构（如 OAuth 框架改版）：评估后整体合并，SSY 模块随之适配。
- 不 cherry-pick 上游发行相关提交（签名、release workflow 按钮等）——SSY-Switch 有独立发行身份。

## 不回传上游的内容

- 品牌改造（名称、图标、identifier、数据目录）
- 胜算云专属 OAuth 模块与 UI（如上游接受 PR，可另行抽出精简版回传）
- 胜算云发行/更新配置
