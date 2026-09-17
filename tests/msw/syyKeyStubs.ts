// 胜算云 Key 选择器的 MSW 桩：5 个 Tauri 命令的固定响应 + 调用记录。
import { http, HttpResponse } from "msw";
import { server } from "./server";

const TAURI_ENDPOINT = "http://tauri.local";

export const ssyAccountFixture = {
  id: "acct-1",
  uidMasked: "628***",
  displayName: "熊叔",
  emailMasked: "b***@shengsuanyun.com",
  avatarUrl: "",
  isCreator: true,
  balanceYuan: 12.5,
  balanceUpdatedAt: 1,
  createdAt: 1,
};

export const ssyDefaultKeyFixture = {
  id: 68545,
  name: "default",
  tokenMasked: "sk-def……cdef",
  desc: "",
  isDefault: true,
  isBanned: false,
  isExpired: false,
  selectable: true,
  maxQuotaYuan: null,
  consumedYuan: 226.4263,
  createdAt: 1,
  expiresAt: 0,
  supportedModels: ["anthropic/claude-opus-5"],
};

export const ssyBearKeyFixture = {
  id: 83949,
  name: "Bear Xiong",
  tokenMasked: "sk-bea……beef",
  desc: "",
  isDefault: false,
  isBanned: false,
  isExpired: false,
  selectable: true,
  maxQuotaYuan: 100,
  consumedYuan: 0,
  createdAt: 2,
  expiresAt: 0,
  supportedModels: [],
};

export const ssyBannedKeyFixture = {
  ...ssyBearKeyFixture,
  id: 90001,
  name: "Revoked",
  isBanned: true,
  selectable: false,
};

type StubOptions = {
  accounts?: unknown[];
  keys?: unknown[];
  binding?: unknown;
  plaintext?: string;
};

export const stubSsyKeyCommands = (options: StubOptions = {}) => {
  const setBindingCalls: Record<string, unknown>[] = [];
  const revealCalls: Record<string, unknown>[] = [];

  server.use(
    http.post(`${TAURI_ENDPOINT}/shengsuanyun_list_accounts`, () =>
      HttpResponse.json(options.accounts ?? [ssyAccountFixture]),
    ),
    http.post(`${TAURI_ENDPOINT}/shengsuanyun_list_keys`, () =>
      HttpResponse.json(
        options.keys ?? [ssyDefaultKeyFixture, ssyBearKeyFixture],
      ),
    ),
    http.post(
      `${TAURI_ENDPOINT}/shengsuanyun_reveal_key`,
      async ({ request }) => {
        revealCalls.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(options.plaintext ?? "sk-bear-plaintext");
      },
    ),
    http.post(`${TAURI_ENDPOINT}/shengsuanyun_get_binding`, () =>
      HttpResponse.json(options.binding ?? null),
    ),
    http.post(
      `${TAURI_ENDPOINT}/shengsuanyun_set_binding_key`,
      async ({ request }) => {
        setBindingCalls.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(true);
      },
    ),
  );

  return { setBindingCalls, revealCalls };
};
