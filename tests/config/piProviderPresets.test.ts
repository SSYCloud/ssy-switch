import { describe, expect, it } from "vitest";
import { piProviderPresets } from "../../src/config/piProviderPresets";

describe("piProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(piProviderPresets.map((p) => p.name)).toEqual(["Shengsuanyun"]);
  });

  it("胜算云是首个预设", () => {
    expect(piProviderPresets[0]?.name).toBe("Shengsuanyun");
  });

  it("胜算云预设包含路由地址", () => {
    const cfg = JSON.stringify(piProviderPresets[0]?.settingsConfig ?? {});
    expect(cfg).toContain("router.shengsuanyun.com");
  });
});
