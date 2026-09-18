import { describe, expect, it } from "vitest";
import { claudeDesktopProviderPresets } from "../../src/config/claudeDesktopProviderPresets";

describe("claudeDesktopProviderPresets 精选预设", () => {
  it("只包含胜算云与原厂官方入口", () => {
    expect(claudeDesktopProviderPresets.map((p) => p.name)).toEqual(["Claude Desktop Official", "Shengsuanyun", "GitHub Copilot", "Codex", "xAI (Grok)"]);
  });

  it("胜算云在精选清单中", () => {
    expect(claudeDesktopProviderPresets.some((p) => p.name === "Shengsuanyun")).toBe(true);
  });

});
