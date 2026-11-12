import { Building2, UserRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { ShengsuanyunAccountScope } from "@/lib/api/shengsuanyun";

interface Props {
  onLogin: (scope: ShengsuanyunAccountScope) => void;
  disabled?: boolean;
  size?: "default" | "sm" | "lg";
}

export function ShengsuanyunLoginButtons({
  onLogin,
  disabled = false,
  size = "default",
}: Props) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap gap-2">
      <Button size={size} disabled={disabled} onClick={() => onLogin("main")}>
        <UserRound className="mr-2 h-4 w-4" />
        {t("shengsuanyun.personalLogin", { defaultValue: "个人账号登录" })}
      </Button>
      <Button size={size} disabled={disabled} onClick={() => onLogin("sub")}>
        <Building2 className="mr-2 h-4 w-4" />
        {t("shengsuanyun.enterpriseLogin", { defaultValue: "企业子账号登录" })}
      </Button>
    </div>
  );
}
