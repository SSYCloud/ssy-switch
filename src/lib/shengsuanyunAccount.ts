import type { TFunction } from "i18next";
import type { ShengsuanyunAccount } from "./api/shengsuanyun";

/** 只呈现 SDK 返回的身份，不根据昵称、登录按钮或脱敏 UID 推断。 */
export function ssyAccountLabel(
  t: TFunction,
  account: ShengsuanyunAccount,
): string {
  const type =
    account.accountType === "enterprise"
      ? t("shengsuanyun.accountTypeEnterprise", {
          defaultValue: "企业网关账号",
        })
      : account.accountType === "personal"
        ? t("shengsuanyun.accountTypePersonal", { defaultValue: "个人账号" })
        : t("shengsuanyun.accountIdentityUnknown", {
            defaultValue: "账号身份待确认",
          });
  const role =
    account.accountRole === "sub"
      ? t("shengsuanyun.accountRoleSub", { defaultValue: "子账号" })
      : account.accountRole === "main"
        ? t("shengsuanyun.accountRoleMain", { defaultValue: "主账号" })
        : null;
  return role ? `${type} · ${role}` : type;
}

export function hasPersonalWallet(account: ShengsuanyunAccount): boolean {
  return account.accountType === "personal" && account.accountRole === "main";
}
