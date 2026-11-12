import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  shengsuanyunApi,
  type ShengsuanyunAccount,
  type ShengsuanyunEnterpriseBudget as Budget,
} from "@/lib/api/shengsuanyun";

type BudgetState = {
  account: ShengsuanyunAccount;
} & (
  | { status: "loading" | "error" }
  | { status: "ready"; budget: Budget; fetchedAt: Date }
);

export function ShengsuanyunEnterpriseBudget({
  account,
}: {
  account: ShengsuanyunAccount;
}) {
  const { t, i18n } = useTranslation();
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<BudgetState | null>(null);

  useEffect(() => {
    let active = true;
    setState({ account, status: "loading" });
    void shengsuanyunApi.getEnterpriseBudget(account.id).then(
      (budget) => {
        if (active)
          setState({ account, status: "ready", budget, fetchedAt: new Date() });
      },
      () => {
        if (active) setState({ account, status: "error" });
      },
    );
    return () => {
      active = false;
    };
  }, [account, attempt]);

  // 切换账号或同账号重新登录时，不暂显上一个身份/项目的预算。
  const current = state?.account === account ? state : null;
  const loading = !current || current.status === "loading";
  const budget = current?.status === "ready" ? current.budget : null;
  const currency = (value: number | null) =>
    value == null
      ? "—"
      : `¥${value.toLocaleString(i18n.language, { minimumFractionDigits: 2, maximumFractionDigits: 6 })}`;
  const dateTime = (value: string) => {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? null
      : date.toLocaleString(i18n.language, {
          timeZone: "Asia/Shanghai",
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
        });
  };
  const periodStart = budget ? dateTime(budget.period_start) : null;
  const periodEnd = budget ? dateTime(budget.period_end) : null;
  const periodKind =
    ["month", "monthly"].includes(budget?.period_kind ?? "")
      ? t("shengsuanyun.enterpriseBudget.month", { defaultValue: "月度预算" })
      : ["day", "daily"].includes(budget?.period_kind ?? "")
        ? t("shengsuanyun.enterpriseBudget.day", { defaultValue: "每日预算" })
        : t("shengsuanyun.enterpriseBudget.period", {
            defaultValue: "预算周期",
          });
  const unlimited = t("shengsuanyun.enterpriseBudget.unlimited", {
    defaultValue: "无限额",
  });
  const percentage = budget?.remaining_percentage;

  return (
    <section
      className="w-full space-y-3 rounded-lg bg-muted/40 p-3"
      aria-label={t("shengsuanyun.enterpriseBudget.title", {
        defaultValue: "企业项目预算",
      })}
      aria-busy={loading}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">
            {t("shengsuanyun.enterpriseBudget.title", {
              defaultValue: "企业项目预算",
            })}
            {budget && (
              <span className="ml-2 text-xs font-normal text-muted-foreground">
                {t("shengsuanyun.enterpriseBudget.project", {
                  defaultValue: "项目 #{{id}}",
                  id: budget.project_id,
                })}
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("shengsuanyun.enterpriseBudget.scope", {
              defaultValue: "本子账号在登录所选项目中的额度",
            })}
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={loading}
          onClick={() => setAttempt((n) => n + 1)}
        >
          <RefreshCw
            className={`mr-1 h-3.5 w-3.5${loading ? " animate-spin" : ""}`}
          />
          {t("shengsuanyun.enterpriseBudget.refresh", {
            defaultValue: "刷新预算",
          })}
        </Button>
      </div>
      {loading && (
        <p
          role="status"
          className="flex items-center gap-2 text-sm text-muted-foreground"
        >
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("shengsuanyun.enterpriseBudget.loading", {
            defaultValue: "正在读取预算…",
          })}
        </p>
      )}
      {current?.status === "error" && (
        <p role="alert" className="text-sm text-destructive">
          {t("shengsuanyun.enterpriseBudget.error", {
            defaultValue:
              "预算读取失败，请刷新重试；如果登录已失效，请重新登录。",
          })}
        </p>
      )}
      {budget && (
        <>
          <div className="text-xs text-muted-foreground">
            {periodKind} ·{" "}
            {periodStart && periodEnd
              ? `${periodStart} – ${periodEnd} (UTC+8)`
              : t("shengsuanyun.enterpriseBudget.periodUnknown", {
                  defaultValue: "周期信息暂不可用",
                })}
          </div>
          <dl className="grid grid-cols-3 gap-3 text-sm">
            <div>
              <dt className="text-xs text-muted-foreground">
                {t("shengsuanyun.enterpriseBudget.used", {
                  defaultValue: "本周期已用",
                })}
              </dt>
              <dd className="mt-1 font-medium tabular-nums">
                {currency(budget.period_consumed_cny)}
              </dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">
                {t("shengsuanyun.enterpriseBudget.limit", {
                  defaultValue: "预算上限",
                })}
              </dt>
              <dd className="mt-1 font-medium tabular-nums">
                {budget.unlimited
                  ? unlimited
                  : currency(budget.period_limit_cny)}
              </dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">
                {t("shengsuanyun.enterpriseBudget.remaining", {
                  defaultValue: "剩余额度",
                })}
              </dt>
              <dd className="mt-1 font-medium tabular-nums">
                {budget.unlimited ? unlimited : currency(budget.remaining_cny)}
              </dd>
            </div>
          </dl>
          {!budget.unlimited &&
            percentage != null &&
            Number.isFinite(percentage) && (
              <p className="text-xs text-muted-foreground">
                {t("shengsuanyun.enterpriseBudget.percentage", {
                  defaultValue: "剩余 {{percentage}}%",
                  percentage: Math.min(100, Math.max(0, percentage)).toFixed(1),
                })}
              </p>
            )}
          {!budget.unlimited && budget.remaining_cny === 0 && (
            <p className="text-xs text-amber-700 dark:text-amber-300">
              {t("shengsuanyun.enterpriseBudget.exhausted", {
                defaultValue: "本周期额度已用尽",
              })}
            </p>
          )}
          {current?.status === "ready" && (
            <p className="text-xs text-muted-foreground">
              {t("shengsuanyun.enterpriseBudget.updated", {
                defaultValue: "更新于 {{time}}",
                time: current.fetchedAt.toLocaleTimeString(i18n.language),
              })}
            </p>
          )}
        </>
      )}
    </section>
  );
}
