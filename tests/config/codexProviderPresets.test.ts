import { describe, expect, it } from "vitest";
import { codexProviderPresets } from "../../src/config/codexProviderPresets";

describe("codexProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(codexProviderPresets.map((p) => p.name)).toEqual(["Shengsuanyun", "OpenAI Official", "xAI (Grok) OAuth"]);
  });

  it("胜算云是首个预设", () => {
    expect(codexProviderPresets[0]?.name).toBe("Shengsuanyun");
  });

  it("胜算云预设包含路由地址", () => {
    const cfg = JSON.stringify(codexProviderPresets[0] ?? {});
    expect(cfg.includes("router.shengsuanyun.com") || cfg.includes("shengsuanyun")).toBe(true);
  });
});
