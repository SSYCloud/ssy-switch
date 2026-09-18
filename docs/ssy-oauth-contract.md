# 胜算云 OAuth 接口契约（v0.2 · 登录链路已联调）

> 状态：**登录链路已与真实环境联调通过（2026-09-16）**；其余条目基于 LoomLoom webapp
> （`参考/auth_ssy.go`）逆向，实现（`src-tauri/src/shengsuanyun/`）按此契约编写。
> 标注「实测」的条目为当日真实请求观测，未标注的仍待上游确认。

## 1. 授权入口

```
GET https://router.shengsuanyun.com/auth
    ?callback_url=http://127.0.0.1:<port>/auth/shengsuanyun/callback
    &state=<uuid-v4>
    &from=SSY_SWITCH
```

- 浏览器登录后跳转 `callback_url?code=xxx&state=<原样回传>`。
- **实测**：`state` 原样回传，登录链路端到端跑通（`callback_server.rs` 的强校验因此可以常开）。

## 2. code 换 API Key

```
POST https://api.shengsuanyun.com/auth/keys
Content-Type: application/json

{ "code": "<oauth code>", "callback_url": "<与授权时一致>" }
```

响应层级不稳定，客户端做三层兜底：`data.data.api_key` / `data.api_key` / `api_key`。
code 一次性、短 TTL（推测 ≤5 min）。

响应**同时下发两把凭据**，用途不同，切勿混用：

| 字段 | 用途 |
|---|---|
| `api_key` | 模型网关调用（`Authorization: Bearer`），写入供应商配置 |
| `jwt_token` | 胜算云用户 API（`x-token` 头）：`/user/info`、`/token/list`、`/auth/keys` |

`jwt_token` 缺失时退回 `api_key`（见 `SsyCredentials::identity_token`）。
**实测**：响应含 `expires_in`（单次取样 ≈593400s ≈ 6.9 天）；到期后的续期路径尚未验证，
当前按「重新走一次 OAuth」处理。

## 3. 用户信息与余额

```
GET https://api.shengsuanyun.com/user/info
x-token: <jwt_token>          # 优先 jwt_token；缺失时退回 api_key
```

- 认证头是 **`x-token`**（不是 `Authorization: Bearer`——那是模型网关 router 的用法）。
- 与 `/token/list` 同一套认证：`api_key` 在部分用户接口上会被判为非法 token（见 §5），
  因此用户信息统一走 `identity_token()`。
- 字段大小写双兼容：`ID|id`、`Nickname|nickname`、`Email|email`、`HeadImg|photoUrl`。
- 余额：`data.Wallet.Assets`（整数资产单位）；展示层统一 `/10000` 转元，仅转换一次。

## 4. 创作者角色（可选探测）

```
GET https://loomloom.shengsuanyun.com/loom/v1/creators/me/marketListings?pageSize=100
x-token: <api_key>
```

`items` 非空 → isCreator。失败按 false 处理，不阻塞登录。

## 5. 账号下多 Key（`GET /token/list`）

```
GET https://api.shengsuanyun.com/token/list
x-token: <jwt_token>          # 必须用 jwt_token
```

- **实测**：用 `api_key` 调用会返回业务错误 `20003 That's not even a token`（HTTP 200 + `code != 0`）。
- **实测**：响应为 `{ code: 0, data: [ ... ] }`，`data` 直接是数组（兼容层仍保留
  `data.list` / `data.items` / `data.data` 兜底）。业务码非 0 一律按失败处理。
- 单条字段：`ID` / `Name` / `Desc` / `IsDefault` / `IsBanned` / `IsExpired` /
  `MaxQuota` / `ConsumedAmount` / `CreatedAt` / `ExpiresAt` / `SupportedModels`。
  数字与字符串混用，全部走柔性解析。
- `Token` 字段是**完整明文 Key**。客户端只在用户显式选择某一把时按需读取
  （`shengsuanyun_reveal_key`），列表接口只回脱敏元数据 —— 不整表落库、不写日志、
  不一次性灌给前端。
- 单位：`MaxQuota` / `ConsumedAmount` 与 `Wallet.Assets` 同单位，展示层统一 `/10000` 转元。
- **实测**：`MaxQuota == 0` 表示**无上限**（不是「额度为 0」），UI 显示「无上限」。
  解析时不可用 `!= 0.0` 判断字段是否存在。
- `SupportedModels` 是 **JSON 字符串**（也可能是数组），解析失败按空集合处理。
- 选择结果只落 `shengsuanyun_bindings.key_id` 这个标识；重登/启动时按 `key_id` 优先复用，
  失效才回落到账号默认 Key。

## 5b. 充值下单与支付状态（2026-09-18 实测，站内充值 P2 数据源）

```
POST https://api.shengsuanyun.com/user/recharge      x-token: <jwt_token>
POST https://api.shengsuanyun.com/user/payQuery      x-token: <jwt_token>
```

- **两种互斥模式**：`recahrgeId: null` + `amounts`（1e-4 元）= 自定义金额，
  **最低 ¥30**（低于报 `code: 70002`）；`recahrgeId: 22` = 定额 ¥10 套餐
  （服务端忽略 amounts，传任意值 Price 均 100000）
- 下单响应 `data.url` 为收款码链接（如 `https://qr.alipay.com/...`），
  渲染二维码即可收款；未支付订单自动过期不扣款
- `payQuery` 按 `data.payOrder.OrderID` 查询（注意不是客户端生成的 orderId）；
  `msg` 承载状态：`unpaid` = 未支付，支付成功为其他值（成功文案待实测）
- 已知未确认：微信渠道 `payWay` 取值、套餐目录接口、订单过期时长

## 6. 客户端安全约束

| 项 | 值 |
|---|---|
| state | UUID v4，5 分钟 TTL，一次性消费，绑定回调端口 |
| 回调监听 | `127.0.0.1:0` 随机端口，独立于本地代理，仅 `/auth/shengsuanyun/callback` |
| API Key 存储 | 自家 SQLite `shengsuanyun_credentials` 表（与 CC Switch 一致，用户决策 2026-09-16）；**注意：DB 导出/云同步会包含凭据** |
| 事件 payload | 仅脱敏账号（uid/email 打码） |
| 深链 | `ssyswitch://v1/oauth?provider=shengsuanyun&app={claude\|codex\|gemini}&from=<归因>`，白名单校验，前端确认后才启动 |

## 7. 本地数据安全约束（2026-09-16 事故复盘）

本轮出现过一次**真实用户数据被删**的事故，两条根因都在「uid 为空」上，规则固化如下：

| 规则 | 原因 |
|---|---|
| **空 uid ≠ 脏数据** | 旧版本把上游数字型 `data.ID` 解析成空串，登录产生的账号 uid 恒为空；但这些账号带真实凭据与宿主绑定。启动对账**不得**按 uid 为空删除账号，只能用已存凭据重查 `/user/info` 回填（`ShengsuanyunAuthManager::backfill_empty_uid_accounts`） |
| **uid 不能无条件当删除键** | `upsert` 里 `DELETE ... WHERE uid = ?` 必须带 `!uid.is_empty()` 保护；否则一次空 uid 写入会匹配并删除**所有**历史空 uid 行 |
| **schema v21：`uid` 不再列级 UNIQUE** | 旧约束下 `INSERT OR REPLACE` 会用新空 uid 行顶掉旧空 uid 行，静默删账号。改为部分唯一索引 `idx_shengsuanyun_accounts_uid_nonempty ... WHERE uid <> ''`，既允许历史空 uid 多行共存，又保住「一个上游账号一行」 |
| **启动/迁移前自动备份** | `~/.ssy-switch/backups/`（每次启动、每次 schema 迁移各一份），事故恢复就是靠它 |

## 8. 待上游确认清单

- [x] `/auth` 支持并回传 `state`（2026-09-16 实测）
- [ ] `/auth/keys` 在 loopback callback 下的真实响应结构与错误码
- [x] `Wallet.Assets` 的精确单位与币种（2026-09-18 交叉验证：billlist 中
      `Asset=100000` ↔ 实际充值 ¥10、`Balance=299591` ↔ `/api/v1/balance`
      `account_balance_cny=29.9591`，三者同口径确认为 **1e-4 元、CNY**；
      官方 balance-api 文档亦定义 `*_cny` 字段单位为元）
- [x] 官方余额接口 `GET /api/v1/balance`（BearerKey，单位元，普通/企业网关自动区分）
      —— 见 `openapi/ssy-api.yaml`（2026-09-18，lean.shengsuanyun.com balance-api 页）
- [x] `/user/info` 的用户标识字段：`data.ID`（**数字**，实测 62890）。
      客户端已按数字/字符串双兼容解析（`pick_id_str`），此前 `as_str` 静默取空串会误删其它账号
- [ ] `jwt_token` 过期后的续期路径（是否可用 `api_key` 换新 `jwt_token`）
- [ ] code TTL、单次使用、错误码语义
- [ ] 401/402/429/5xx 错误码与文案（用于错误诊断映射）
