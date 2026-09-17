// 胜算云 Key 选择器：编辑供应商时，从当前登录的胜算云账号名下挑一把 Key。
//
// 设计要点：
// - 输入框保持可编辑（用户仍可粘贴任意来源的 Key），选择器只是内嵌在右侧的快捷键
// - 列表只拿脱敏元数据；用户点中某一项时才单独取那一把的明文
//   （webview 同时最多持有一个 secret，而不是一次灌入全部 Key）
// - 选中后把 keyId 记到绑定表：重登 / 启动对账时会优先复用，不会悄悄改回默认 Key
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  ExternalLink,
  Loader2,
  RefreshCw,
} from "lucide-react";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import {
  shengsuanyunApi,
  type ShengsuanyunAccount,
  type ShengsuanyunBinding,
  type SsyToken,
} from "@/lib/api/shengsuanyun";

/** 账号列表进程内缓存：同一表单里通常有多个字段渲染本组件，避免重复 invoke */
let accountsCache: Promise<ShengsuanyunAccount[]> | null = null;

function loadAccounts(force = false): Promise<ShengsuanyunAccount[]> {
  if (force || !accountsCache) {
    accountsCache = shengsuanyunApi.listAccounts().catch((e) => {
      accountsCache = null;
      throw e;
    });
  }
  return accountsCache;
}

/** 登出/登录后让缓存失效（由设置页调用） */
export function invalidateSsyAccountsCache() {
  accountsCache = null;
}

interface SsyKeyPickerProps {
  /** 宿主的 app 标识（claude / codex / …），用于记录绑定关系 */
  appType: string;
  /** 编辑已有供应商时的 provider id；新增供应商时为空，此时只填 Key 不记录 */
  providerId?: string;
  /** 当前输入框里的 Key：仅用于把触发器点亮（有值时高亮） */
  value: string;
  onSelect: (apiKey: string, keyId: number) => void;
}

function formatYuan(v: number): string {
  return `¥${v.toFixed(2)}`;
}

export function SsyKeyPicker({
  appType,
  providerId,
  value,
  onSelect,
}: SsyKeyPickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [accounts, setAccounts] = useState<ShengsuanyunAccount[]>([]);
  const [accountId, setAccountId] = useState<string | null>(null);
  const [tokens, setTokens] = useState<SsyToken[]>([]);
  const [binding, setBinding] = useState<ShengsuanyunBinding | null>(null);
  const [loading, setLoading] = useState(false);
  const [pickingId, setPickingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [switching, setSwitching] = useState(false);

  // 未登录时整个触发器不出现（登录入口在「认证中心」）
  useEffect(() => {
    let alive = true;
    loadAccounts()
      .then((list) => {
        if (!alive) return;
        setAccounts(list);
        setAccountId((prev) => prev ?? list[0]?.id ?? null);
      })
      .catch(() => {
        /* 未登录/查询失败：静默隐藏选择器 */
      });
    return () => {
      alive = false;
    };
  }, []);

  // 面板打开时重新读一次账号（本地库查询，不联网）：
  // 覆盖「组件先挂载、用户随后登录」的情况；已选项失效时回落到第一个账号
  const refreshAccounts = useCallback(async () => {
    try {
      const list = await loadAccounts(true);
      setAccounts(list);
      setAccountId((prev) =>
        prev && list.some((a) => a.id === prev) ? prev : (list[0]?.id ?? null),
      );
    } catch {
      /* 未登录：保持隐藏 */
    }
  }, []);

  const refresh = useCallback(async () => {
    if (!accountId) return;
    setLoading(true);
    setError(null);
    try {
      const list = await shengsuanyunApi.listKeys(accountId);
      setTokens(list);
    } catch (e) {
      setTokens([]);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [accountId]);

  // 打开面板时才拉取（避免每次渲染表单都打一次上游接口）
  useEffect(() => {
    if (!open) return;
    void refreshAccounts();
    void refresh();
    if (!providerId) {
      setBinding(null);
      return;
    }
    let alive = true;
    shengsuanyunApi
      .getBinding(appType, providerId)
      .then((b) => {
        if (!alive) return;
        setBinding(b);
        if (b) setAccountId((prev) => prev ?? b.accountId);
      })
      .catch(() => {
        if (alive) setBinding(null);
      });
    return () => {
      alive = false;
    };
  }, [open, refresh, refreshAccounts, providerId, appType]);

  const handlePick = useCallback(
    async (token: SsyToken) => {
      if (!accountId || !token.selectable || pickingId !== null) return;
      setPickingId(token.id);
      setError(null);
      try {
        const plaintext = await shengsuanyunApi.revealKey(accountId, token.id);
        onSelect(plaintext, token.id);
        if (providerId) {
          try {
            await shengsuanyunApi.setBindingKey(
              appType,
              providerId,
              accountId,
              token.id,
            );
            setBinding((prev) =>
              prev
                ? { ...prev, accountId, keyId: token.id }
                : {
                    appType,
                    providerId,
                    accountId,
                    credentialSource: "key",
                    keyId: token.id,
                    updatedAt: Math.floor(Date.now() / 1000),
                  },
            );
          } catch (e) {
            // Key 已经填进输入框，落地失败只提示不阻塞
            console.warn("记录胜算云绑定 Key 失败", e);
          }
        }
        setOpen(false);
      } catch (e) {
        setError(String(e));
      } finally {
        setPickingId(null);
      }
    },
    [accountId, appType, onSelect, pickingId, providerId],
  );

  if (accounts.length === 0) return null;

  const hasValue = value.trim().length > 0;

  const activeAccount = accounts.find((a) => a.id === accountId) ?? accounts[0];
  // 没有显式记录时，默认 Key 视为当前生效项
  const selectedKeyId =
    binding && binding.accountId === activeAccount?.id ? binding.keyId : null;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          title={t("shengsuanyun.keyPicker.trigger", {
            defaultValue: "选择胜算云 API Key",
          })}
          aria-label={t("shengsuanyun.keyPicker.trigger", {
            defaultValue: "选择胜算云 API Key",
          })}
          className={cn(
            "flex h-7 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors",
            "hover:bg-accent hover:text-foreground",
            hasValue && "text-blue-500",
          )}
        >
          <ChevronDown size={15} />
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 p-0">
        {/* 账号行 */}
        <div className="flex items-center justify-between gap-2 border-b border-border-default px-3 py-2">
          <div className="min-w-0">
            <div className="truncate text-xs font-medium text-foreground">
              {activeAccount?.displayName ||
                activeAccount?.emailMasked ||
                "胜算云"}
            </div>
            <div className="truncate text-[11px] text-muted-foreground">
              {activeAccount?.balanceYuan != null
                ? t("shengsuanyun.keyPicker.balance", {
                    defaultValue: "余额 {{amount}}",
                    amount: formatYuan(activeAccount.balanceYuan),
                  })
                : activeAccount?.emailMasked}
            </div>
          </div>
          {accounts.length > 1 && (
            <button
              type="button"
              onClick={() => setSwitching((v) => !v)}
              className="shrink-0 rounded px-1.5 py-1 text-[11px] text-blue-500 hover:bg-accent"
            >
              {t("shengsuanyun.keyPicker.switchAccount", {
                defaultValue: "换个账号",
              })}
            </button>
          )}
        </div>

        {switching && accounts.length > 1 && (
          <div className="border-b border-border-default py-1">
            {accounts.map((a) => (
              <button
                key={a.id}
                type="button"
                onClick={() => {
                  setAccountId(a.id);
                  setSwitching(false);
                }}
                className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-accent"
              >
                <Check
                  size={12}
                  className={cn(
                    "shrink-0",
                    a.id === activeAccount?.id ? "opacity-100" : "opacity-0",
                  )}
                />
                <span className="truncate">
                  {a.displayName || a.emailMasked || a.uidMasked}
                </span>
              </button>
            ))}
          </div>
        )}

        {/* Key 列表 */}
        <div className="max-h-64 overflow-y-auto py-1">
          {loading && (
            <div className="flex items-center justify-center gap-2 py-4 text-xs text-muted-foreground">
              <Loader2 size={14} className="animate-spin" />
              {t("common.loading", { defaultValue: "加载中…" })}
            </div>
          )}

          {!loading && error && (
            <div className="px-3 py-3 text-xs text-red-500">
              <p className="break-all">{error}</p>
              <button
                type="button"
                onClick={() => void refresh()}
                className="mt-2 inline-flex items-center gap-1 text-blue-500 hover:underline"
              >
                <RefreshCw size={12} />
                {t("common.retry", { defaultValue: "重试" })}
              </button>
            </div>
          )}

          {!loading && !error && tokens.length === 0 && (
            <div className="px-3 py-3 text-xs text-muted-foreground">
              {t("shengsuanyun.keyPicker.empty", {
                defaultValue: "该账号下还没有 API Key",
              })}
            </div>
          )}

          {!loading &&
            !error &&
            tokens.map((token) => {
              const isSelected =
                token.id === selectedKeyId ||
                (selectedKeyId == null && token.isDefault);
              return (
                <button
                  key={token.id}
                  type="button"
                  disabled={!token.selectable || pickingId !== null}
                  onClick={() => void handlePick(token)}
                  className={cn(
                    "flex w-full items-start gap-2 px-3 py-2 text-left transition-colors",
                    token.selectable
                      ? "hover:bg-accent"
                      : "cursor-not-allowed opacity-50",
                  )}
                >
                  <span className="mt-0.5 flex w-3 shrink-0 justify-center">
                    {pickingId === token.id ? (
                      <Loader2 size={12} className="animate-spin" />
                    ) : (
                      <Check
                        size={12}
                        className={cn(isSelected ? "opacity-100" : "opacity-0")}
                      />
                    )}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span className="truncate text-xs font-medium text-foreground">
                        {token.name || `Key #${token.id}`}
                      </span>
                      {token.isDefault && (
                        <span className="shrink-0 rounded bg-muted px-1 py-px text-[10px] text-muted-foreground">
                          {t("shengsuanyun.keyPicker.defaultBadge", {
                            defaultValue: "默认",
                          })}
                        </span>
                      )}
                      {token.isBanned && (
                        <span className="shrink-0 rounded bg-red-500/10 px-1 py-px text-[10px] text-red-500">
                          {t("shengsuanyun.keyPicker.banned", {
                            defaultValue: "已禁用",
                          })}
                        </span>
                      )}
                      {token.isExpired && (
                        <span className="shrink-0 rounded bg-amber-500/10 px-1 py-px text-[10px] text-amber-600">
                          {t("shengsuanyun.keyPicker.expired", {
                            defaultValue: "已过期",
                          })}
                        </span>
                      )}
                    </span>
                    <span className="mt-0.5 block truncate font-mono text-[10px] text-muted-foreground">
                      {token.tokenMasked}
                    </span>
                    <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
                      {t("shengsuanyun.keyPicker.used", {
                        defaultValue: "已用 {{used}} / {{limit}}",
                        used: formatYuan(token.consumedYuan),
                        limit:
                          token.maxQuotaYuan == null
                            ? t("shengsuanyun.keyPicker.unlimited", {
                                defaultValue: "无上限",
                              })
                            : formatYuan(token.maxQuotaYuan),
                      })}
                    </span>
                  </span>
                </button>
              );
            })}
        </div>

        {/* 底部：去控制台管理 */}
        <div className="border-t border-border-default px-3 py-2">
          <a
            href="https://console.shengsuanyun.com/user/token"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-blue-500"
          >
            <ExternalLink size={11} />
            {t("shengsuanyun.keyPicker.manage", {
              defaultValue: "在胜算云控制台创建 / 管理 Key",
            })}
          </a>
          {!providerId && (
            <span className="ml-2 text-[11px] text-muted-foreground">
              {t("shengsuanyun.keyPicker.saveHint", {
                defaultValue: "（保存后写入供应商配置）",
              })}
            </span>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

export default SsyKeyPicker;
