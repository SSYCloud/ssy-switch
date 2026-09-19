// 胜算云调用统计面板（立项 P1"调用统计"）。
// 数据源：GET /modelrouter/userusage（jwt 认证，按日/按模型聚合）。
// total_amount 单位为 1e-7 元，展示层统一换算为元。
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BarChart3, Loader2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  shengsuanyunApi,
  type UsageSummary,
  type ModalityUsageResponse,
} from "@/lib/api/shengsuanyun";


function fmtDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function daysAgo(n: number): string {
  return fmtDate(new Date(Date.now() - n * 86_400_000));
}

export function ShengsuanyunUsageStats({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [range, setRange] = useState<13 | 29>(13);
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [modalities, setModalities] = useState<ModalityUsageResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (r: 13 | 29) => {
    setLoading(true);
    setError(null);
    try {
      const [sum, mods] = await Promise.all([
        shengsuanyunApi.getUsageSummary(daysAgo(r), daysAgo(0)),
        shengsuanyunApi
          .getModalityUsage(daysAgo(r), daysAgo(0))
          .catch(() => null),
      ]);
      setSummary(sum);
      setModalities(mods);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(range);
  }, [range, load]);


  return (
    <div className="mt-3 rounded-lg border border-border/60 p-4 text-sm">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 font-medium">
          <BarChart3 className="h-4 w-4" />
          {t("shengsuanyun.usageStats", { defaultValue: "调用统计" })}
        </div>
        <div className="flex items-center gap-1">
          {([13, 29] as const).map((r) => (
            <Button
              key={r}
              variant={range === r ? "secondary" : "ghost"}
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={() => setRange(r)}
            >
              {t("shengsuanyun.lastDays", { defaultValue: "近 {{n}} 天", n: r + 1 })}
            </Button>
          ))}
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-1"
            aria-label={t("shengsuanyun.close", { defaultValue: "关闭" })}
            onClick={onClose}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {loading && (
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("shengsuanyun.loadingStats", { defaultValue: "加载中…" })}
        </div>
      )}

      {!loading && error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}

      {!loading && !error && (
        <>
          <div className="mb-3 text-xs text-muted-foreground">
            {t("shengsuanyun.statsTotal", { defaultValue: "区间总消费" })}:{" "}
            <span className="font-semibold text-foreground">
              ¥{(summary?.total_yuan ?? 0).toFixed(2)}
            </span>
            {" · "}
            {t("shengsuanyun.statsTokens", { defaultValue: "总 tokens" })}:{" "}
            {(summary?.total_tokens ?? 0).toLocaleString()}
          </div>

          {/* 按日消费条形图 */}
          <div className="mb-4 space-y-1.5">
            {(!summary || summary.daily.length === 0) && (
              <p className="text-xs text-muted-foreground">
                {t("shengsuanyun.noUsage", { defaultValue: "区间内无调用记录" })}
              </p>
            )}
            {(summary?.daily ?? []).map((d) => {
              const maxDaily = Math.max(
                ...(summary?.daily ?? []).map((x) => x.amount_yuan),
                0.0001,
              );
              return (
              <div key={d.date} className="flex items-center gap-2 text-xs">
                <span className="w-20 shrink-0 tabular-nums text-muted-foreground">
                  {d.date.slice(5)}
                </span>
                <div className="h-3 flex-1 overflow-hidden rounded bg-muted">
                  <div
                    className="h-full rounded bg-emerald-500/70"
                    style={{
                      width: `${Math.max(2, (d.amount_yuan / maxDaily) * 100)}%`,
                    }}
                  />
                </div>
                <span className="w-16 shrink-0 text-right tabular-nums">
                  ¥{d.amount_yuan.toFixed(2)}
                </span>
              </div>
              );
            })}
          </div>

          {/* 按模型明细 */}
          {(summary?.models.length ?? 0) > 0 && (
            <table className="w-full text-xs">
              <thead>
                <tr className="text-left text-muted-foreground">
                  <th className="pb-1 font-medium">
                    {t("shengsuanyun.model", { defaultValue: "模型" })}
                  </th>
                  <th className="pb-1 text-right font-medium">
                    {t("shengsuanyun.tokens", { defaultValue: "Tokens" })}
                  </th>
                  <th className="pb-1 text-right font-medium">
                    {t("shengsuanyun.amount", { defaultValue: "消费" })}
                  </th>
                </tr>
              </thead>
              <tbody>
                {(summary?.models ?? []).map((m) => (
                  <tr key={m.model} className="border-t border-border/40">
                    <td className="py-1 pr-2">{m.model}</td>
                    <td className="py-1 text-right tabular-nums">
                      {m.tokens.toLocaleString()}
                    </td>
                    <td className="py-1 text-right tabular-nums">
                      ¥{m.amount_yuan.toFixed(2)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}

          {/* 多模态（图片/视频等）消费 */}
          {(modalities?.usages?.length ?? 0) > 0 && (
            <div className="mt-4 border-t border-border/40 pt-3">
              <div className="mb-2 text-xs font-medium text-muted-foreground">
                {t("shengsuanyun.modalityUsage", {
                  defaultValue: "多模态调用（图片/视频）",
                })}
              </div>
              <table className="w-full text-xs">
                <tbody>
                  {modalities!.usages.map((u) =>
                    u.details.map((d) => (
                      <tr key={`${u.date}-${d.model}`}>
                        <td className="py-1 pr-2 tabular-nums text-muted-foreground">
                          {u.date.slice(0, 10)}
                        </td>
                        <td className="py-1 pr-2">{d.model}</td>
                        <td className="py-1 text-right tabular-nums">
                          ¥{(d.amount_yuan).toFixed(4)}
                        </td>
                      </tr>
                    )),
                  )}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}
