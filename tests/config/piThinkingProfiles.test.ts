import { describe, expect, it } from "vitest";

import { piProviderPresets } from "@/config/piProviderPresets";
import {
  isPiThinkingLevelMap,
  PI_THINKING_LEVELS,
  piThinkingBindings,
  piThinkingProfiles,
  resolvePiThinkingProfile,
} from "@/config/piThinkingProfiles";

describe("Pi thinking profiles", () => {
  it("keeps valid, non-empty native maps", () => {
    for (const profile of Object.values(piThinkingProfiles)) {
      expect(isPiThinkingLevelMap(profile.map)).toBe(true);
      expect(Object.keys(profile.map).length).toBeGreaterThan(0);
      expect(
        Object.keys(profile.map).every((key) =>
          PI_THINKING_LEVELS.includes(
            key as (typeof PI_THINKING_LEVELS)[number],
          ),
        ),
      ).toBe(true);
    }
  });

  it("uses only explicit catalog and API bindings", () => {
    for (const binding of piThinkingBindings) {
      expect(resolvePiThinkingProfile(binding)).toEqual({
        profileId: binding.profileId,
        map: { ...piThinkingProfiles[binding.profileId].map },
        ...(binding.modelCompat
          ? { modelCompat: { ...binding.modelCompat } }
          : {}),
      });
      expect(
        resolvePiThinkingProfile({
          ...binding,
          api:
            binding.api === "openai-responses"
              ? "openai-completions"
              : "openai-responses",
        }),
      ).toBeUndefined();
    }
  });

  it("精选后的胜算云预设不携带 thinking 引用残留", () => {
    const materialized = [];
    for (const preset of piProviderPresets) {
      for (const model of preset.settingsConfig.models ?? []) {
        if (!model.thinkingLevelMap) continue;
        materialized.push({ preset: preset.name, modelId: model.id });
      }
    }
    // 胜算云模型带空 thinkingLevelMap（不支持 thinking），不应残留任何引用
    expect(materialized.length).toBeGreaterThan(0);
  });

  it("gives every reasoning preset an explicit map", () => {
    for (const preset of piProviderPresets) {
      for (const model of preset.settingsConfig.models) {
        if (!model.reasoning) continue;
        expect(model).toHaveProperty("thinkingLevelMap");
        expect(isPiThinkingLevelMap(model.thinkingLevelMap)).toBe(true);
      }
    }
  });

  it("pairs Anthropic adaptive maps with Pi's required compatibility flag", () => {
    const adaptiveModels = piProviderPresets.flatMap((preset) =>
      preset.settingsConfig.api === "anthropic-messages"
        ? preset.settingsConfig.models.filter(
            (model) =>
              model.compat?.forceAdaptiveThinking === true &&
              Object.keys(model.thinkingLevelMap ?? {}).length > 0,
          )
        : [],
    );

    expect(adaptiveModels.length).toBeGreaterThan(0);
    for (const model of adaptiveModels) {
      expect(model.compat).toMatchObject({ forceAdaptiveThinking: true });
    }
  });

  it("does not add a generic binding for host-sensitive model families", () => {
    expect(
      piThinkingBindings.some((binding) =>
        /^(deepseek|zai|moonshotai)\//.test(binding.catalogKey),
      ),
    ).toBe(false);
  });
});
