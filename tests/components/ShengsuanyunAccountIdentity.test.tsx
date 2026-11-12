import { act, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ShengsuanyunAccount } from "@/lib/api/shengsuanyun";
import { ShengsuanyunAuthSection } from "@/components/providers/forms/ShengsuanyunAuthSection";
import { emitTauriEvent } from "../msw/tauriMocks";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/components/providers/forms/ShengsuanyunUsageStats", () => ({
  ShengsuanyunUsageStats: () => null,
}));
vi.mock("@/components/providers/forms/ShengsuanyunBillList", () => ({
  ShengsuanyunBillList: () => null,
}));
vi.mock("@/components/providers/forms/ShengsuanyunVoucherList", () => ({
  ShengsuanyunVoucherList: () => null,
}));

const account: ShengsuanyunAccount = {
  id: "sub-account",
  displayName: "企业管理员",
  uidMasked: "su*****7",
  emailMasked: "",
  avatarUrl: "",
  isCreator: false,
  balanceYuan: null,
  voucherYuan: null,
  balanceUpdatedAt: null,
  createdAt: 1,
  accountType: "enterprise",
  accountRole: "sub",
};

describe("verified account identity presentation", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("shows enterprise subaccount identity after login and does not offer personal wallet actions", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "shengsuanyun_enterprise_budget")
        throw new Error("Budget unavailable in identity test");
      return [account];
    });
    const first = render(<ShengsuanyunAuthSection />);
    expect(await screen.findByText("企业网关账号 · 子账号")).toBeVisible();
    expect(screen.getByText("使用企业项目预算")).toBeVisible();
    expect(screen.getByRole("region", { name: "企业项目预算" })).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "充值" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "刷新余额" }),
    ).not.toBeInTheDocument();
    await act(async () => {
      emitTauriEvent("shengsuanyun-oauth-complete", {
        account,
        appType: "codex",
        providerId: null,
      });
    });
    const notice = await screen.findByRole("status");
    expect(within(notice).getByText("企业网关账号 · 子账号")).toBeVisible();
    expect(notice).toHaveTextContent("企业管理员");
    first.unmount();
    render(<ShengsuanyunAuthSection />);
    expect(await screen.findByText("企业网关账号 · 子账号")).toBeVisible();
  });

  it("identifies a personal main account and preserves its wallet actions", async () => {
    vi.mocked(invoke).mockResolvedValue([
      {
        ...account,
        id: "main-account",
        accountType: "personal",
        accountRole: "main",
        balanceYuan: 25,
      },
    ]);
    render(<ShengsuanyunAuthSection />);
    expect(await screen.findByText("个人账号 · 主账号")).toBeVisible();
    expect(screen.getByRole("button", { name: "充值" })).toBeVisible();
    expect(screen.getByRole("button", { name: "刷新余额" })).toBeVisible();
  });

  it("does not infer identity from a nickname or masked UID when SDK fields are missing", async () => {
    vi.mocked(invoke).mockResolvedValue([
      { ...account, accountType: undefined, accountRole: undefined },
    ]);
    render(<ShengsuanyunAuthSection />);
    expect(await screen.findByText("账号身份待确认")).toBeVisible();
    expect(screen.queryByText("个人账号 · 主账号")).not.toBeInTheDocument();
    expect(screen.queryByText("企业网关账号 · 子账号")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("shengsuanyun_list_accounts"),
    );
  });
});
