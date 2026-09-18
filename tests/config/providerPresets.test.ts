import { describe, expect, it } from "vitest";
import { providerPresets } from "../../src/config/claudeProviderPresets";

describe("providerPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(providerPresets.map((p) => p.name)).toEqual(["Shengsuanyun", "Claude Official", "Gemini Native", "OpenCode Go", "GitHub Copilot", "Codex", "xAI (Grok)"]);
  });

  it("胜算云是首个预设", () => {
    expect(providerPresets[0]?.name).toBe("Shengsuanyun");
  });

  it("胜算云预设包含路由地址", () => {
    const cfg = JSON.stringify(providerPresets[0]?.settingsConfig ?? {});
    expect(cfg).toContain("router.shengsuanyun.com");
  });
});
