// 充值入口对话框（网页充值，2026-09-23 起）。
//
// JWT 权限体系（过期 / 续期）解决之前，客户端不再直接创建支付订单，
// 统一跳转胜算云控制台完成充值：
// - 主按钮打开系统浏览器（settingsApi.openExternal，不经 webview）
// - 打开时 markRechargeOpened()，支付完成回到应用后由 focus 触发余额自动刷新
// - 原应用内下单/二维码/轮询流程保留在后端命令与 SDK 中（未接线），
//   待 jwt 续期路径就绪后可恢复
import { useTranslation } from "react-i18next";
import { ExternalLink, RefreshCw, Wallet, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { settingsApi } from "@/lib/api/settings";
import { analyticsApi } from "@/lib/api/analytics";
import { markRechargeOpened } from "@/lib/rechargeTracker";

const RECHARGE_URL = "https://console.shengsuanyun.com/user/recharge";

export function ShengsuanyunRechargeDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();

  if (!open) return null;

  const openWebRecharge = () => {
    markRechargeOpened();
    void analyticsApi
      .track("recharge_web_opened", { url: RECHARGE_URL })
      .catch(() => {});
    void settingsApi.openExternal(RECHARGE_URL);
  };

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/50"
      role="dialog"
      aria-modal="true"
    >
      <div className="w-[min(400px,calc(100vw-32px))] rounded-xl border border-border bg-card p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-base font-semibold">
            <Wallet className="h-4 w-4" />
            {t("shengsuanyun.rechargeTitle", { defaultValue: "胜算云充值" })}
          </h2>
          <button
            aria-label={t("shengsuanyun.close", { defaultValue: "关闭" })}
            className="rounded p-1 hover:bg-muted"
            onClick={onClose}
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <p className="mb-1 text-sm">
          {t("shengsuanyun.rechargeWebHint", {
            defaultValue: "充值将在浏览器中的胜算云控制台完成。",
          })}
        </p>
        <p className="mb-5 flex items-center gap-1.5 text-xs text-muted-foreground">
          <RefreshCw className="h-3.5 w-3.5" />
          {t("shengsuanyun.webRechargeReturnHint", {
            defaultValue: "支付完成后返回本应用，余额将自动刷新。",
          })}
        </p>

        <Button
          className="w-full bg-emerald-600 hover:bg-emerald-500"
          onClick={openWebRecharge}
        >
          <ExternalLink className="mr-2 h-4 w-4" />
          {t("shengsuanyun.openWebRecharge", {
            defaultValue: "打开网页充值",
          })}
        </Button>

        <p className="mt-3 text-center text-xs text-muted-foreground">
          {RECHARGE_URL}
        </p>
      </div>
    </div>
  );
}
