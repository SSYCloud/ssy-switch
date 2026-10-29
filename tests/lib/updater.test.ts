import { beforeEach, describe, expect, it, vi } from "vitest";
import { checkForUpdate } from "@/lib/updater";

const { check } = vi.hoisted(() => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check }));

describe("SSY-Switch update checks", () => {
  beforeEach(() => {
    check.mockReset();
  });

  it("returns metadata and releases the native resource without installing", async () => {
    const close = vi.fn().mockResolvedValue(undefined);
    const install = vi.fn();
    check.mockResolvedValue({
      currentVersion: "3.21.1",
      version: "3.21.2",
      body: "Release notes",
      date: "2026-10-29T00:00:00Z",
      close,
      install,
    });
    expect(await checkForUpdate()).toEqual({
      status: "available",
      info: {
        currentVersion: "3.21.1",
        availableVersion: "3.21.2",
        notes: "Release notes",
        pubDate: "2026-10-29T00:00:00Z",
      },
    });
    expect(check).toHaveBeenCalledWith({ timeout: 30000 });
    expect(close).toHaveBeenCalledOnce();
    expect(install).not.toHaveBeenCalled();
  });

  it("reports no update only when the updater returns none", async () => {
    check.mockResolvedValue(null);
    expect(await checkForUpdate({ timeout: 5000 })).toEqual({
      status: "up-to-date",
    });
    expect(check).toHaveBeenCalledWith({ timeout: 5000 });
  });

  it("does not present network or manifest errors as up-to-date", async () => {
    check.mockRejectedValue(new Error("Invalid manifest"));
    await expect(checkForUpdate()).rejects.toThrow("Invalid manifest");
  });
});
