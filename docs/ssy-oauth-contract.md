# 胜算云 OAuth 接口契约（v0.1 · 待上游确认）

> 状态：**未与真实环境联调**。以下基于 LoomLoom webapp（`参考/auth_ssy.go`）逆向，
> 实现（`src-tauri/src/shengsuanyun/`）按此契约编写。W1 烟囱测试通过前不得对外承诺。

## 1. 授权入口

```
GET https://router.shengsuanyun.com/auth
    ?callback_url=http://127.0.0.1:<port>/auth/shengsuanyun/callback
    &state=<uuid-v4>
    &from=SSY_SWITCH
```

- 浏览器登录后跳转 `callback_url?code=xxx&state=<原样回传>`。
- **待确认**：上游是否支持 `state` 并原样回传（demo 未使用 state；客户端已强制校验）。

## 2. code 换 API Key

```
POST https://api.shengsuanyun.com/auth/keys
Content-Type: application/json

{ "code": "<oauth code>", "callback_url": "<与授权时一致>" }
```

响应层级不稳定，客户端做三层兜底：`data.data.api_key` / `data.api_key` / `api_key`。
code 一次性、短 TTL（推测 ≤5 min）。

## 3. 用户信息与余额

```
GET https://api.shengsuanyun.com/user/info
x-token: <api_key>
```

- 认证头是 **`x-token`**（不是 `Authorization: Bearer`——那是模型网关 router 的用法）。
- 字段大小写双兼容：`ID|id`、`Nickname|nickname`、`Email|email`、`HeadImg|photoUrl`。
- 余额：`data.Wallet.Assets`（整数资产单位）；展示层统一 `/10000` 转元，仅转换一次。

## 4. 创作者角色（可选探测）

```
GET https://loomloom.shengsuanyun.com/loom/v1/creators/me/marketListings?pageSize=100
x-token: <api_key>
```

`items` 非空 → isCreator。失败按 false 处理，不阻塞登录。

## 5. 客户端安全约束

| 项 | 值 |
|---|---|
| state | UUID v4，5 分钟 TTL，一次性消费，绑定回调端口 |
| 回调监听 | `127.0.0.1:0` 随机端口，独立于本地代理，仅 `/auth/shengsuanyun/callback` |
| API Key 存储 | OS Keychain（keyring crate），不入 SQLite/日志/事件 |
| 事件 payload | 仅脱敏账号（uid/email 打码） |
| 深链 | `ssyswitch://v1/oauth?provider=shengsuanyun&app={claude\|codex\|gemini}&from=<归因>`，白名单校验，前端确认后才启动 |

## 6. 待上游确认清单（阻塞项）

- [ ] `/auth` 是否支持并回传 `state`
- [ ] `/auth/keys` 在 loopback callback 下的真实响应结构与错误码
- [ ] `Wallet.Assets` 的精确单位与币种
- [ ] code TTL、单次使用、错误码语义
- [ ] 401/402/429/5xx 错误码与文案（用于错误诊断映射）
