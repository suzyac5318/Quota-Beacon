import { describe, expect, it } from "vitest";
import {
  DEFAULT_PALETTE_COLORS,
  PALETTE_PERCENTAGES,
  paletteGradient,
  paletteValidationError,
  quotaThemeStyle,
} from "./quotaTheme";

describe("quotaThemeStyle", () => {
  it("uses five fixed color milestones", () => {
    PALETTE_PERCENTAGES.forEach((percent, index) => {
      expect(quotaThemeStyle(percent)["--card-base"]).toBe(DEFAULT_PALETTE_COLORS[index]);
    });
  });

  it("creates a distinct background for every integer percentage", () => {
    const colors = Array.from({ length: 101 }, (_, percent) => quotaThemeStyle(percent)["--card-base"]);
    expect(new Set(colors)).toHaveLength(101);
  });

  it("uses custom colors at every fixed milestone", () => {
    const custom = ["#d94f70", "#ef8a4c", "#f4d35e", "#76c893", "#4d96ff"];
    PALETTE_PERCENTAGES.forEach((percent, index) => {
      expect(quotaThemeStyle(percent, custom)["--card-base"]).toBe(custom[index]);
    });
    expect(paletteGradient(custom)).toContain("#76c893 60%");
  });

  it("rejects palettes whose interpolation produces duplicate states", () => {
    expect(paletteValidationError(["#000000", "#000001", "#000002", "#000003", "#000004"])).not.toBeNull();
  });

  it("keeps foreground legible independently of the quota tint", () => {
    const dark = ["#101820", "#182330", "#203050", "#293f66", "#36557d"];
    expect(quotaThemeStyle(50, dark)["--card-foreground"]).toBe("#17191f");
    expect(quotaThemeStyle(50)["--card-foreground"]).toBe("#17191f");
  });

  it("separates glass tint weights from the opaque quota color", () => {
    const style = quotaThemeStyle(60);
    expect(style["--glass-card-tint"]).toBe(`color-mix(in srgb, ${style["--card-base"]} 16%, transparent)`);
    expect(style["--glass-orb-tint"]).toBe(`color-mix(in srgb, ${style["--card-base"]} 13%, transparent)`);
    expect(style["--glass-control-tint"]).toBe(`color-mix(in srgb, ${style["--card-base"]} 16%, transparent)`);
  });

  it("clamps out-of-range values", () => {
    expect(quotaThemeStyle(101)["--card-base"]).toBe(quotaThemeStyle(100)["--card-base"]);
    expect(quotaThemeStyle(-1)["--card-base"]).toBe(quotaThemeStyle(0)["--card-base"]);
  });
});
