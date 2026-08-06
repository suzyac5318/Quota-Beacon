import { describe, expect, it } from "vitest";
// @ts-expect-error Vitest runs in Node; the production bundle does not include this test module.
import { readFileSync } from "node:fs";
import tauriConfig from "../src-tauri/tauri.conf.json";
import cargoManifest from "../src-tauri/Cargo.toml?raw";
import nativeMaterial from "../src-tauri/src/window_material.rs?raw";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

type TransparentWindowConfig = {
  label: string;
  transparent?: boolean;
  backgroundColor?: string;
};

describe("Windows glass material", () => {
  it("keeps every native material window fully transparent", () => {
    const windows = tauriConfig.app.windows as TransparentWindowConfig[];
    expect(windows.map((window) => window.label)).toEqual(["widget", "palette", "palette-editor"]);
    expect(windows.every((window) => window.transparent && window.backgroundColor === "#00000000")).toBe(true);
  });

  it("uses Windows 11 Desktop Acrylic without the legacy acrylic crate", () => {
    expect(cargoManifest).toContain('"Win32_Graphics_Dwm"');
    expect(cargoManifest).not.toContain("window-vibrancy");
    expect(nativeMaterial).toContain("DWMSBT_TRANSIENTWINDOW");
    expect(nativeMaterial).toContain("CreateRoundRectRgn");
    expect(nativeMaterial).toContain("CLIP_GENERATION");
    expect(nativeMaterial).toContain('for label in ["widget", "palette", "palette-editor"]');
    expect(nativeMaterial).toContain('#[cfg(target_os = "windows")]');
  });

  it("defines one glass system and accessibility fallbacks", () => {
    expect(styles).toContain("--glass-card-surface");
    expect(styles).toContain("--glass-orb-surface");
    expect(styles).toContain("--glass-control-surface");
    expect(styles).toContain("@media (prefers-reduced-transparency: reduce)");
    expect(styles).toContain("@media (prefers-contrast: more)");
    expect(styles).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
