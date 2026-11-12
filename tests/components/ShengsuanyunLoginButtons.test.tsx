import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ShengsuanyunLoginButtons } from "@/components/providers/ShengsuanyunLoginButtons";
import { startLoginFlow } from "@/lib/shengsuanyunFlow";
import { settingsApi } from "@/lib/api/settings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/api/settings", () => ({
  settingsApi: { openExternal: vi.fn().mockResolvedValue(undefined) },
}));
vi.mock("@/lib/api/analytics", () => ({
  analyticsApi: { track: vi.fn().mockResolvedValue(undefined) },
}));

describe("explicit Shengsuanyun account login", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it.each([
    ["个人账号登录", "main"],
    ["企业子账号登录", "sub"],
  ] as const)(
    "%s preserves the chosen account type through IPC",
    async (label, scope) => {
      const authorizationUrl = `https://login.example.test/${scope}`;
      vi.mocked(invoke).mockResolvedValueOnce({
        authorizationUrl,
        sessionId: "test-session",
      });
      render(
        <ShengsuanyunLoginButtons
          onLogin={(accountScope) =>
            void startLoginFlow("codex", "banner", accountScope)
          }
        />,
      );
      expect(
        screen.getByRole("button", { name: "个人账号登录" }),
      ).toBeVisible();
      expect(
        screen.getByRole("button", { name: "企业子账号登录" }),
      ).toBeVisible();
      await userEvent.click(screen.getByRole("button", { name: label }));
      await waitFor(() =>
        expect(settingsApi.openExternal).toHaveBeenCalledWith(authorizationUrl),
      );
      expect(invoke).toHaveBeenCalledTimes(1);
      expect(invoke).toHaveBeenCalledWith("shengsuanyun_start_login", {
        targetApp: "codex",
        targetProviderId: null,
        accountScope: scope,
      });
    },
  );
});
