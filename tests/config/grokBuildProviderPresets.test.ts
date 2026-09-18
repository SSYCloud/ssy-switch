import { describe, expect, it } from "vitest";
import { grokBuildProviderPresets } from "../../src/config/grokBuildProviderPresets";

describe("grokBuildProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(grokBuildProviderPresets.map((p) => p.name)).toEqual(["Shengsuanyun"]);
  });

  it("胜算云是首个预设", () => {
    expect(grokBuildProviderPresets[0]?.name).toBe("Shengsuanyun");
  });

});
