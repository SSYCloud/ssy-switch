// 胜算云 OAuth 前端 API 层。
// 所有返回 DTO 均为脱敏数据（无 API Key / code / state）。
import { invoke } from "@tauri-apps/api/core";

export interface ShengsuanyunAccount {
  id: string;
  uidMasked: string;
  displayName: string;
  emailMasked: string;
  avatarUrl: string;
  isCreator: boolean;
  /** 余额（元），null 表示尚未获取 */
  balanceYuan: number | null;
  /** 体验券（元），null 表示尚未获取 */
  voucherYuan: number | null;
  balanceUpdatedAt: number | null;
  createdAt: number;
}

export interface ShengsuanyunLoginStart {
  authorizationUrl: string;
  sessionId: string;
}

export interface ShengsuanyunBindResult {
  status: "ok" | "conflict";
  providerId: string | null;
  accountId: string | null;
}

/** 上游 `/token/list` 的脱敏视图（只有掩码，没有明文） */
export interface SsyToken {
  id: number;
  name: string;
  tokenMasked: string;
  desc: string;
  isDefault: boolean;
  isBanned: boolean;
  isExpired: boolean;
  /** 可选：被禁用或已过期的 Key 不可绑定 */
  selectable: boolean;
  /** 额度上限（元）；null 表示无上限 */
  maxQuotaYuan: number | null;
  consumedYuan: number;
  createdAt: number;
  expiresAt: number;
  supportedModels: string[];
}

/** app/provider ↔ 胜算云账号的绑定（含用户选中的 Key ID） */
export interface ShengsuanyunBinding {
  appType: string;
  providerId: string;
  accountId: string;
  credentialSource: string;
  /** 用户显式选中的 Token ID；null 表示使用账号默认 Key */
  keyId: number | null;
  updatedAt: number;
}

export function startShengsuanyunLogin(
  targetApp?: string | null,
  targetProviderId?: string | null,
): Promise<ShengsuanyunLoginStart> {
  return invoke("shengsuanyun_start_login", {
    targetApp: targetApp ?? null,
    targetProviderId: targetProviderId ?? null,
  });
}

export function cancelShengsuanyunLogin(sessionId: string): Promise<boolean> {
  return invoke("shengsuanyun_cancel_login", { sessionId });
}

export function listShengsuanyunAccounts(): Promise<ShengsuanyunAccount[]> {
  return invoke("shengsuanyun_list_accounts");
}

export function getShengsuanyunStatus(
  appType: string,
  providerId: string,
): Promise<ShengsuanyunAccount | null> {
  return invoke("shengsuanyun_get_status", { appType, providerId });
}

export function refreshShengsuanyunBalance(accountId: string): Promise<number> {
  return invoke("shengsuanyun_refresh_balance", { accountId });
}

export function logoutShengsuanyun(accountId: string): Promise<void> {
  return invoke("shengsuanyun_logout", { accountId });
}

export function bindShengsuanyunAllApps(
  accountId: string,
): Promise<ShengsuanyunBindResult[]> {
  return invoke("shengsuanyun_bind_all_apps", { accountId });
}

export interface UsageDayDetail {
  model: string;
  /** 1e-7 元 */
  total_amount: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
}

export interface UsageDay {
  date: string;
  details: UsageDayDetail[];
}

export interface UserUsageResponse {
  usages: UsageDay[];
}

export interface UsageSummary {
  daily: { date: string; amount_yuan: number; amountYuan?: number }[];
  models: { model: string; amount_yuan: number; tokens: number }[];
  total_yuan: number;
  total_tokens: number;
}

export function getUsageSummary(
  startDate: string,
  endDate: string,
): Promise<UsageSummary> {
  return invoke("shengsuanyun_usage_summary", { startDate, endDate });
}

export function getUserUsage(
  startDate: string,
  endDate: string,
): Promise<UserUsageResponse> {
  return invoke("shengsuanyun_user_usage", { startDate, endDate });
}

export interface BillEntry {
  ID: number;
  CreatedAt: string;
  /** 1e-4 元 */
  Asset: number;
  /** 变动后余额，1e-4 元 */
  Balance: number;
  BalanceStatementType: string;
  BillType: string;
  BillSubType: string;
}

export function getBillList(page = 1, pageSize = 10): Promise<{ bills: BillEntry[] }> {
  return invoke("shengsuanyun_bill_list", { page, pageSize });
}

/** 上游 /voucher/user_voucher_list 的单张代金券（金额字段单位 1e-4 元） */
export interface VoucherRecord {
  id: number;
  title: string;
  des: string;
  /** success=可用；used=已用完；其余（如 expired）按原文展示 */
  status: string;
  original_amount: number;
  remaining_amount: number;
  used_amount: number;
  expired_amount: number;
  /** all=通用；其它值（如 loomloom）为产品专属券，不计入"体验券"汇总 */
  scope: string;
  redeem_time: string;
  expire_time: string;
}

export function getVoucherList(): Promise<{
  voucher_records: VoucherRecord[];
  total: number;
}> {
  return invoke("shengsuanyun_voucher_list");
}

export interface ModalityUsageResponse {
  usages: { date: string; details: { model: string; amount_yuan: number }[] }[];
  total: number;
}

export function getModalityUsage(
  startDate: string,
  endDate: string,
): Promise<ModalityUsageResponse> {
  return invoke("shengsuanyun_modality_usage", { startDate, endDate });
}

export function bindShengsuanyunAccount(
  appType: string,
  accountId: string,
  activate = true,
  overwrite = false,
): Promise<ShengsuanyunBindResult> {
  return invoke("shengsuanyun_bind_account", {
    appType,
    accountId,
    activate,
    overwrite,
  });
}

/** 列出某账号名下全部 API Key（脱敏） */
export function listShengsuanyunKeys(accountId: string): Promise<SsyToken[]> {
  return invoke("shengsuanyun_list_keys", { accountId });
}

/** 按需取单把 Key 的明文（用户点选某个 Key 时才调用） */
export function revealShengsuanyunKey(
  accountId: string,
  keyId: number,
): Promise<string> {
  return invoke("shengsuanyun_reveal_key", { accountId, keyId });
}

export function getShengsuanyunBinding(
  appType: string,
  providerId: string,
): Promise<ShengsuanyunBinding | null> {
  return invoke("shengsuanyun_get_binding", { appType, providerId });
}

/** 记录该供应商使用哪一把 Key（仅元数据，不落明文） */
export function setShengsuanyunBindingKey(
  appType: string,
  providerId: string,
  accountId: string,
  keyId: number | null,
): Promise<boolean> {
  return invoke("shengsuanyun_set_binding_key", {
    appType,
    providerId,
    accountId,
    keyId,
  });
}

export const shengsuanyunApi = {
  startLogin: startShengsuanyunLogin,
  cancelLogin: cancelShengsuanyunLogin,
  listAccounts: listShengsuanyunAccounts,
  getStatus: getShengsuanyunStatus,
  refreshBalance: refreshShengsuanyunBalance,
  logout: logoutShengsuanyun,
  bindAccount: bindShengsuanyunAccount,
  bindAllApps: bindShengsuanyunAllApps,
  listKeys: listShengsuanyunKeys,
  revealKey: revealShengsuanyunKey,
  getBinding: getShengsuanyunBinding,
  setBindingKey: setShengsuanyunBindingKey,
  getUserUsage,
  getUsageSummary,
  getBillList,
  getModalityUsage,
  getVoucherList,
};
