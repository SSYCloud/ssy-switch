import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useRef,
} from "react";
import type { UpdateInfo } from "../lib/updater";
import { checkForUpdate } from "../lib/updater";
import { extractErrorMessage } from "../utils/errorUtils";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { settingsApi } from "../lib/api/settings";

export interface UpdateProgress {
  downloaded: number;
  total?: number | null;
}

interface UpdateContextValue {
  // 更新状态
  hasUpdate: boolean;
  updateInfo: UpdateInfo | null;
  isChecking: boolean;
  error: string | null;
  isDownloading: boolean;
  isInstalling: boolean;
  progress: UpdateProgress | null;
  installUpdate: () => Promise<boolean>;

  // 提示状态
  isDismissed: boolean;
  dismissUpdate: () => void;

  // 操作方法
  checkUpdate: () => Promise<boolean>;
  resetDismiss: () => void;
}

const UpdateContext = createContext<UpdateContextValue | undefined>(undefined);

export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const DISMISSED_VERSION_KEY = "ssyswitch:update:dismissedVersion";
  const LEGACY_DISMISSED_KEY = "dismissedUpdateVersion"; // 兼容旧键

  const [hasUpdate, setHasUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isDismissed, setIsDismissed] = useState(false);
  const [isDownloading, setIsDownloading] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const isInstallingRef = useRef(false);

  // 从 localStorage 读取已关闭的版本
  useEffect(() => {
    const current = updateInfo?.availableVersion;
    if (!current) return;

    // 读取新键；若不存在，尝试迁移旧键
    let dismissedVersion = localStorage.getItem(DISMISSED_VERSION_KEY);
    if (!dismissedVersion) {
      const legacy = localStorage.getItem(LEGACY_DISMISSED_KEY);
      if (legacy) {
        localStorage.setItem(DISMISSED_VERSION_KEY, legacy);
        localStorage.removeItem(LEGACY_DISMISSED_KEY);
        dismissedVersion = legacy;
      }
    }

    setIsDismissed(dismissedVersion === current);
  }, [updateInfo?.availableVersion]);

  const isCheckingRef = useRef(false);

  const checkUpdate = useCallback(async () => {
    if (isCheckingRef.current || isInstallingRef.current) return false;
    isCheckingRef.current = true;
    setIsChecking(true);
    setError(null);

    try {
      const result = await checkForUpdate({ timeout: 30000 });

      if (result.status === "available") {
        setHasUpdate(true);
        setUpdateInfo(result.info);

        // 检查是否已经关闭过这个版本的提醒
        let dismissedVersion = localStorage.getItem(DISMISSED_VERSION_KEY);
        if (!dismissedVersion) {
          const legacy = localStorage.getItem(LEGACY_DISMISSED_KEY);
          if (legacy) {
            localStorage.setItem(DISMISSED_VERSION_KEY, legacy);
            localStorage.removeItem(LEGACY_DISMISSED_KEY);
            dismissedVersion = legacy;
          }
        }
        setIsDismissed(dismissedVersion === result.info.availableVersion);
        return true; // 有更新
      } else {
        setHasUpdate(false);
        setUpdateInfo(null);
        setIsDismissed(false);
        return false; // 已是最新
      }
    } catch (err) {
      console.error("检查更新失败:", err);
      setError(extractErrorMessage(err) || "检查更新失败");
      setHasUpdate(false);
      setUpdateInfo(null);
      throw err; // 抛出错误让调用方处理
    } finally {
      setIsChecking(false);
      isCheckingRef.current = false;
    }
  }, []);

  const dismissUpdate = useCallback(() => {
    setIsDismissed(true);
    if (updateInfo?.availableVersion) {
      localStorage.setItem(DISMISSED_VERSION_KEY, updateInfo.availableVersion);
      // 清理旧键
      localStorage.removeItem(LEGACY_DISMISSED_KEY);
    }
  }, [updateInfo?.availableVersion]);

  const resetDismiss = useCallback(() => {
    setIsDismissed(false);
    localStorage.removeItem(DISMISSED_VERSION_KEY);
    localStorage.removeItem(LEGACY_DISMISSED_KEY);
  }, []);

  const installUpdate = useCallback(async () => {
    if (isInstallingRef.current || isCheckingRef.current) return false;
    if (!updateInfo) throw new Error("No update has been selected");
    isInstallingRef.current = true;
    setIsDownloading(true);
    setIsInstalling(false);
    setProgress(null);
    setError(null);
    const listeners: UnlistenFn[] = [];
    try {
      // Keep progress in the provider: switching settings tabs must not start a second install.
      listeners.push(
        await listen<UpdateProgress>(
          "update-download-progress",
          ({ payload }) => {
            setProgress(payload);
          },
        ),
      );
      listeners.push(
        await listen("update-installing", () => setIsInstalling(true)),
      );
      const installed = await settingsApi.installUpdateAndRestart(
        updateInfo.availableVersion,
      );
      if (!installed) {
        setHasUpdate(false);
        setUpdateInfo(null);
      }
      return installed;
    } catch (err) {
      setError(extractErrorMessage(err));
      setHasUpdate(false);
      setUpdateInfo(null);
      throw err;
    } finally {
      listeners.forEach((unlisten) => unlisten());
      isInstallingRef.current = false;
      setIsDownloading(false);
      setIsInstalling(false);
      setProgress(null);
    }
  }, [updateInfo]);

  // 应用启动时自动检查更新
  useEffect(() => {
    // 延迟1秒后检查，避免影响启动体验
    const timer = setTimeout(() => {
      checkUpdate().catch(console.error);
    }, 1000);

    const interval = setInterval(
      () => {
        if (navigator.onLine) void checkUpdate().catch(() => {});
      },
      6 * 60 * 60 * 1000,
    );
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, [checkUpdate]);

  const value: UpdateContextValue = {
    hasUpdate,
    updateInfo,
    isChecking,
    error,
    isDownloading,
    isInstalling,
    progress,
    installUpdate,
    isDismissed,
    dismissUpdate,
    checkUpdate,
    resetDismiss,
  };

  return (
    <UpdateContext.Provider value={value}>{children}</UpdateContext.Provider>
  );
}

export function useUpdate() {
  const context = useContext(UpdateContext);
  if (!context) {
    throw new Error("useUpdate must be used within UpdateProvider");
  }
  return context;
}
