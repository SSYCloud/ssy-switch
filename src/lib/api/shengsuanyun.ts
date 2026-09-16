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

export const shengsuanyunApi = {
  startLogin: startShengsuanyunLogin,
  cancelLogin: cancelShengsuanyunLogin,
  listAccounts: listShengsuanyunAccounts,
  getStatus: getShengsuanyunStatus,
  refreshBalance: refreshShengsuanyunBalance,
  logout: logoutShengsuanyun,
  bindAccount: bindShengsuanyunAccount,
  bindAllApps: bindShengsuanyunAllApps,
};
