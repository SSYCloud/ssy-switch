// 胜算云充值/账单流水（立项 P1：充值记录可见，D7 首充/复充在客户端可查）。
// Asset/Balance 单位 1e-4 元。
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { shengsuanyunApi, type BillEntry } from "@/lib/api/shengsuanyun";

const PAGE_SIZE = 10;

export function ShengsuanyunBillList({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [bills, setBills] = useState<BillEntry[]>([]);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fmtYuan = (v: number) => `¥${(v / 10_000).toFixed(2)}`;
  const fmtTime = (iso: string) => iso.replace("T", " ").slice(0, 16);
  const subTypeText = (sub: string) => {
    if (sub.includes("wechat")) return t("shengsuanyun.payWechat", { defaultValue: "微信支付" });
    if (sub.includes("alipay")) return t("shengsuanyun.payAlipay", { defaultValue: "支付宝" });
    return sub;
  };

  const loadPage = useCallback(async (p: number, append: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const res = await shengsuanyunApi.getBillList(p, PAGE_SIZE);
      const list = res.bills ?? [];
      setBills((prev) => (append ? [...prev, ...list] : list));
      setHasMore(list.length === PAGE_SIZE);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadPage(1, false);
  }, [loadPage]);

  return (
    <div className="mt-3 rounded-lg border border-border/60 p-4 text-sm">
      <div className="mb-3 flex items-center justify-between">
        <span className="font-medium">
          {t("shengsuanyun.billList", { defaultValue: "充值记录" })}
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-1"
          aria-label={t("shengsuanyun.close", { defaultValue: "关闭" })}
          onClick={onClose}
        >
          ✕
        </Button>
      </div>

      {loading && bills.length === 0 && (
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("shengsuanyun.loadingStats", { defaultValue: "加载中…" })}
        </div>
      )}
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      {!loading && !error && bills.length === 0 && (
        <p className="text-xs text-muted-foreground">
          {t("shengsuanyun.noBills", { defaultValue: "暂无账单记录" })}
        </p>
      )}

      {bills.length > 0 && (
        <table className="w-full text-xs">
          <thead>
            <tr className="text-left text-muted-foreground">
              <th className="pb-1 font-medium">
                {t("shengsuanyun.billTime", { defaultValue: "时间" })}
              </th>
              <th className="pb-1 font-medium">
                {t("shengsuanyun.billType", { defaultValue: "类型" })}
              </th>
              <th className="pb-1 text-right font-medium">
                {t("shengsuanyun.billAmount", { defaultValue: "金额" })}
              </th>
              <th className="pb-1 text-right font-medium">
                {t("shengsuanyun.billBalance", { defaultValue: "余额快照" })}
              </th>
            </tr>
          </thead>
          <tbody>
            {bills.map((b) => (
              <tr key={b.ID} className="border-t border-border/40">
                <td className="py-1.5 pr-2 tabular-nums">{fmtTime(b.CreatedAt)}</td>
                <td className="py-1.5 pr-2">
                  {t("shengsuanyun.billRecharge", { defaultValue: "充值" })} ·{" "}
                  {subTypeText(b.BillSubType)}
                </td>
                <td className="py-1.5 text-right font-medium tabular-nums">
                  +{fmtYuan(b.Asset)}
                </td>
                <td className="py-1.5 text-right tabular-nums text-muted-foreground">
                  {fmtYuan(b.Balance)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {hasMore && (
        <div className="mt-2 text-center">
          <Button
            variant="outline"
            size="sm"
            disabled={loading}
            onClick={() => {
              const next = page + 1;
              setPage(next);
              void loadPage(next, true);
            }}
          >
            {loading ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : null}
            {t("shengsuanyun.loadMore", { defaultValue: "加载更多" })}
          </Button>
        </div>
      )}
    </div>
  );
}
