import { describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { emitTauriEvent } from "../msw/tauriMocks";
import { installGlobalLoginBridge } from "@/lib/shengsuanyunFlow";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/api/analytics", () => ({
  analyticsApi: { track: vi.fn().mockResolvedValue(undefined) },
}));
vi.mock("@/lib/api/connectivity-check", () => ({
  streamCheckProvider: vi
    .fn()
    .mockResolvedValue({ success: true, status: "operational" }),
}));

describe("login completion account binding", () => {
  it("binds the exact account and app from the completed session without guessing from list order", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "shengsuanyun_list_accounts")
        return [{ id: "unrelated-personal" }];
      return { status: "ok", providerId: null, accountId: "verified-sub" };
    });
    await installGlobalLoginBridge();
    emitTauriEvent("shengsuanyun-oauth-complete", {
      account: {
        id: "verified-sub",
        accountType: "enterprise",
        accountRole: "sub",
      },
      appType: "codex",
      providerId: null,
    });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("shengsuanyun_bind_account", expect.objectContaining({
        appType: "codex",
        accountId: "verified-sub",
      })),
    );
    expect(invoke).not.toHaveBeenCalledWith("shengsuanyun_list_accounts");
  });
});
