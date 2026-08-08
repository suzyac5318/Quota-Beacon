import { describe, expect, it } from "vitest";
// @ts-expect-error Vitest runs in Node; the production bundle does not include this test module.
import { readFileSync } from "node:fs";
import tauriConfig from "../src-tauri/tauri.conf.json";
import cargoManifest from "../src-tauri/Cargo.toml?raw";
import frontendApp from "./App.tsx?raw";
import frontendBridge from "./lib/bridge.ts?raw";
import nativeApp from "../src-tauri/src/lib.rs?raw";
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

  it("enables Win32 host backdrop before creating the clipped blur brush", () => {
    expect(cargoManifest).toContain('"UI_Composition"');
    expect(cargoManifest).toContain('"Win32_Graphics_Dwm"');
    expect(cargoManifest).not.toContain("window-vibrancy");
    expect(nativeMaterial).toContain("DWMWA_USE_HOSTBACKDROPBRUSH");
    expect(nativeMaterial).toContain("DwmGetWindowAttribute");
    expect(nativeMaterial).toContain("CreateHostBackdropBrush");
    expect(nativeMaterial.indexOf("enable_host_backdrop(surface)?;")).toBeLessThan(
      nativeMaterial.indexOf("CreateHostBackdropBrush"),
    );
    expect(nativeMaterial).toContain("BLUR_AMOUNT: f32 = 8.0");
    expect(nativeMaterial).toContain("CreateRoundedRectangleGeometry");
    expect(nativeMaterial).toContain("CreateGeometricClipWithGeometry");
    expect(nativeMaterial).toContain("SetCornerRadius");
    expect(nativeMaterial).toContain("compact_surface_uses_one_ten_pixel_inset");
    expect(nativeMaterial).toContain("compact_surface_stays_eighty_pixels_after_parent_expands");
    expect(nativeMaterial).toContain("native_morph_curve_matches_css_keyframes");
    expect(nativeMaterial).toContain("compact_surface_tracks_webview_pixel_scale_without_corner_overhang");
    expect(nativeMaterial).toContain("WEBVIEW_SCALE_BITS");
    expect(nativeMaterial).toContain("EXPAND_MORPH_MS: u64 = 400");
    expect(nativeMaterial).toContain("COLLAPSE_MORPH_DELAY_MS: u64 = 90");
    expect(nativeMaterial).toContain("COLLAPSE_MORPH_MS: u64 = 190");
    expect(nativeMaterial).toContain("(0.60, 1.018)");
    expect(nativeMaterial).toContain("blur surface geometry mismatch");
    expect(nativeMaterial).toContain("ShowWindow(surface, SW_HIDE)");
    expect(nativeMaterial).not.toContain("DWMSBT_TRANSIENTWINDOW");
    expect(nativeMaterial).not.toContain("SetWindowCompositionAttribute");
    expect(nativeMaterial).not.toContain("GLASS_TINT");
  });

  it("keeps the blur surface synchronized with widget lifecycle events", () => {
    expect(nativeApp).toContain("window_material::sync_window_material");
    expect(nativeApp).toContain("WindowEvent::ScaleFactorChanged { .. }");
    expect(nativeApp).toContain("window_material::hide_window_material()");
    expect(nativeApp).toContain("window_material::destroy_window_materials()");
    expect(nativeApp).toContain("set_widget_css_scale");
    expect(frontendApp).toContain("syncWidgetCssScale(window.innerWidth)");
    expect(frontendApp).toContain('window.visualViewport?.addEventListener("resize", syncScale)');
    expect(frontendBridge).toContain("physicalSize.width / cssViewportWidth");
  });

  it("retains the v1.6.1 glass colors and accessibility fallbacks", () => {
    expect(styles).toContain("--glass-card-surface: rgba(255,255,255,.38)");
    expect(styles).toContain("--glass-orb-surface: rgba(255,255,255,.3)");
    expect(styles).toContain("--glass-control-surface: rgba(255,255,255,.42)");
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
