import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateProvider, useUpdate } from "@/contexts/UpdateContext";
import { emitTauriEvent } from "../msw/tauriMocks";

const mocks = vi.hoisted(() => ({ check: vi.fn(), install: vi.fn() }));
vi.mock("@/lib/updater", () => ({ checkForUpdate: mocks.check }));
vi.mock("@/lib/api/settings", () => ({
  settingsApi: { installUpdateAndRestart: mocks.install },
}));
const available = {
  status: "available",
  info: { currentVersion: "3.21.1", availableVersion: "3.21.2" },
};

describe("application update lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    mocks.check.mockReset().mockResolvedValue(available);
    mocks.install.mockReset().mockResolvedValue(true);
  });
  afterEach(() => vi.useRealTimers());

  it("checks at startup and periodically, but never installs without a click", async () => {
    const { result, unmount } = renderHook(useUpdate, {
      wrapper: UpdateProvider,
    });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(result.current.hasUpdate).toBe(true);
    await act(() => vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000));
    expect(mocks.check).toHaveBeenCalledTimes(2);
    expect(mocks.install).not.toHaveBeenCalled();
    unmount();
    await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000);
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });

  it("clears stale available metadata when a later check fails", async () => {
    const { result } = renderHook(useUpdate, { wrapper: UpdateProvider });
    await act(async () => {
      await result.current.checkUpdate();
    });
    mocks.check.mockRejectedValueOnce(new Error("offline"));
    await act(async () => {
      await expect(result.current.checkUpdate()).rejects.toThrow("offline");
    });
    expect(result.current.hasUpdate).toBe(false);
    expect(result.current.updateInfo).toBeNull();
    expect(result.current.error).toBe("offline");
  });

  it("keeps one install active, shows progress and releases state on failure", async () => {
    let reject!: (e: Error) => void;
    mocks.install.mockImplementation(
      () =>
        new Promise((_, no) => {
          reject = no;
        }),
    );
    const { result } = renderHook(useUpdate, { wrapper: UpdateProvider });
    await act(async () => {
      await result.current.checkUpdate();
    });
    let installing!: Promise<boolean>;
    await act(async () => {
      installing = result.current.installUpdate();
    });
    expect(mocks.install).toHaveBeenCalledWith("3.21.2");
    await act(async () => {
      await result.current.installUpdate();
      await result.current.checkUpdate();
    });
    expect(mocks.install).toHaveBeenCalledOnce();
    expect(mocks.check).toHaveBeenCalledOnce();
    act(() =>
      emitTauriEvent("update-download-progress", {
        downloaded: 50,
        total: 100,
      }),
    );
    expect(result.current.progress).toEqual({ downloaded: 50, total: 100 });
    act(() => emitTauriEvent("update-installing", null));
    expect(result.current.isInstalling).toBe(true);
    await act(async () => {
      reject(new Error("Signature mismatch"));
      await expect(installing).rejects.toThrow("Signature mismatch");
    });
    expect(result.current.isDownloading).toBe(false);
    expect(result.current.hasUpdate).toBe(false);
    expect(result.current.progress).toBeNull();
    act(() =>
      emitTauriEvent("update-download-progress", {
        downloaded: 99,
        total: 100,
      }),
    );
    expect(result.current.progress).toBeNull();
    await act(async () => {
      await result.current.checkUpdate();
    });
    mocks.install.mockResolvedValueOnce(true);
    await act(async () => {
      expect(await result.current.installUpdate()).toBe(true);
    });
    expect(mocks.install).toHaveBeenCalledTimes(2);
  });

  it("clears a withdrawn update without reporting an installation", async () => {
    const { result } = renderHook(useUpdate, { wrapper: UpdateProvider });
    await act(async () => {
      await result.current.checkUpdate();
    });
    mocks.install.mockResolvedValueOnce(false);
    await act(async () => {
      expect(await result.current.installUpdate()).toBe(false);
    });
    expect(result.current.hasUpdate).toBe(false);
  });
});
