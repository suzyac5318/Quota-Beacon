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

  it("keeps rectangular native window material disabled", () => {
    expect(cargoManifest).not.toContain('"Win32_Graphics_Dwm"');
    expect(cargoManifest).not.toContain('"Win32_Graphics_Gdi"');
    expect(cargoManifest).not.toContain("window-vibrancy");
    expect(nativeMaterial).not.toContain("DwmSetWindowAttribute");
    expect(nativeMaterial).not.toContain("SetWindowRgn");
    expect(nativeMaterial).toContain("pub fn apply_window_materials(_app: &AppHandle) {}");
    expect(nativeMaterial).toContain("pub fn animate_widget_region(_app: AppHandle, _expanded: bool) {}");
  });

  it("defines one glass system and accessibility fallbacks", () => {
    expect(styles).toContain("--glass-card-surface");
    expect(styles).toContain("--glass-orb-surface");
    expect(styles).toContain("--glass-control-surface");
    expect(styles).toContain("@media (prefers-reduced-transparency: reduce)");
    expect(styles).toContain("@media (prefers-contrast: more)");
    expect(styles).toContain("@media (prefers-reduced-motion: reduce)");
  });

  it("does not paint an outer shadow into the transparent widget corners", () => {
    expect(styles).not.toContain("0 10px 32px rgba(37,45,60,.12)");
    expect(styles).not.toContain("0 5px 16px rgba(37,45,60,.1)");
    expect(styles).not.toContain("0 8px 22px rgba(72,88,112,.15)");
    expect(styles).not.toContain("0 4px 14px rgba(72,88,112,.08)");
  });
});
