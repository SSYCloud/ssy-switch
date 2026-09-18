import { describe, expect, it } from "vitest";
import { openclawProviderPresets } from "../../src/config/openclawProviderPresets";

describe("openclawProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(openclawProviderPresets.map((p) => p.name)).toEqual(["Shengsuanyun"]);
  });

  it("胜算云是首个预设", () => {
    expect(openclawProviderPresets[0]?.name).toBe("Shengsuanyun");
  });

  it("胜算云预设包含路由地址", () => {
    const cfg = JSON.stringify(openclawProviderPresets[0]?.settingsConfig ?? {});
    expect(cfg).toContain("router.shengsuanyun.com");
  });
});
