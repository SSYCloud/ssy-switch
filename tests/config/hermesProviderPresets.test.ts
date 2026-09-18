import { describe, expect, it } from "vitest";
import { hermesProviderPresets } from "../../src/config/hermesProviderPresets";

describe("hermesProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(hermesProviderPresets.map((p) => p.name)).toEqual(["Shengsuanyun", "Nous Research"]);
  });

  it("胜算云是首个预设", () => {
    expect(hermesProviderPresets[0]?.name).toBe("Shengsuanyun");
  });

  it("胜算云预设包含路由地址", () => {
    const cfg = JSON.stringify(hermesProviderPresets[0]?.settingsConfig ?? {});
    expect(cfg).toContain("router.shengsuanyun.com");
  });
});
