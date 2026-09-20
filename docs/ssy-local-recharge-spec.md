# SSY-Switch 应用内本地支付 Spec（P2 站内充值）

> 版本 v1.0 · 2026-09-19 · 状态：待评审
>
> 目标：把充值体验从「外跳云端控制台」（当前 P0 形态：打开浏览器 → 二次登录 → 网页支付）
> 升级为「**应用内本地支付**」——档位选择、收款二维码、轮询到账全部在应用内完成，
> 用户全程不离开 SSY-Switch。云端外跳**降级保留**为大额/对公/客服路径。

## 1. 已验证的上游契约（全部 2026-09-17/18 真实请求实测）

### 1.1 创建充值订单

```
POST https://api.shengsuanyun.com/user/recharge
x-token: <jwt_token>              # 网关 api_key 不参与支付流程
Content-Type: application/json
```

**两种互斥模式：**

| 模式 | 参数 | 行为 |
|---|---|---|
| **定额档位** | `recahrgeId: <id>` + `amounts: <档位表原样值>` | 服务端忽略传入金额，按套餐定价出单 |
| **自定义金额** | `recahrgeId: null` + `amounts: 元 × 10000` | 金额生效；**下限 ¥30 / 上限 ¥5000**（低于报 `code: 70002`） |

**档位对照表**（2026-09-18 控制台抓包，参数须逐字节照抄）：

| 档位 | recahrgeId | amounts |
|---:|---:|---:|
| ¥10 | 22 | 10,000 |
| ¥30 | 8 | 3,000,000,000 |
| ¥100 | 14 | 10,000,000,000 |
| ¥200 | 13 | 20,000,000,000 |
| ¥500 | 12 | 50,000,000,000 |

**响应**：`{code: 0, data: {url: <收款码链接, 如 qr.alipay.com/…>, payOrder: {OrderID, Price, Amount, Status: 0, …}}}`

关键规则：
- 服务端可能改写 `orderId` → **状态查询必须用响应中的 `payOrder.OrderID`**
- `Status: 0` = 未支付；未支付订单自动过期，不扣款

### 1.2 支付状态查询

```
POST https://api.shengsuanyun.com/user/payQuery
x-token: <jwt_token>
{ "orderId": "<payOrder.OrderID>" }
```

响应：`{code: 0, msg: "unpaid"}` —— `msg` 承载状态语义；`unpaid` 已实测，
**支付成功的文案待一次真实支付后回填**（见 §7 待办）。

### 1.3 到账验证（复用既有接口）

- `POST /userorder/billlist`：最新账单（`Asset`/`Balance` 单位 1e-4 元，
  `BillType: recharge`）
- `GET /api/v1/balance`：网关余额（**单位已是元**，`account_balance_cny`）

## 2. 应用内交互设计

### 2.1 入口

- 认证中心账号卡片「充值」按钮 → **打开充值对话框**（替代现外跳行为）
- Provider 卡片余额旁 CTA → 同上
- 外跳入口**不删除**：对话框底部保留「大额充值 / 对公转账 → 打开控制台」链接

### 2.2 充值对话框（新组件 `ShengsuanyunRechargeDialog.tsx`）

```
┌──────────────────────────────────────┐
│ 胜算云充值                    [关闭] │
│                                      │
│ [¥10] [¥30] [¥100] [¥200] [¥500]    │  ← 档位按钮（含档位表映射）
│ 自定义金额 [    30    ] 元            │  ← 30–5000 校验
│                                      │
│ ┌────────────────┐                   │
│ │   ▓▓ 二维码 ▓▓   │  支付宝扫一扫     │  ← 前端本地渲染（qrcode npm 包）
│ └────────────────┘                   │
│ 订单 8e763e… · 待支付 · 12s          │  ← 轮询计时
│                                      │
│ ✅ 充值 ¥30 已到账（余额 ¥59.96）     │  ← 支付成功态
│ ⏳ 二维码已过期 [刷新]                │  ← 超时态
│ ⚠ 最低充值 ¥30                       │  ← 70002 错误态
│                                      │
│ 大额充值 / 对公转账 → 打开控制台 ↗    │  ← 云端外跳兜底
└──────────────────────────────────────┘
```

### 2.3 状态机

```
idle ──创建订单──▶ pending(显示二维码, 每 3s 轮询)
                     │
        ┌────────────┼──────────────┐
        ▼            ▼              ▼
      paid        expired       error(70002/网络)
   刷新余额+     [重新生成]      [重试/外跳兜底]
   成功提示
```

- 轮询：`payQuery` 每 **3 秒**，最长 **5 分钟**（100 次）；`msg !== "unpaid"` 即视为终态
- 过期/超时：显示「刷新二维码」按钮 → 重新走创建流程（新订单）
- 支付成功判定：`msg !== "unpaid"` 即离轮询（成功文案待 §7 回填后精确化）；
  以 **billlist 最新 `recharge` 账单 + 网关余额**做二次确认

## 3. 架构与改动清单

### 3.1 规则层（ssy-core v0.1.3，纯函数可测）

```rust
// 档位表 + 请求构造（档位命中 → 档位表原样；自定义 → null + 元×1e4）
pub struct RechargeTier { pub yuan: f64, pub recahrge_id: i64, pub amounts: i64 }
pub const RECHARGE_TIERS: &[RechargeTier] = &[ …五档… ];
pub fn recharge_request(yuan: f64) -> Result<(i64 /*amounts*/, Option<i64> /*recahrgeId*/), RechargeError>;
// RechargeError::BelowMinimum / AboveMaximum
```

- 新增金样本测试：五档逐字节对照、70002 边界（29/30/5000/5001）

### 3.2 壳层命令（src-tauri）

| 命令 | 输入 | 输出 | 说明 |
|---|---|---|---|
| `shengsuanyun_create_recharge_order` | `yuan: f64` | `{url, order_id, price_yuan}` | 规则层构造 → client 调用；校验 30–5000 |
| `shengsuanyun_pay_status` | `order_id` | `{status: "unpaid"/…}` | payQuery 封装 |

（既有 `shengsuanyun_bill_list` / `shengsuanyun_refresh_balance` 复用）

### 3.3 前端

| 文件 | 改动 |
|---|---|
| `ShengsuanyunRechargeDialog.tsx`（新）| 档位/自定义输入、二维码渲染（`qrcode` npm 包，纯 JS 打进 bundle，离线可用）、轮询、三态 |
| `ShengsuanyunAuthSection.tsx` | 充值按钮从 `openRecharge()` 外跳改为打开对话框；外跳降级为对话框内链接 |
| `UsageFooter.tsx` 余额旁 CTA | 同上改开对话框 |
| `lib/api/shengsuanyun.ts` | 新增 `createRechargeOrder` / `payStatus` |

### 3.4 二维码渲染决策

- **前端 `qrcode` npm 包**（纯 JS，打包离线可用）——不用 CDN、不用服务端生成
  （web 测试页的服务端 SVG 方案在 Tauri 内无必要，且前端渲染避免多一层 IPC）

## 4. 安全约束

- 支付流程仅使用 `x-token`（jwt）；**api_key 不参与**，永不出现在订单请求
- `url`（收款码链接）仅在会话内存中传递，不落库、不写日志、不发埋点
- 埋点事件：`recharge_dialog_opened` / `recharge_qr_created(yuan)` /
  `recharge_completed(yuan)` / `recharge_expired`——不含金额以外的用户数据
- 金额上限 5000 的超额场景引导外跳对公（平台规则），客户端不做大额支付

## 5. 保留与降级（云端外跳不删除）

| 场景 | 路径 |
|---|---|
| ≤ ¥5000 | 应用内二维码（本 spec） |
| > ¥5000 / 企业充值 / 对公转账 | 外跳控制台（现 P0 行为） |
| 应用内支付失败 | 对话框内提供外跳链接兜底 |

## 6. 测试计划

| 层 | 用例 |
|---|---|
| 规则层（ssy-core） | 五档逐字节对照、自定义 30/5000 边界、70002 映射、金额换算 |
| 命令层（mock manager） | create 返回 url/order_id、异常透传 |
| 前端（vitest） | 对话框三态渲染、金额校验、轮询超时 |
| E2E（人工） | ¥30 真实支付 → 到账提示 → 余额刷新；二维码过期 → 刷新 |

## 7. 待办（不阻塞，需真实支付/后端确认）

- [ ] `payQuery` 支付成功的 `msg` 文案回填（下次真实支付时抓取）
- [ ] 微信渠道 `payWay` 取值（抓包控制台微信 tab）
- [ ] 订单过期时长（后端确认，影响「刷新」提示时机）
- [ ] 档位是否含赠送（`GiftAmount` 字段已在响应中，展示规则待产品确认）

## 8. 验收标准

1. 认证中心「充值」→ 对话框内完成 ¥30 支付 → 到账提示 + 余额刷新，**全程不离开应用**（扫码除外）
2. 五档按钮生成的请求与控制台抓包逐字节一致
3. 70002/网络错误有明确提示与恢复路径
4. 云端外跳入口保留（大额/对公）
5. 门禁全绿（Rust/前端/clippy），隐私扫描通过（无 Key/jwt 泄漏到日志）
