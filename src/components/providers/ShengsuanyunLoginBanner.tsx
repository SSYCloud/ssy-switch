// 首页顶部的胜算云登录横幅：未登录时高亮展示，引导一键 OAuth。
// 已登录后自动隐藏（账号管理仍在 设置 → 认证中心）。
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { KeyRound, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ProviderIcon } from "@/components/ProviderIcon";
import { startLoginFlow } from "@/lib/shengsuanyunFlow";
import { shengsuanyunApi } from "@/lib/api/shengsuanyun";
import { analyticsApi } from "@/lib/api/analytics";

export function ShengsuanyunLoginBanner({
  appId,
}: {
  /** 当前选中的 app（claude/codex/gemini），用于登录后绑定目标 */
  appId: string;
}) {
  const { t } = useTranslation();
  const [visible, setVisible] = useState<boolean | null>(null);

  useEffect(() => {
    let disposed = false;
    const check = async () => {
      try {
        const accounts = await shengsuanyunApi.listAccounts();
        if (!disposed) setVisible(accounts.length === 0);
      } catch {
        // Tauri IPC 不可用（如纯浏览器调试）时不显示横幅
        if (!disposed) setVisible(false);
      }
    };
    void check();
    const off = setInterval(check, 30_000); // 登出后横幅自动回来
    return () => {
      disposed = true;
      clearInterval(off);
    };
  }, []);

  if (visible !== true) return null;

  const appNameMap: Record<string, string> = {
    claude: "Claude Code",
    "claude-desktop": "Claude Desktop",
    codex: "Codex",
    gemini: "Gemini CLI",
    grokbuild: "Grok Build",
    opencode: "OpenCode",
    openclaw: "OpenClaw",
    hermes: "Hermes",
    pi: "Pi",
  };
  const appName = appNameMap[appId];

  return (
    <div className="relative overflow-hidden rounded-2xl border border-emerald-500/30 bg-gradient-to-r from-emerald-500/15 via-emerald-500/5 to-transparent p-5">
      <div className="flex flex-wrap items-center gap-4">
        <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-emerald-500/15">
          <ProviderIcon name="shengsuanyun" size={26} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 font-semibold">
            <Sparkles className="h-4 w-4 text-emerald-500" />
            {t("shengsuanyun.bannerTitle", {
              defaultValue: "连接胜算云，一键登录即用",
            })}
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("shengsuanyun.bannerBody", {
              defaultValue:
                "无需复制 API Key{0}。浏览器登录胜算云账号后自动绑定并激活。",
            }).replace(
              "{0}",
              appName ? `，直接在 ${appName} 中使用` : "",
            )}
          </p>
        </div>
        <Button
          size="lg"
          className="bg-emerald-600 hover:bg-emerald-500"
          onClick={() => {
            // 直接发起 OAuth（打开浏览器授权页），登录完成后由全局桥自动绑定；
            // 同时跳转认证中心展示进度
            void analyticsApi.track("recharge_clicked", { entry: "banner" }).catch(() => {});
            startLoginFlow(appId, "banner").catch((e) =>
              console.error("start shengsuanyun login failed", e),
            );
            window.dispatchEvent(
              new CustomEvent("ssy-open-auth", { detail: { appId } }),
            );
          }}
        >
          <KeyRound className="mr-2 h-4 w-4" />
          {t("shengsuanyun.bannerCta", { defaultValue: "用胜算云账号登录" })}
        </Button>
      </div>
    </div>
  );
}
