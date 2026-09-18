// 胜算云登录全局流程（单例桥）。
//
// 设计原因：登录可以从多个入口发起（首页横幅、认证中心、深链确认），
// 而"OAuth 完成后自动绑定目标 App"必须**始终有人监听**——不能依赖
// 认证中心面板恰好挂载着。这里用模块级状态 + App 挂载时安装的全局
// 事件桥保证任意入口发起的登录完成后都会自动绑定。
import { invoke } from "@tauri-apps/api/core";
import { settingsApi } from "./api/settings";
import { analyticsApi } from "./api/analytics";
import { streamCheckProvider } from "./api/connectivity-check";
import type { AppId } from "./api/types";
import {
  bindShengsuanyunAccount,
  bindShengsuanyunAllApps,
  listShengsuanyunAccounts,
} from "./api/shengsuanyun";

/** 最近一次登录携带的目标 app（OAuth 完成后绑定用；null = 三端全绑） */
let lastBindApp: string | null = null;

/** 桥是否已安装（App 生命周期内一次） */
let bridgeInstalled = false;

/**
 * 发起登录：创建 loopback 回调 + state，打开系统浏览器授权页。
 * 登录结果由 installGlobalLoginBridge 安装的 complete 监听处理。
 */
export async function startLoginFlow(
  appId?: string | null,
  source = "unknown",
): Promise<{ authorizationUrl: string; sessionId: string }> {
  lastBindApp = appId ?? null;
  void analyticsApi.track("login_started", { app: appId ?? null, source });
  const start = await invoke<{
    authorizationUrl: string;
    sessionId: string;
  }>("shengsuanyun_start_login", {
    targetApp: appId ?? null,
    targetProviderId: null,
  });
  // 通知 UI（认证中心面板若挂载则进入 pending 态）
  window.dispatchEvent(
    new CustomEvent("ssy-login-pending", {
      detail: { sessionId: start.sessionId, appId: appId ?? null },
    }),
  );
  await settingsApi.openExternal(start.authorizationUrl);
  return start;
}

/** OAuth 完成：自动绑定最近一次登录的目标 app（无目标则三端全绑）。 */
async function handleOAuthComplete(): Promise<void> {
  try {
    const accounts = await listShengsuanyunAccounts();
    const latest = accounts[accounts.length - 1];
    if (!latest) return;
    const app = lastBindApp;
    const results = app
      ? [await bindShengsuanyunAccount(app, latest.id)]
      : await bindShengsuanyunAllApps(latest.id);
    for (const r of results) {
      void analyticsApi.track("bind_completed", {
        app: app ?? null,
        status: r.status,
      });
    }
    if (results.some((r) => r.status === "conflict")) {
      window.dispatchEvent(new CustomEvent("ssy-bind-conflict"));
    }
    // 健康检查：绑定成功的 provider 立即做一次连通性探测（立项 P0"一键测试"）
    const apps = app ? [app] : ["claude", "codex", "gemini"];
    results.forEach((r, i) => {
      if (r.status === "ok" && r.providerId) {
        const appType = apps[i] ?? apps[0];
        void streamCheckProvider(appType as AppId, r.providerId)
          .then((res) => {
            window.dispatchEvent(
              new CustomEvent("ssy-health-checked", {
                detail: {
                  appType,
                  providerId: r.providerId,
                  ok: res.success && res.status === "operational",
                  message: res.message,
                },
              }),
            );
          })
          .catch(() => {});
      }
    });
  } catch (e) {
    window.dispatchEvent(
      new CustomEvent("ssy-bind-error", { detail: String(e) }),
    );
  } finally {
    lastBindApp = null;
  }
}

/** 在 App 挂载时调用一次：安装全局 complete 监听（幂等）。 */
export async function installGlobalLoginBridge(): Promise<void> {
  if (bridgeInstalled) return;
  bridgeInstalled = true;
  const { listen } = await import("@tauri-apps/api/event");
  await listen("shengsuanyun-oauth-complete", () => {
    void handleOAuthComplete();
  });
}
