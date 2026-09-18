// 胜算云代金券/体验券明细面板（对齐官方控制台"卡券"页的数据）。
// 金额字段单位 1e-4 元；scope=all 为通用券，loomloom 等为产品专属券
// （专属券不计入账号的"体验券"汇总，与官方口径一致，不要合并显示）。
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  shengsuanyunApi,
  type VoucherRecord,
} from "@/lib/api/shengsuanyun";

/** 可用券排最前（剩余>0 且状态 success），其余按过期时间倒序 */
function sortRecords(records: VoucherRecord[]): VoucherRecord[] {
  const usable = (r: VoucherRecord) =>
    r.status === "success" && r.remaining_amount > 0;
  return [...records].sort((a, b) => {
    if (usable(a) !== usable(b)) return usable(a) ? -1 : 1;
    return (b.expire_time || "").localeCompare(a.expire_time || "");
  });
}

export function ShengsuanyunVoucherList({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [records, setRecords] = useState<VoucherRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fmtYuan = (v: number) => `¥${(v / 10_000).toFixed(2)}`;
  const fmtTime = (iso: string) => iso.replace("T", " ").slice(0, 16);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await shengsuanyunApi.getVoucherList();
      setRecords(sortRecords(res.voucher_records ?? []));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const statusBadge = (r: VoucherRecord) => {
    if (r.status === "success" && r.remaining_amount > 0) {
      return { text: t("shengsuanyun.voucherActive", { defaultValue: "已激活，正在使用中" }), cls: "border-emerald-500/40 text-emerald-600 dark:text-emerald-400" };
    }
    if (r.status === "used") {
      return { text: t("shengsuanyun.voucherUsedUp", { defaultValue: "已用完" }), cls: "border-border text-muted-foreground" };
    }
    if (r.status.includes("expire")) {
      return { text: t("shengsuanyun.voucherExpired", { defaultValue: "已过期" }), cls: "border-border text-muted-foreground" };
    }
    return { text: r.status, cls: "border-border text-muted-foreground" };
  };

  const scopeLabel = (scope: string) => {
    if (!scope || scope === "all") {
      return t("shengsuanyun.voucherScopeAll", { defaultValue: "通用" });
    }
    if (scope === "loomloom") {
      return t("shengsuanyun.voucherScopeLoom", { defaultValue: "仅限 LoomLoom" });
    }
    return scope;
  };

  return (
    <div className="mt-3 rounded-lg border border-border/60 p-4 text-sm">
      <div className="mb-3 flex items-center justify-between">
        <span className="font-medium">
          {t("shengsuanyun.voucherList", { defaultValue: "代金券" })}
        </span>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-1"
            aria-label={t("shengsuanyun.refreshBalance", { defaultValue: "刷新" })}
            disabled={loading}
            onClick={() => void load()}
          >
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              "↻"
            )}
          </Button>
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
      </div>

      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      {!loading && !error && records.length === 0 && (
        <p className="text-xs text-muted-foreground">
          {t("shengsuanyun.noVouchers", { defaultValue: "暂无代金券" })}
        </p>
      )}

      <div className="space-y-2">
        {records.map((r) => {
          const badge = statusBadge(r);
          return (
            <div
              key={r.id}
              className="flex items-stretch gap-3 rounded-lg border border-border/40 overflow-hidden"
            >
              <div className="flex w-24 shrink-0 flex-col items-center justify-center bg-orange-500/10 px-2 py-2 text-center">
                <span className="text-[10px] text-muted-foreground">
                  {t("shengsuanyun.voucherRemaining", { defaultValue: "剩余" })}
                </span>
                <span
                  className={`text-base font-semibold tabular-nums ${
                    r.remaining_amount > 0 ? "text-orange-600 dark:text-orange-400" : "text-muted-foreground"
                  }`}
                >
                  {fmtYuan(r.remaining_amount)}
                </span>
                <span className="text-[10px] tabular-nums text-muted-foreground">
                  {t("shengsuanyun.voucherUsed", { defaultValue: "已用" })}{" "}
                  {fmtYuan(r.used_amount)}
                </span>
              </div>
              <div className="min-w-0 flex-1 py-2 pr-3">
                <div className="flex items-start justify-between gap-2">
                  <span className="truncate font-medium">{r.title || r.des}</span>
                  <span
                    className={`shrink-0 rounded-full border px-2 py-0.5 text-[10px] ${badge.cls}`}
                  >
                    {badge.text}
                  </span>
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
                  <span>{scopeLabel(r.scope)}</span>
                  {r.expire_time && (
                    <span>
                      {t("shengsuanyun.voucherExpireAt", { defaultValue: "有效期至" })}:{" "}
                      {fmtTime(r.expire_time)}
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
