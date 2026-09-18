// 首次使用引导：检测到本机装有原版 CC Switch 时，在主页提供一键导入入口。
// 复用设置→高级的导入能力（只读源库 ~/.cc-switch，导入前后端自动备份本地库，
// 导入的卡片不激活）。导入完成或用户关闭卡片后写入 localStorage，不再出现。
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Database, HardDriveDownload, Loader2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  executeCcSwitchImport,
  previewCcSwitchImport,
  type ImportPreview,
} from "@/lib/api/ccswitchImport";

const DISMISSED_KEY = "ssy:ccswitchGuideDismissed";

function markDismissed() {
  try {
    localStorage.setItem(DISMISSED_KEY, String(Date.now()));
  } catch {
    /* localStorage 不可用时静默：仅本次会话不再显示 */
  }
}

export function CcSwitchImportGuide() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    let disposed = false;
    const check = async () => {
      try {
        if (localStorage.getItem(DISMISSED_KEY)) return;
        const pv = await previewCcSwitchImport();
        if (!disposed && pv.sourceExists && pv.total > 0) {
          setPreview(pv);
        }
      } catch {
        /* Tauri IPC 不可用（纯浏览器调试）时不显示引导 */
      }
    };
    void check();
    return () => {
      disposed = true;
    };
  }, []);

  const doImport = async () => {
    setImporting(true);
    try {
      const r = await executeCcSwitchImport();
      markDismissed();
      toast.success(
        t("settings.ccswitchImport.done", {
          n: r.imported,
          skip: r.skippedExisting,
          defaultValue:
            "导入完成：新增 {{n}} 个供应商，跳过已有 {{skip}} 个。导入的卡片均未激活，请在列表中手动启用。",
        }),
      );
      // 立即刷新各 App 的供应商列表
      void queryClient.invalidateQueries({ queryKey: ["providers"] });
      setPreview(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setImporting(false);
    }
  };

  const dismiss = () => {
    markDismissed();
    setPreview(null);
  };

  if (!preview) return null;

  return (
    <div className="rounded-2xl border border-blue-500/30 bg-gradient-to-r from-blue-500/10 via-blue-500/5 to-transparent p-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/15">
          <HardDriveDownload className="h-5 w-5 text-blue-500" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-sm font-semibold">
            {t("ccswitchGuide.title", { defaultValue: "cc-switch 一键导入" })}
          </div>
          <p className="mt-0.5 truncate text-xs text-muted-foreground" title={preview.sourcePath}>
            {t("ccswitchGuide.body", {
              total: preview.total,
              defaultValue:
                "检测到原版 CC Switch 的 {{total}} 个供应商配置。只读导入、不覆盖已有卡片，导入前自动备份本地数据。",
            })}
          </p>
        </div>
        <Button
          size="sm"
          disabled={importing}
          onClick={() => void doImport()}
          className="shrink-0"
        >
          {importing ? (
            <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
          ) : (
            <Database className="mr-1.5 h-3.5 w-3.5" />
          )}
          {t("ccswitchGuide.import", { defaultValue: "导入" })}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 text-muted-foreground"
          aria-label={t("common.close", { defaultValue: "关闭" })}
          onClick={dismiss}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
