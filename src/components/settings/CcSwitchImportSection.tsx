// 设置 → 高级：从原版 CC Switch 一键导入供应商配置。
// 只读源库（绝不写 ~/.cc-switch），导入前自动备份本地库；导入的卡片不激活。
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Database, Download, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  executeCcSwitchImport,
  previewCcSwitchImport,
  type ImportPreview,
} from "@/lib/api/ccswitchImport";

type Step = "idle" | "previewing" | "confirm" | "importing" | "done" | "error";

export function CcSwitchImportSection() {
  const { t } = useTranslation();
  const [step, setStep] = useState<Step>("idle");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{
    imported: number;
    skippedExisting: number;
    backupFile: string | null;
  } | null>(null);

  const doPreview = useCallback(async () => {
    setStep("previewing");
    setError(null);
    try {
      const pv = await previewCcSwitchImport();
      if (!pv.sourceExists) {
        setError(
          t("settings.ccswitchImport.notFound", {
            defaultValue: "未找到原版 CC Switch 数据库（~/.cc-switch/cc-switch.db）",
          }),
        );
        setStep("error");
        return;
      }
      setPreview(pv);
      setStep("confirm");
    } catch (e) {
      setError(String(e));
      setStep("error");
    }
  }, [t]);

  const doImport = useCallback(async () => {
    setStep("importing");
    try {
      const r = await executeCcSwitchImport();
      setResult({
        imported: r.imported,
        skippedExisting: r.skippedExisting,
        backupFile: r.backupFile,
      });
      setStep("done");
    } catch (e) {
      setError(String(e));
      setStep("error");
    }
  }, []);

  return (
    <div className="rounded-xl glass-card p-6 space-y-3">
      <div className="flex items-center gap-3">
        <Download className="h-5 w-5 text-primary" />
        <div>
          <h3 className="font-medium">
            {t("settings.ccswitchImport.title", {
              defaultValue: "从 CC Switch 导入",
            })}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.ccswitchImport.description", {
              defaultValue:
                "读取原版 CC Switch 的供应商配置（只读，不会修改原版数据）。导入的卡片不会自动激活，导入前会自动备份本地数据。",
            })}
          </p>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <Button size="sm" onClick={doPreview} disabled={step === "previewing"}>
          {step === "previewing" ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <Database className="mr-2 h-4 w-4" />
          )}
          {t("settings.ccswitchImport.scan", { defaultValue: "扫描原版数据" })}
        </Button>
      </div>

      {step === "confirm" && preview && (
        <div className="space-y-2 rounded-lg border border-border/60 bg-muted/40 p-4 text-sm">
          <p>
            {t("settings.ccswitchImport.found", {
              defaultValue:
                "在原版 CC Switch（schema v{{version}}）中找到 {{total}} 个供应商：",
            })
              .replace("{{version}}", String(preview.sourceVersion))
              .replace("{{total}}", String(preview.total))}
          </p>
          <ul className="list-disc pl-5 text-muted-foreground">
            {preview.perApp.map(([app, n]) => (
              <li key={app}>
                {app}: {n}
              </li>
            ))}
          </ul>
          {preview.conflicts > 0 && (
            <p className="text-amber-600 dark:text-amber-400">
              {t("settings.ccswitchImport.conflicts", {
                defaultValue:
                  "{{n}} 个与现有卡片同名同 ID，将被跳过（不会覆盖）。",
              }).replace("{{n}}", String(preview.conflicts))}
            </p>
          )}
          <div className="flex gap-2 pt-1">
            <Button size="sm" onClick={doImport}>
              {t("settings.ccswitchImport.confirm", {
                defaultValue: "备份并导入",
              })}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setStep("idle")}
            >
              {t("common.cancel", { defaultValue: "取消" })}
            </Button>
          </div>
        </div>
      )}

      {step === "done" && result && (
        <div className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 p-4 text-sm">
          <p>
            {t("settings.ccswitchImport.done", {
              defaultValue:
                "导入完成：新增 {{n}} 个供应商，跳过已有 {{skip}} 个。导入的卡片均未激活，请在列表中手动启用。",
            })
              .replace("{{n}}", String(result.imported))
              .replace("{{skip}}", String(result.skippedExisting))}
          </p>
          {result.backupFile && (
            <p className="mt-1 text-xs text-muted-foreground">
              {t("settings.ccswitchImport.backup", {
                defaultValue: "已自动备份本地数据",
              })}
            </p>
          )}
        </div>
      )}

      {step === "error" && error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
