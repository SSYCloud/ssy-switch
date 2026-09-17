import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  SsyKeyPicker,
  invalidateSsyAccountsCache,
} from "@/components/providers/forms/shared/SsyKeyPicker";
import {
  ssyBannedKeyFixture,
  stubSsyKeyCommands,
} from "../msw/syyKeyStubs";

const renderPicker = (
  overrides: Partial<Parameters<typeof SsyKeyPicker>[0]> = {},
) => {
  const onSelect = vi.fn();
  const props = {
    appType: "claude",
    providerId: "p1",
    value: "",
    onSelect,
    ...overrides,
  };
  const utils = render(<SsyKeyPicker {...props} />);
  return { onSelect, props, ...utils };
};

const openPanel = async () => {
  const trigger = await screen.findByRole("button", {
    name: "选择胜算云 API Key",
  });
  fireEvent.click(trigger);
  return trigger;
};

describe("SsyKeyPicker", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
    // 账号列表是模块级缓存：用例之间必须清掉，避免串味
    invalidateSsyAccountsCache();
  });

  it("未登录时不出现触发器", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    stubSsyKeyCommands({ accounts: [] });

    renderPicker();

    await waitFor(() => {
      expect(
        fetchSpy.mock.calls.some(([url]) =>
          String(url).includes("shengsuanyun_list_accounts"),
        ),
      ).toBe(true);
    });
    expect(
      screen.queryByRole("button", { name: "选择胜算云 API Key" }),
    ).not.toBeInTheDocument();
    fetchSpy.mockRestore();
  });

  it("列出账号下全部 Key，且只取被选中那把的明文", async () => {
    const { setBindingCalls, revealCalls } = stubSsyKeyCommands();
    const { onSelect } = renderPicker();

    await openPanel();

    expect(await screen.findByText("熊叔")).toBeInTheDocument();
    expect(screen.getByText("余额 ¥12.50")).toBeInTheDocument();
    expect(screen.getByText("sk-def……cdef")).toBeInTheDocument();
    expect(screen.getByText("sk-bea……beef")).toBeInTheDocument();
    expect(screen.getByText("已用 ¥226.43 / 无上限")).toBeInTheDocument();
    expect(screen.getByText("已用 ¥0.00 / ¥100.00")).toBeInTheDocument();

    // 打开面板只拿脱敏元数据，明文一次都不取
    expect(revealCalls).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: /Bear Xiong/ }));

    await waitFor(() => {
      expect(onSelect).toHaveBeenCalledWith("sk-bear-plaintext", 83949);
    });
    expect(revealCalls).toEqual([{ accountId: "acct-1", keyId: 83949 }]);
    await waitFor(() => {
      expect(setBindingCalls).toEqual([
        {
          appType: "claude",
          providerId: "p1",
          accountId: "acct-1",
          keyId: 83949,
        },
      ]);
    });
  });

  it("已禁用 / 已过期的 Key 可见但不可选", async () => {
    const { setBindingCalls } = stubSsyKeyCommands({
      keys: [ssyBannedKeyFixture],
    });
    const { onSelect } = renderPicker();

    await openPanel();

    const banned = await screen.findByRole("button", { name: /Revoked/ });
    expect(banned).toBeDisabled();
    expect(screen.getByText("已禁用")).toBeInTheDocument();

    fireEvent.click(banned);
    expect(onSelect).not.toHaveBeenCalled();
    expect(setBindingCalls).toHaveLength(0);
  });

  it("新增供应商（无 providerId）时只回填 Key，不写绑定", async () => {
    const { setBindingCalls } = stubSsyKeyCommands();
    const { onSelect } = renderPicker({ providerId: undefined });

    await openPanel();
    expect(screen.getByText("（保存后写入供应商配置）")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: /Bear Xiong/ }));

    await waitFor(() => {
      expect(onSelect).toHaveBeenCalledWith("sk-bear-plaintext", 83949);
    });
    expect(setBindingCalls).toHaveLength(0);
  });
});
