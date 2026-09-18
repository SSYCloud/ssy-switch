import { describe, expect, it } from "vitest";
import { geminiProviderPresets } from "../../src/config/geminiProviderPresets";

describe("geminiProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(geminiProviderPresets.map((p) => p.name)).toEqual(["Shengsuanyun", "Google Official"]);
  });

  it("胜算云是首个预设", () => {
    expect(geminiProviderPresets[0]?.name).toBe("Shengsuanyun");
  });

  it("胜算云预设包含路由地址", () => {
    const cfg = JSON.stringify(geminiProviderPresets[0]?.settingsConfig ?? {});
    expect(cfg).toContain("router.shengsuanyun.com");
  });
});
