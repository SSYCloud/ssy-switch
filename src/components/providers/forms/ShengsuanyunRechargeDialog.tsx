// 应用内充值对话框（P2 本地支付，spec: docs/ssy-local-recharge-spec.md）。
//
// 由 App 根部挂载（单实例），任何入口通过 window 事件 'ssy-open-recharge' 打开。
// 流程：档位/自定义金额 → 创建订单 → 应用内二维码 → 每 3s 轮询 payQuery
//   → 支付成功自动刷新余额；超时可刷新二维码重下单。
// 大额（>¥5000）/对公转账走底部控制台外跳兜底。
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Wallet, X, Loader2, ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import { shengsuanyunApi } from "@/lib/api/shengsuanyun";
import { settingsApi } from "@/lib/api/settings";

const POLL_INTERVAL_MS = 3000;
const POLL_MAX_TRIES = 100;
const CUSTOM_MIN = 30;
const CUSTOM_MAX = 5000;

type Phase = "form" | "pending" | "paid" | "error";

export function ShengsuanyunRechargeDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [amount, setAmount] = useState<number>(30);
  const [phase, setPhase] = useState<Phase>("form");
  const [qrSrc, setQrSrc] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pollRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const stopPoll = useCallback(() => {
    if (pollRef.current) {
      clearTimeout(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!open) stopPoll();
    return stopPoll;
  }, [open, stopPoll]);

  const createAndPoll = useCallback(async () => {
    stopPoll();
    setPhase("pending");
    setError(null);
    try {
      const o = await shengsuanyunApi.createRechargeOrder(amount);
      const { toDataURL } = await import("qrcode");
      setQrSrc(await toDataURL(o.url, { width: 180, margin: 1 }));
      const loop = async (tries: number) => {
        const s = await shengsuanyunApi.payStatus(o.order_id);
        if (s.status === "unpaid") {
          if (tries >= POLL_MAX_TRIES) {
            setPhase("error");
            setError(
              t("shengsuanyun.qrExpired", { defaultValue: "二维码已过期，请刷新重试" }),
            );
            return;
          }
          pollRef.current = setTimeout(() => loop(tries + 1), POLL_INTERVAL_MS);
          return;
        }
        setPhase("paid");
        await shengsuanyunApi.refreshBalance((await shengsuanyunApi.listAccounts())[0]?.id ?? "").catch(() => {});
      };
      await loop(0);
    } catch (e) {
      setPhase("error");
      setError(String(e));
    }
  }, [amount, t]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50" role="dialog" aria-modal="true">
      <div className="w-[min(420px,calc(100vw-32px))] rounded-xl border border-border bg-card p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-base font-semibold">
            <Wallet className="h-4 w-4" />
            {t("shengsuanyun.rechargeTitle", { defaultValue: "胜算云充值" })}
          </h2>
          <button
            aria-label={t("shengsuanyun.close", { defaultValue: "关闭" })}
            className="rounded p-1 hover:bg-muted"
            onClick={() => {
              stopPoll();
              onClose();
            }}
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {(phase === "form" || phase === "error") && (
          <>
            <div className="mb-3 flex flex-wrap gap-2">
              {[10, 30, 100, 200, 500].map((v) => (
                <button
                  key={v}
                  className={`rounded-lg border px-4 py-2 text-sm ${
                    amount === v ? "border-emerald-500 bg-emerald-500/10 font-semibold" : "border-border hover:bg-muted"
                  }`}
                  onClick={() => setAmount(v)}
                >
                  ¥{v}
                </button>
              ))}
            </div>
            <label className="block text-xs text-muted-foreground">
              {t("shengsuanyun.customAmount", { defaultValue: "自定义金额（元，30–5000）" })}
              <input
                type="number"
                min={CUSTOM_MIN}
                max={CUSTOM_MAX}
                value={amount}
                onChange={(e) => setAmount(parseInt(e.target.value, 10) || 0)}
                className="mt-1 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
              />
            </label>
            <Button
              className="mt-4 w-full bg-emerald-600 hover:bg-emerald-500"
              disabled={busy || !(amount >= CUSTOM_MIN && amount <= CUSTOM_MAX)}
              onClick={async () => {
                setBusy(true);
                await createAndPoll();
                setBusy(false);
              }}
            >
              <Wallet className="mr-2 h-4 w-4" />
              {t("shengsuanyun.rechargeNow", { defaultValue: "生成收款二维码" })}
            </Button>
          </>
        )}

        {phase === "pending" && (
          <div className="mt-4 grid place-items-center gap-2">
            <div className="rounded-lg border border-border bg-white p-2">
              {qrSrc ? (
                <img alt="QR" src={qrSrc} width={180} height={180} />
              ) : (
                <Loader2 className="h-6 w-6 animate-spin" />
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              {t("shengsuanyun.scanToPay", { defaultValue: "支付宝扫一扫完成支付" })}
            </p>
          </div>
        )}

        {phase === "paid" && (
          <div className="mt-4 rounded-lg border border-emerald-500/30 bg-emerald-500/10 p-4 text-center text-sm text-emerald-700 dark:text-emerald-300">
            {t("shengsuanyun.rechargePaid", { defaultValue: "充值成功，余额已刷新" })}
          </div>
        )}

        {phase === "error" && error && (
          <div className="mt-3 space-y-2" role="alert">
            <p className="text-sm text-destructive">{error}</p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setPhase("form");
                setError(null);
              }}
            >
              {t("shengsuanyun.retry", { defaultValue: "重试" })}
            </Button>
          </div>
        )}

        <div className="mt-4 border-t border-border/40 pt-3 text-center text-xs text-muted-foreground">
          {t("shengsuanyun.largeAmountHint", { defaultValue: "大额充值 / 对公转账" })}:{" "}
          <button
            className="text-primary underline"
            onClick={() => void settingsApi.openExternal("https://console.shengsuanyun.com/user/recharge")}
          >
            <ExternalLink className="mr-1 inline h-3 w-3" />
            {t("shengsuanyun.openConsole", { defaultValue: "打开控制台" })}
          </button>
        </div>
      </div>
    </div>
  );
}
