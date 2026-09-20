// 应用内充值对话框（P2 本地支付，spec: docs/ssy-local-recharge-spec.md）。
//
// 由 App 根部挂载（单实例），入口通过 window 事件 'ssy-open-recharge' 打开。
// 两个视图：金额/渠道选择 → 二维码（可返回重选）。
// 支付渠道：支付宝（qr.alipay.com）/ 微信（weixin://wxpay/…），均已实测出码。
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  CheckCircle2,
  ExternalLink,
  Loader2,
  Wallet,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { shengsuanyunApi } from "@/lib/api/shengsuanyun";
import { settingsApi } from "@/lib/api/settings";

const POLL_INTERVAL_MS = 3000;
const POLL_MAX_TRIES = 100;
const CUSTOM_MIN = 30;
const CUSTOM_MAX = 5000;

type PayWay = "alipay" | "wechatpay";

const TIER_AMOUNTS = [10, 30, 100, 200, 500];
/** 档位金额（服务端套餐，含 ¥10）始终合法；非档位走自定义区间 30–5000 */
const isValidAmount = (yuan: number): boolean =>
  TIER_AMOUNTS.includes(yuan) ||
  (yuan >= CUSTOM_MIN && yuan <= CUSTOM_MAX);
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
  const [payWay, setPayWay] = useState<PayWay>("alipay");
  const [phase, setPhase] = useState<Phase>("form");
  const [qrSrc, setQrSrc] = useState("");
  const [orderAmount, setOrderAmount] = useState(0);
  const [orderWay, setOrderWay] = useState<PayWay>("alipay");
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

  const backToForm = useCallback(() => {
    stopPoll();
    setPhase("form");
    setQrSrc("");
    setError(null);
  }, [stopPoll]);

  const createAndPoll = useCallback(async () => {
    stopPoll();
    setBusy(true);
    setPhase("pending");
    setError(null);
    setOrderAmount(amount);
    setOrderWay(payWay);
    try {
      const o = await shengsuanyunApi.createRechargeOrder(amount, payWay);
      const { toDataURL } = await import("qrcode");
      setQrSrc(
        await toDataURL(o.url, {
          width: 200,
          margin: 2,
          color: { dark: "#000000ff", light: "#ffffffff" },
          errorCorrectionLevel: "M",
        }),
      );
      const loop = async (tries: number) => {
        const s = await shengsuanyunApi.payStatus(o.order_id);
        if (s.status === "unpaid") {
          if (tries >= POLL_MAX_TRIES) {
            setPhase("error");
            setError(
              t("shengsuanyun.qrExpired", {
                defaultValue: "二维码已过期，请返回重新生成",
              }),
            );
            return;
          }
          pollRef.current = setTimeout(() => loop(tries + 1), POLL_INTERVAL_MS);
          return;
        }
        setPhase("paid");
        const accounts = await shengsuanyunApi.listAccounts();
        if (accounts[0]) {
          await shengsuanyunApi
            .refreshBalance(accounts[0].id)
            .catch(() => {});
        }
      };
      await loop(0);
    } catch (e) {
      setPhase("error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [amount, payWay, t, stopPoll]);

  if (!open) return null;

  const wayLabel: Record<PayWay, string> = {
    alipay: t("shengsuanyun.alipay", { defaultValue: "支付宝" }),
    wechatpay: t("shengsuanyun.wechatPay", { defaultValue: "微信支付" }),
  };

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/50"
      role="dialog"
      aria-modal="true"
    >
      <div className="w-[min(440px,calc(100vw-32px))] rounded-xl border border-border bg-card p-6 shadow-xl">
        {/* 标题栏 */}
        <div className="mb-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            {phase === "pending" && (
              <button
                aria-label={t("shengsuanyun.back", { defaultValue: "返回重选" })}
                className="rounded p-1 hover:bg-muted"
                onClick={backToForm}
              >
                <ArrowLeft className="h-4 w-4" />
              </button>
            )}
            <h2 className="flex items-center gap-2 text-base font-semibold">
              <Wallet className="h-4 w-4" />
              {phase === "paid"
                ? t("shengsuanyun.rechargeDone", { defaultValue: "充值成功" })
                : phase === "pending"
                  ? t("shengsuanyun.scanPayTitle", { defaultValue: "扫码支付" })
                  : t("shengsuanyun.rechargeTitle", { defaultValue: "胜算云充值" })}
            </h2>
          </div>
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

        {/* 表单视图：档位 + 自定义金额 + 支付渠道 */}
        {phase === "form" && (
          <>
            <div className="mb-3 grid grid-cols-5 gap-2">
              {TIER_AMOUNTS.map((v) => (
                <button
                  key={v}
                  className={`rounded-lg border px-2 py-3 text-center text-sm transition-colors ${
                    amount === v
                      ? "border-emerald-500 bg-emerald-500/10 font-semibold text-emerald-600 dark:text-emerald-400"
                      : "border-border hover:bg-muted"
                  }`}
                  onClick={() => setAmount(v)}
                >
                  <span className="text-xs text-muted-foreground">¥</span>
                  <br />
                  {v}
                </button>
              ))}
            </div>

            <label className="mb-4 block text-xs text-muted-foreground">
              {t("shengsuanyun.customAmount", {
                defaultValue: "自定义金额（元，30–5000）",
              })}
              <input
                type="number"
                min={CUSTOM_MIN}
                max={CUSTOM_MAX}
                value={amount}
                onChange={(e) => setAmount(parseInt(e.target.value, 10) || 0)}
                className="mt-1 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
              />
            </label>

            <div className="mb-4">
              <p className="mb-2 text-xs text-muted-foreground">
                {t("shengsuanyun.payMethod", { defaultValue: "支付方式" })}
              </p>
              <div className="flex gap-2">
                {(["alipay", "wechatpay"] as const).map((w) => (
                  <button
                    key={w}
                    className={`flex-1 rounded-lg border px-3 py-3 text-sm transition-colors ${
                      payWay === w
                        ? "border-emerald-500 bg-emerald-500/10 font-semibold"
                        : "border-border hover:bg-muted"
                    }`}
                    onClick={() => setPayWay(w)}
                  >
                    <span className="flex items-center justify-center gap-2">
                      {payWay === w && (
                        <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                      )}
                      {wayLabel[w]}
                    </span>
                  </button>
                ))}
              </div>
            </div>

            <Button
              className="w-full bg-emerald-600 hover:bg-emerald-500"
              disabled={busy || !isValidAmount(amount)}
              onClick={() => void createAndPoll()}
            >
              {busy ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Wallet className="mr-2 h-4 w-4" />
              )}
              {t("shengsuanyun.rechargeNow", {
                defaultValue: "确认充值 ¥{{amount}}",
                amount,
              })}
            </Button>
          </>
        )}

        {/* 二维码视图 */}
        {phase === "pending" && (
          <div className="grid place-items-center gap-3">
            <p className="text-2xl font-bold tabular-nums">
              ¥{orderAmount.toFixed(2)}
            </p>
            <p className="text-xs text-muted-foreground">{wayLabel[orderWay]}</p>
            <div
              className="rounded-lg border-2 border-border p-3"
              style={{ backgroundColor: "#ffffff" }}
            >
              {qrSrc ? (
                <img
                  alt={t("shengsuanyun.qrAlt", { defaultValue: "收款码" })}
                  src={qrSrc}
                  width={200}
                  height={200}
                />
              ) : (
                <Loader2 className="h-8 w-8 animate-spin" />
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              {t("shengsuanyun.scanToPay", {
                defaultValue: "请使用对应 App 扫码完成支付",
              })}
            </p>
            <p className="text-xs text-muted-foreground/60">
              {t("shengsuanyun.orderPending", {
                defaultValue: "正在等待支付结果…",
              })}
            </p>
          </div>
        )}

        {/* 成功视图 */}
        {phase === "paid" && (
          <div className="grid place-items-center gap-3 py-4">
            <CheckCircle2 className="h-12 w-12 text-emerald-500" />
            <p className="text-lg font-semibold">
              ¥{orderAmount.toFixed(2)}{" "}
              {t("shengsuanyun.rechargePaid", {
                defaultValue: "充值成功",
              })}
            </p>
            <p className="text-xs text-muted-foreground">
              {t("shengsuanyun.balanceRefreshed", {
                defaultValue: "余额已自动刷新",
              })}
            </p>
            <Button
              variant="outline"
              onClick={() => {
                setPhase("form");
                setQrSrc("");
              }}
            >
              {t("shengsuanyun.rechargeAgain", { defaultValue: "再充一笔" })}
            </Button>
          </div>
        )}

        {/* 错误视图 */}
        {phase === "error" && error && (
          <div className="space-y-3" role="alert">
            <p className="text-center text-sm text-destructive">{error}</p>
            <div className="flex justify-center gap-2">
              <Button variant="outline" size="sm" onClick={backToForm}>
                <ArrowLeft className="mr-1 h-3.5 w-3.5" />
                {t("shengsuanyun.backToSelect", { defaultValue: "返回重选" })}
              </Button>
            </div>
          </div>
        )}

        {/* 底部兜底 */}
        <div className="mt-4 border-t border-border/40 pt-3 text-center text-xs text-muted-foreground">
          {t("shengsuanyun.largeAmountHint", {
            defaultValue: "大额充值 / 对公转账",
          })}
          ：{" "}
          <button
            className="text-primary underline"
            onClick={() =>
              void settingsApi.openExternal(
                "https://console.shengsuanyun.com/user/recharge",
              )
            }
          >
            <ExternalLink className="mr-0.5 inline h-3 w-3" />
            {t("shengsuanyun.openConsole", { defaultValue: "打开控制台" })}
          </button>
        </div>
      </div>
    </div>
  );
}
