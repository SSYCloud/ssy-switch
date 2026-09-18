// 胜算云账号登录/管理区块（Auth Center + 深链共用）。
//
// 状态机：idle → pending（等待浏览器回调）→ complete/failed。
// 结果由后端事件 shengsuanyun-oauth-complete / -failed 推送（payload 脱敏）。
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { BarChart3, Loader2, LogIn, LogOut, RefreshCw, Ticket, Wallet } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  shengsuanyunApi,
  type ShengsuanyunAccount,
} from "@/lib/api/shengsuanyun";
import { invalidateSsyAccountsCache } from "./shared/SsyKeyPicker";
import { startLoginFlow } from "@/lib/shengsuanyunFlow";
import { analyticsApi } from "@/lib/api/analytics";
import { settingsApi } from "@/lib/api/settings";
import { ShengsuanyunUsageStats } from "./ShengsuanyunUsageStats";
import { ShengsuanyunBillList } from "./ShengsuanyunBillList";
import { ShengsuanyunVoucherList } from "./ShengsuanyunVoucherList";
import { SSY_RECHARGE_URL } from "@/config/constants";
import {
  markRechargeOpened,
  shouldRefreshOnFocus,
} from "@/lib/rechargeTracker";

type Phase = "idle" | "pending" | "error";

/** 胜算云错误分类（立项 P0 错误诊断映射） */
type SsyErrorKind = "insufficient" | "relogin" | "ratelimit" | "upstream" | "unknown";
function classifySsyError(error: unknown): SsyErrorKind {
  const raw = String(error);
  if (/402|insufficient|余额不足/i.test(raw)) return "insufficient";
  if (/401|token invalid|token expired|unauthorized|凭据失效/i.test(raw)) return "relogin";
  if (/429|rate limit/i.test(raw)) return "ratelimit";
  if (/5\d\d|bad gateway|service unavailable|暂时不可用/i.test(raw)) return "upstream";
  return "unknown";
}

interface OAuthIntent {
  provider: string;
  appType: string;
  from: string;
}

interface Props {
  /** 登录成功后要绑定/激活的 app（claude/codex/gemini）；未指定则只登录不切换 */
  targetApp?: string | null;
}

export function ShengsuanyunAuthSection({ targetApp = null }: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<ShengsuanyunAccount[]>([]);
  const [busy, setBusy] = useState(false);
  const [intent, setIntent] = useState<OAuthIntent | null>(null);
  const [statsOpen, setStatsOpen] = useState(false);
  const [billsOpen, setBillsOpen] = useState(false);
  const [vouchersOpen, setVouchersOpen] = useState(false);
  const [health, setHealth] = useState<{ app: string; ok: boolean } | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  // 最近一次登录携带的目标 app：OAuth 成功后自动绑定并激活该 app 的胜算云 Provider
  const bindAppRef = useRef<string | null>(targetApp ?? null);

  const lastBalanceRefreshRef = useRef<number>(0);

  // 应用重新聚焦时自动刷新余额（充值回来即看到账）：
  // 仅在曾打开过充值页、且距上次刷新 > 20s 时触发，避免高频请求
  useEffect(() => {
    const onFocus = () => {
      const now = Date.now();
      if (!shouldRefreshOnFocus(now)) return;
      if (now - lastBalanceRefreshRef.current < 20_000) return;
      const account = accountsRef.current[0];
      if (!account) return;
      lastBalanceRefreshRef.current = now;
      void refreshBalanceSilent(account.id);
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const reload = useCallback(async () => {
    // 账号集合发生变化（登录/登出）：让供应商表单里的 Key 选择器重新读取
    invalidateSsyAccountsCache();
    try {
      setAccounts(await shengsuanyunApi.listAccounts());
    } catch (e) {
      // 列表失败不阻塞 UI，下次事件/操作会再拉
      console.warn("list shengsuanyun accounts failed", e);
    }
  }, []);

  const accountsRef = useRef<ShengsuanyunAccount[]>([]);
  useEffect(() => {
    accountsRef.current = accounts;
  }, [accounts]);

  const refreshBalanceSilent = useCallback(async (accountId: string) => {
    try {
      await shengsuanyunApi.refreshBalance(accountId);
      await reload();
    } catch {
      /* 静默：余额刷新失败不打扰用户 */
    }
  }, [reload]);

  useEffect(() => {
    void reload();
    // 登录流程由全局桥发起（横幅/本面板/深链共用）：这里只跟进 pending 态
    const onPending = (ev: Event) => {
      const detail = (ev as CustomEvent<{ sessionId: string }>).detail;
      sessionIdRef.current = detail.sessionId;
      setPhase("pending");
      setError(null);
    };
    window.addEventListener("ssy-login-pending", onPending);
    const onLoginRequestCompat = (ev: Event) => {
      const detail = (ev as CustomEvent<{ appId: string }>).detail;
      if (detail?.appId) bindAppRef.current = detail.appId;
      void startLoginFlow(detail?.appId).catch((e) => {
        setPhase("error");
        setError(String(e));
      });
    };
    window.addEventListener("ssy-login-request", onLoginRequestCompat);
    let offComplete: UnlistenFn | undefined;
    let offFailed: UnlistenFn | undefined;
    let offIntent: UnlistenFn | undefined;
    let offConflict: UnlistenFn | undefined;
    let offHealth: UnlistenFn | undefined;
    let disposed = false;

    (async () => {
      offComplete = await listen("shengsuanyun-oauth-complete", () => {
        setPhase("idle");
        setError(null);
        void reload();
        // 自动绑定由全局桥（shengsuanyunFlow）处理
      });
      offConflict = await listen("ssy-bind-conflict", () => {
        setError(
          t("shengsuanyun.bindConflict", {
            defaultValue:
              "部分应用已有手动配置的 Key，未自动覆盖。请在供应商设置中确认后再绑定。",
          }),
        );
      });
      // 健康检查结果（绑定后自动探测）
      offHealth = await listen<{ appType: string; ok: boolean; message: string }>(
        "ssy-health-checked",
        (e) => {
          setHealth({ app: e.payload.appType, ok: e.payload.ok });
        },
      );
      offFailed = await listen<{ sessionId: string; reason: string }>(
        "shengsuanyun-oauth-failed",
        (e) => {
          setPhase("error");
          setError(e.payload.reason);
        },
      );
      // Web 深链（ssyswitch://v1/oauth）到达时展示确认横幅，不静默启动 OAuth
      offIntent = await listen<OAuthIntent>("ssy-oauth-intent", (e) => {
        setIntent(e.payload);
      });
      if (disposed) {
        offComplete();
        offFailed();
        offIntent();
        offConflict();
      }
    })();

    return () => {
      disposed = true;
      offComplete?.();
      offFailed?.();
      offIntent?.();
      offConflict?.();
      offHealth?.();
      window.removeEventListener("ssy-login-pending", onPending);
      window.removeEventListener("ssy-login-request", onLoginRequestCompat);
    };
  }, [reload]);

  const startLogin = async (overrideApp?: string) => {
    setBusy(true);
    setError(null);
    try {
      const app = overrideApp ?? targetApp;
      bindAppRef.current = app ?? null;
      await startLoginFlow(app, "auth_center");
      // pending 态由 ssy-login-pending 事件驱动（flow 内已派发）
    } catch (e) {
      setPhase("error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const cancelLogin = async () => {
    if (sessionIdRef.current) {
      try {
        await shengsuanyunApi.cancelLogin(sessionIdRef.current);
      } catch {
        /* 服务端 state 5 分钟后自动过期 */
      }
    }
    sessionIdRef.current = null;
    setPhase("idle");
  };

  const refreshBalance = async (accountId: string) => {
    setBusy(true);
    lastBalanceRefreshRef.current = Date.now();
    try {
      await shengsuanyunApi.refreshBalance(accountId);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const logout = async (accountId: string) => {
    setBusy(true);
    try {
      await shengsuanyunApi.logout(accountId);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  /// 打开充值页（系统浏览器），记录时间供聚焦后刷新判断
  const openRecharge = async () => {
    try {
      markRechargeOpened();
      void analyticsApi.track("recharge_clicked", { entry: "auth_center" }).catch(() => {});
      await settingsApi.openExternal(SSY_RECHARGE_URL);
    } catch (e) {
      setError(String(e));
    }
  };

  const describeError = (error: string): { text: string; action: "recharge" | "relogin" | null } => {
    switch (classifySsyError(error)) {
      case "insufficient":
        return {
          text: t("shengsuanyun.insufficientBalance", { defaultValue: "余额不足，无法完成请求" }),
          action: "recharge",
        };
      case "relogin":
        return {
          text: t("shengsuanyun.credentialsExpired", {
            defaultValue: "登录凭据已失效，请重新登录",
          }),
          action: "relogin",
        };
      case "ratelimit":
        return {
          text: t("shengsuanyun.ratelimited", { defaultValue: "请求过于频繁，请稍后重试" }),
          action: null,
        };
      case "upstream":
        return {
          text: t("shengsuanyun.upstreamDown", {
            defaultValue: "胜算云服务暂时不可用，请稍后重试",
          }),
          action: null,
        };
      default:
        return { text: error, action: null };
    }
  };

  // 手动绑定：查找或创建目标 App 的胜算云 Provider，写入 Key 并激活
  const bindToApp = async (appType: string, accountId: string) => {
    setBusy(true);
    setError(null);
    try {
      const result = await shengsuanyunApi.bindAccount(appType, accountId);
      if (result.status === "conflict") {
        setError(
          t("shengsuanyun.bindConflict", {
            defaultValue:
              "该应用已有手动配置的 Key，未自动覆盖。请重试并选择覆盖，或在供应商设置中确认。",
          }),
        );
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const loginPending = phase === "pending";

  return (
    <div className="space-y-4">
      {intent && !loginPending && (
        <div className="space-y-2 rounded-lg border border-primary/40 bg-primary/5 p-4 text-sm">
          <p>
            {t("shengsuanyun.intentPrompt", {
              defaultValue:
                "收到来自胜算云 Web 的登录请求，是否开始登录并绑定？",
            })}
          </p>
          <div className="flex gap-2">
            <Button
              size="sm"
              disabled={busy}
              onClick={() => {
                const app = intent.appType || null;
                setIntent(null);
                void startLogin(app ?? undefined);
              }}
            >
              {t("shengsuanyun.intentConfirm", { defaultValue: "开始登录" })}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setIntent(null)}
            >
              {t("shengsuanyun.intentCancel", { defaultValue: "忽略" })}
            </Button>
          </div>
        </div>
      )}

      {accounts.length === 0 && !loginPending && (
        <Button onClick={() => startLogin()} disabled={busy}>
          {busy ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <LogIn className="mr-2 h-4 w-4" />
          )}
          {t("shengsuanyun.login", {
            defaultValue: "用胜算云账号登录（无需复制 Key）",
          })}
        </Button>
      )}

      {loginPending && (
        <div className="space-y-2 rounded-lg border border-border/60 bg-muted/40 p-4 text-sm">
          <div className="flex items-center gap-2">
            <Loader2 className="h-4 w-4 animate-spin" />
            <span>
              {t("shengsuanyun.pending", {
                defaultValue: "已完成浏览器跳转？请在浏览器中完成登录…",
              })}
            </span>
          </div>
          <Button variant="outline" size="sm" onClick={cancelLogin}>
            {t("shengsuanyun.cancel", { defaultValue: "取消登录" })}
          </Button>
        </div>
      )}

      {accounts.map((a) => (
        <div
          key={a.id}
          className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border/60 p-4"
        >
          <div className="min-w-0">
            <div className="truncate font-medium">
              {a.displayName || a.uidMasked}
              {a.isCreator && (
                <span className="ml-2 text-xs text-muted-foreground">
                  Creator
                </span>
              )}
            </div>
            <div className="text-sm text-muted-foreground">
              {a.emailMasked && <span className="mr-3">{a.emailMasked}</span>}
              <span>
                {t("shengsuanyun.balance", { defaultValue: "余额" })}:{" "}
                {a.balanceYuan != null
                  ? `¥${a.balanceYuan.toFixed(2)}`
                  : "—"}
              </span>
              {a.voucherYuan != null && (
                <span className="ml-2">
                  {t("shengsuanyun.voucher", { defaultValue: "体验券" })}:{" "}
                  ¥{a.voucherYuan.toFixed(2)}
                </span>
              )}
            </div>
          </div>
          <div className="flex flex-col items-end gap-2">
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              aria-label={t("shengsuanyun.voucherList", { defaultValue: "代金券" })}
              title={t("shengsuanyun.voucherList", { defaultValue: "代金券" })}
              onClick={() => {
                setVouchersOpen((v) => !v);
                setStatsOpen(false);
                setBillsOpen(false);
              }}
            >
              <Ticket className="h-4 w-4" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              aria-label={t("shengsuanyun.usageStats", { defaultValue: "调用统计" })}
              onClick={() => {
                setStatsOpen((v) => !v);
                setBillsOpen(false);
                setVouchersOpen(false);
                void analyticsApi.track("usage_stats_opened").catch(() => {});
              }}
            >
              <BarChart3 className="h-4 w-4" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              aria-label={t("shengsuanyun.billList", { defaultValue: "充值记录" })}
              onClick={() => {
                setBillsOpen((v) => !v);
                setStatsOpen(false);
                setVouchersOpen(false);
              }}
            >
              {t("shengsuanyun.billList", { defaultValue: "充值记录" })}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              aria-label={t("shengsuanyun.refreshBalance", {
                defaultValue: "刷新余额",
              })}
              onClick={() => refreshBalance(a.id)}
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => startLogin()}
            >
              {t("shengsuanyun.relogin", { defaultValue: "重新登录" })}
            </Button>
            <Button
              size="sm"
              className="bg-emerald-600 hover:bg-emerald-500"
              disabled={busy}
              aria-label={t("shengsuanyun.recharge", { defaultValue: "充值" })}
              onClick={openRecharge}
            >
              <Wallet className="mr-1 h-4 w-4" />
              {t("shengsuanyun.recharge", { defaultValue: "充值" })}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              aria-label={t("shengsuanyun.logout", { defaultValue: "退出登录" })}
              onClick={() => logout(a.id)}
            >
              <LogOut className="h-4 w-4" />
            </Button>
          </div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span>{t("shengsuanyun.bindTo", { defaultValue: "绑定并激活" })}:</span>
            {(["claude", "codex", "gemini"] as const).map((appType) => (
              <Button
                key={appType}
                variant="secondary"
                size="sm"
                className="h-6 px-2 text-xs"
                disabled={busy}
                onClick={() => bindToApp(appType, a.id)}
              >
                {appType === "claude"
                  ? "Claude"
                  : appType === "codex"
                    ? "Codex"
                    : "Gemini"}
              </Button>
            ))}
          </div>
          </div>
        </div>
      ))}

      {/* 额度不足提醒（立项 P0：额度提醒） */}
      {accounts.some((a) => a.balanceYuan != null && a.balanceYuan < 10) && (
        <div className="flex items-center justify-between gap-3 rounded-lg border border-orange-500/30 bg-orange-500/10 px-4 py-3 text-sm text-orange-700 dark:text-orange-300">
          <span>
            {t("shengsuanyun.lowBalance", {
              defaultValue: "余额较低，可能很快用尽",
            })}
          </span>
          <Button variant="outline" size="sm" onClick={openRecharge} disabled={busy}>
            <Wallet className="mr-1 h-3.5 w-3.5" />
            {t("shengsuanyun.goRecharge", { defaultValue: "前往充值" })}
          </Button>
        </div>
      )}

      {/* 绑定后的健康检查结果（一键测试） */}
      {health && (
        <div
          className={`rounded-lg border px-4 py-3 text-sm ${
            health.ok
              ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
              : "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
          }`}
        >
          {health.ok
            ? t("shengsuanyun.healthOk", {
                defaultValue: "连通性检查通过，{{app}} 可以正常调用胜算云",
                app: health.app,
              })
            : t("shengsuanyun.healthFail", {
                defaultValue:
                  "{{app}} 连通性检查未通过，请检查网络或重新登录后重试",
                app: health.app,
              })}
        </div>
      )}

      {statsOpen && <ShengsuanyunUsageStats onClose={() => setStatsOpen(false)} />}
      {billsOpen && <ShengsuanyunBillList onClose={() => setBillsOpen(false)} />}
      {vouchersOpen && (
        <ShengsuanyunVoucherList onClose={() => setVouchersOpen(false)} />
      )}

      {phase === "error" && error && (
        <div className="space-y-2" role="alert">
          <p className="text-sm text-destructive">
            {describeError(error).text ||
              `${t("shengsuanyun.failed", { defaultValue: "登录失败" })}: ${error}`}
          </p>
          {describeError(error).action === "recharge" && (
            <Button size="sm" onClick={openRecharge} disabled={busy}>
              <Wallet className="mr-1 h-4 w-4" />
              {t("shengsuanyun.goRecharge", { defaultValue: "前往充值" })}
            </Button>
          )}
          {describeError(error).action === "relogin" && (
            <Button size="sm" onClick={() => startLogin()} disabled={busy}>
              <LogIn className="mr-1 h-4 w-4" />
              {t("shengsuanyun.relogin", { defaultValue: "重新登录" })}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
