// 充值回跳后的余额刷新协调器。
//
// 支付发生在系统浏览器，应用感知"用户回来了"的唯一可靠信号是 window focus。
// 这里记录"刚打开过充值页"的时间戳，聚焦时判断是否触发余额刷新——
// 避免无意义的常驻轮询：聚焦是事件驱动的，只在充值后的一段时间窗口内生效。

const KEY = "ssy:rechargeOpenedAt";

/** 打开充值页时调用（所有充值入口共用） */
export function markRechargeOpened(): void {
  try {
    sessionStorage.setItem(KEY, String(Date.now()));
  } catch {
    /* sessionStorage 不可用时静默：退化为无自动刷新 */
  }
}

/**
 * 窗口聚焦时调用：是否应该刷新余额。
 * 时间窗口：打开充值页 20 秒后 ~ 10 分钟内，且两次刷新至少间隔 20 秒。
 */
export function shouldRefreshOnFocus(now = Date.now()): boolean {
  try {
    const opened = Number(sessionStorage.getItem(KEY) || 0);
    if (!opened) return false;
    const sinceOpen = now - opened;
    return sinceOpen > 20_000 && sinceOpen < 600_000;
  } catch {
    return false;
  }
}
