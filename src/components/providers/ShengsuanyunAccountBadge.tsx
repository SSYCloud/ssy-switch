import { useTranslation } from "react-i18next";
import type { ShengsuanyunAccount } from "@/lib/api/shengsuanyun";
import { ssyAccountLabel } from "@/lib/shengsuanyunAccount";

export function ShengsuanyunAccountBadge({
  account,
}: {
  account: ShengsuanyunAccount;
}) {
  const { t } = useTranslation();
  return (
    <span className="inline-flex shrink-0 items-center rounded-full border border-border px-2 py-0.5 text-xs font-normal text-muted-foreground">
      {ssyAccountLabel(t, account)}
    </span>
  );
}
