// 胜算云账号登录/管理区块（Auth Center + 深链共用）。
//
// 状态机：idle → pending（等待浏览器回调）→ complete/failed。
// 结果由后端事件 shengsuanyun-oauth-complete / -failed 推送（payload 脱敏）。
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Loader2, LogIn, LogOut, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  shengsuanyunApi,
  type ShengsuanyunAccount,
} from "@/lib/api/shengsuanyun";
import { settingsApi } from "@/lib/api/settings";

type Phase = "idle" | "pending" | "error";

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

  const reload = useCallback(async () => {
    try {
      setAccounts(await shengsuanyunApi.listAccounts());
    } catch (e) {
      // 列表失败不阻塞 UI，下次事件/操作会再拉
      console.warn("list shengsuanyun accounts failed", e);
    }
  }, []);

  useEffect(() => {
    void reload();
    let offComplete: UnlistenFn | undefined;
    let offFailed: UnlistenFn | undefined;
    let offIntent: UnlistenFn | undefined;
    let disposed = false;

    (async () => {
      offComplete = await listen("shengsuanyun-oauth-complete", () => {
        setPhase("idle");
        setError(null);
        void reload();
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
    };
  }, [reload]);

  const startLogin = async (overrideApp?: string) => {
    setBusy(true);
    setError(null);
    try {
      const start = await shengsuanyunApi.startLogin(
        overrideApp ?? targetApp,
        null,
      );
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
              variant="outline"
              size="sm"
              disabled={busy}
              aria-label={t("shengsuanyun.logout", { defaultValue: "退出登录" })}
              onClick={() => logout(a.id)}
            >
              <LogOut className="h-4 w-4" />
            </Button>
          </div>
        </div>
      ))}

      {phase === "error" && error && (
        <p className="text-sm text-destructive" role="alert">
          {t("shengsuanyun.failed", { defaultValue: "登录失败" })}: {error}
        </p>
      )}
    </div>
  );
}
