import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type {
  ShengsuanyunAccount,
  ShengsuanyunEnterpriseBudget as Budget,
} from "@/lib/api/shengsuanyun";
import { ShengsuanyunEnterpriseBudget } from "@/components/providers/forms/ShengsuanyunEnterpriseBudget";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const account: ShengsuanyunAccount = {
  id: "enterprise-sub-1",
  accountType: "enterprise",
  accountRole: "sub",
  displayName: "测试子账号",
  uidMasked: "su****1",
  emailMasked: "",
  avatarUrl: "",
  isCreator: false,
  balanceYuan: null,
  voucherYuan: null,
  balanceUpdatedAt: null,
  createdAt: 1,
};
const budget: Budget = {
  scope: "account_in_project",
  project_id: 81,
  ram_user_id: 7,
  budget_kind: "monthly",
  period_kind: "monthly",
  period_start: "2026-11-01T00:00:00+08:00",
  period_end: "2026-12-01T00:00:00+08:00",
  period_consumed_cny: 20.125,
  period_limit_cny: 100,
  remaining_cny: 79.875,
  remaining_percentage: 79.875,
  unlimited: false,
};

describe("enterprise project budget", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("loads the account's project budget and displays yuan without another unit conversion", async () => {
    vi.mocked(invoke).mockResolvedValue(budget);
    render(<ShengsuanyunEnterpriseBudget account={account} />);
    expect(await screen.findByText("项目 #81")).toBeVisible();
    expect(screen.getByText("本子账号在登录所选项目中的额度")).toBeVisible();
    expect(screen.getByText(/月度预算/)).toBeVisible();
    expect(screen.getByText("¥20.125")).toBeVisible();
    expect(screen.getByText("¥100.00")).toBeVisible();
    expect(screen.getByText("¥79.875")).toBeVisible();
    expect(screen.getByText("剩余 79.9%")).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("shengsuanyun_enterprise_budget", {
      accountId: account.id,
    });
  });

  it("displays unlimited separately from exhausted or zero funds", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...budget,
      unlimited: true,
      period_limit_cny: null,
      remaining_cny: null,
      remaining_percentage: null,
    });
    render(<ShengsuanyunEnterpriseBudget account={account} />);
    await screen.findByText("项目 #81");
    expect(screen.getAllByText("无限额")).toHaveLength(2);
    expect(screen.queryByText("¥0.00")).not.toBeInTheDocument();
    expect(screen.queryByText("本周期额度已用尽")).not.toBeInTheDocument();
  });

  it("shows zero remaining and an exhausted notice for a spent daily budget", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...budget,
      period_kind: "daily",
      budget_kind: "daily",
      period_consumed_cny: 105,
      remaining_cny: 0,
      remaining_percentage: 0,
    });
    render(<ShengsuanyunEnterpriseBudget account={account} />);
    expect(await screen.findByText("本周期额度已用尽")).toBeVisible();
    expect(screen.getByText(/每日预算/)).toBeVisible();
    expect(screen.getByText("¥0.00")).toBeVisible();
  });

  it("allows retry after failure without turning the failure into zero or displaying raw credential errors", async () => {
    vi.mocked(invoke)
      .mockRejectedValueOnce(new Error("upstream: synthetic-secret"))
      .mockResolvedValueOnce(budget);
    render(<ShengsuanyunEnterpriseBudget account={account} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("预算读取失败");
    expect(screen.queryByText(/synthetic-secret/)).not.toBeInTheDocument();
    expect(screen.queryByText("¥0.00")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "刷新预算" }));
    expect(await screen.findByText("¥79.875")).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("refreshes an existing result and prevents duplicate requests while loading", async () => {
    let resolveRefresh!: (value: Budget) => void;
    vi.mocked(invoke)
      .mockResolvedValueOnce(budget)
      .mockImplementationOnce(
        () =>
          new Promise<Budget>((resolve) => {
            resolveRefresh = resolve;
          }),
      );
    render(<ShengsuanyunEnterpriseBudget account={account} />);
    await screen.findByText("¥79.875");
    await userEvent.click(screen.getByRole("button", { name: "刷新预算" }));
    expect(screen.getByRole("button", { name: "刷新预算" })).toBeDisabled();
    expect(screen.queryByText("¥79.875")).not.toBeInTheDocument();
    await act(async () =>
      resolveRefresh({ ...budget, period_consumed_cny: 30, remaining_cny: 70 }),
    );
    expect(await screen.findByText("¥70.00")).toBeVisible();
  });

  it.each(["another-account", account.id])(
    "ignores an old request after switching or signing in again (%s)",
    async (nextId) => {
      let resolveOld!: (value: Budget) => void;
      vi.mocked(invoke)
        .mockImplementationOnce(
          () =>
            new Promise<Budget>((resolve) => {
              resolveOld = resolve;
            }),
        )
        .mockResolvedValueOnce({
          ...budget,
          project_id: 92,
          remaining_cny: 60,
        });
      const view = render(<ShengsuanyunEnterpriseBudget account={account} />);
      const next = { ...account, id: nextId };
      view.rerender(<ShengsuanyunEnterpriseBudget account={next} />);
      await screen.findByText("项目 #92");
      await act(async () => resolveOld(budget));
      expect(screen.queryByText("项目 #81")).not.toBeInTheDocument();
      expect(screen.getByText("¥60.00")).toBeVisible();
      await waitFor(() =>
        expect(invoke).toHaveBeenLastCalledWith(
          "shengsuanyun_enterprise_budget",
          { accountId: nextId },
        ),
      );
    },
  );
});
