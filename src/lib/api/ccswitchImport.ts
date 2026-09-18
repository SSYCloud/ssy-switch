// 从原版 CC Switch 一键导入（只读源库 + 本地备份后合并，导入卡片不激活）
import { invoke } from "@tauri-apps/api/core";

export interface ImportPreview {
  sourceExists: boolean;
  sourcePath: string;
  sourceVersion: number;
  total: number;
  perApp: [string, number][];
  conflicts: number;
}

export interface ImportResult {
  imported: number;
  skippedExisting: number;
  skippedInvalid: number;
  perApp: [string, number][];
  backupFile: string | null;
}

export function previewCcSwitchImport(
  path?: string,
): Promise<ImportPreview> {
  return invoke("ccswitch_import_preview", { path: path ?? null });
}

export function executeCcSwitchImport(
  path?: string,
): Promise<ImportResult> {
  return invoke("ccswitch_import_execute", { path: path ?? null });
}
