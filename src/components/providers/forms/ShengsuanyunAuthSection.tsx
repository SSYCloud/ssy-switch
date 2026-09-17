// 胜算云账号登录/管理区块（Auth Center + 深链共用）。
//
// 状态机：idle → pending（等待浏览器回调）→ complete/failed。
// 结果由后端事件 shengsuanyun-oauth-complete / -failed 推送（payload 脱敏）。
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Loader2, LogIn, LogOut, RefreshCw, Wallet } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  shengsuanyunApi,
  type ShengsuanyunAccount,
} from "@/lib/api/shengsuanyun";
import { invalidateSsyAccountsCache } from "./shared/SsyKeyPicker";
import { settingsApi } from "@/lib/api/settings";

type Phase = "idle" | "pending" | "error";

/// 胜算云充值页（P0 外跳方案；URL 不携带任何凭据，仅归因参数）
const SSY_RECHARGE_URL = "https://console.shengsuanyun.com/user/recharge?from=ssy_switch";

/// 402 / 余额不足错误特征（用于"前往充值"引导）
function isInsufficientBalance(error: unknown): boolean {
  return /402|insufficient|余额不足/i.test(String(error));
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
  const sessionIdRef = useRef<string | null>(null);
  // 最近一次登录携带的目标 app：OAuth 成功后自动绑定并激活该 app 的胜算云 Provider
  const bindAppRef = useRef<string | null>(targetApp ?? null);

  const rechargeOpenedAtRef = useRef<number>(0);
  const lastBalanceRefreshRef = useRef<number>(0);

  // 应用重新聚焦时自动刷新余额（充值回来即看到账）：
  // 仅在曾打开过充值页、且距上次刷新 > 20s 时触发，避免高频请求
  useEffect(() => {
    const onFocus = () => {
      if (rechargeOpenedAtRef.current === 0) return;
      if (Date.now() - lastBalanceRefreshRef.current < 20_000) return;
      const account = accountsRef.current[0];
      if (!account) return;
      lastBalanceRefreshRef.current = Date.now();
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
    // 首页横幅点击：携带目标 app 直接开始登录（用户点击即确认）
    const onLoginRequest = (ev: Event) => {
      const detail = (ev as CustomEvent<{ appId: string }>).detail;
      if (detail?.appId) bindAppRef.current = detail.appId;
      void startLoginRef.current?.(detail?.appId);
    };
    window.addEventListener("ssy-login-request", onLoginRequest);
    let offComplete: UnlistenFn | undefined;
    let offFailed: UnlistenFn | undefined;
    let offIntent: UnlistenFn | undefined;
    let disposed = false;

    (async () => {
      offComplete = await listen("shengsuanyun-oauth-complete", () => {
        setPhase("idle");
        setError(null);
        void reload();
        // 自动绑定：有目标 app 时只绑该 app；无目标（认证中心直接登录）时三端全绑
        void (async () => {
          try {
            const accounts = await shengsuanyunApi.listAccounts();
            const latest = accounts[accounts.length - 1];
            if (!latest) return;
            const bindApp = bindAppRef.current;
            const results = bindApp
              ? [await shengsuanyunApi.bindAccount(bindApp, latest.id)]
              : await shengsuanyunApi.bindAllApps(latest.id);
            if (results.some((r) => r.status === "conflict")) {
              setError(
                t("shengsuanyun.bindConflict", {
                  defaultValue:
                    "部分应用已有手动配置的 Key，未自动覆盖。请在供应商设置中确认后再绑定。",
                }),
              );
            }
          } catch (e) {
            setError(String(e));
          }
        })();
      });
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
      }
    })();

    return () => {
      disposed = true;
      offComplete?.();
      offFailed?.();
      offIntent?.();
      window.removeEventListener("ssy-login-request", onLoginRequest);
    };
  }, [reload]);

  const startLoginRef = useRef<
    ((app?: string) => Promise<void>) | null
  >(null);

  const startLogin = async (overrideApp?: string) => {
    setBusy(true);
    setError(null);
    try {
      const app = overrideApp ?? targetApp;
      bindAppRef.current = app ?? null;
      const start = await shengsuanyunApi.startLogin(app, null);
      sessionIdRef.current = start.sessionId;
      setPhase("pending");
      await settingsApi.openExternal(start.authorizationUrl);
    } catch (e) {
      setPhase("error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  startLoginRef.current = startLogin;

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
      await settingsApi.openExternal(SSY_RECHARGE_URL);
      rechargeOpenedAtRef.current = Date.now();
    } catch (e) {
      setError(String(e));
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
            </div>
          </div>
          <div className="flex flex-col items-end gap-2">
          <div className="flex items-center gap-2">
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

      {phase === "error" && error && (
        <div className="space-y-2" role="alert">
          <p className="text-sm text-destructive">
            {isInsufficientBalance(error)
              ? t("shengsuanyun.insufficientBalance", {
                  defaultValue: "余额不足，无法完成请求",
                })
              : `${t("shengsuanyun.failed", { defaultValue: "登录失败" })}: ${error}`}
          </p>
          {isInsufficientBalance(error) && (
            <Button size="sm" onClick={openRecharge} disabled={busy}>
              <Wallet className="mr-1 h-4 w-4" />
              {t("shengsuanyun.goRecharge", { defaultValue: "前往充值" })}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
