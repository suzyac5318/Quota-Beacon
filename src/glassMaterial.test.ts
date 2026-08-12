import { describe, expect, it } from "vitest";
// @ts-expect-error Vitest runs in Node; the production bundle does not include this test module.
import { readFileSync } from "node:fs";
import tauriConfig from "../src-tauri/tauri.conf.json";
import capabilities from "../src-tauri/capabilities/default.json";
import cargoManifest from "../src-tauri/Cargo.toml?raw";
import frontendApp from "./App.tsx?raw";
import frontendBridge from "./lib/bridge.ts?raw";
import nativeApp from "../src-tauri/src/lib.rs?raw";
import nativeMaterial from "../src-tauri/src/window_material.rs?raw";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

type TransparentWindowConfig = {
  label: string;
  height?: number;
  transparent?: boolean;
  backgroundColor?: string;
};

describe("Windows glass material", () => {
  it("keeps every native material window fully transparent", () => {
    const windows = tauriConfig.app.windows as TransparentWindowConfig[];
    expect(windows.map((window) => window.label)).toEqual(["widget", "palette", "palette-editor", "account-switcher"]);
    expect(windows.every((window) => window.transparent && window.backgroundColor === "#00000000")).toBe(true);
    expect(windows.find((window) => window.label === "account-switcher")?.height).toBe(190);
    expect(capabilities.windows).toContain("account-switcher");
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
    expect(nativeMaterial).toContain("BLUR_AMOUNT: f32 = 4.5");
    expect(nativeMaterial).toContain("BLUR_EDGE_INSET: f32 = 1.0");
    expect(nativeMaterial).toContain("CONTROL_RADIUS: f32 = 24.0");
    expect(nativeMaterial).toContain("CreateRoundedRectangleGeometry");
    expect(nativeMaterial).toContain("CreateGeometricClipWithGeometry");
    expect(nativeMaterial).toContain("SetCornerRadius");
    expect(nativeMaterial).toContain("compact_blur_stays_inside_css_border");
    expect(nativeMaterial).toContain("expanded_blur_stays_inside_css_border");
    expect(nativeMaterial).toContain("control_blur_stays_inside_internal_stroke");
    expect(nativeMaterial).toContain("compact_surface_stays_eighty_pixels_after_parent_expands");
    expect(nativeMaterial).toContain("native_morph_curve_matches_css_keyframes");
    expect(nativeMaterial).toContain("compact_surface_tracks_webview_pixel_scale_without_corner_overhang");
    expect(nativeMaterial).toContain("WEBVIEW_SCALE_BITS");
    expect(nativeMaterial).toContain("BLUR_WINDOWS");
    expect(nativeMaterial).toContain('(\"palette\", BlurWindowKind::Palette)');
    expect(nativeMaterial).toContain('(\"palette-editor\", BlurWindowKind::PaletteEditor)');
    expect(nativeMaterial).toContain('(\"account-switcher\", BlurWindowKind::Control)');
    expect(nativeMaterial).toContain("animate_palette_material");
    expect(nativeMaterial).toContain("PALETTE_CLOSE_DELAY_MS: u64 = 55");
    expect(nativeMaterial).toContain("EDITOR_OPEN_DELAY_MS: u64 = 60");
    expect(nativeMaterial).toContain("SetOpacity");
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
    expect(frontendBridge).toContain("getCurrentWindow().innerSize()");
    expect(frontendBridge).not.toContain("getCurrentWindow().outerSize()");
    expect(frontendBridge).toContain("physicalSize.width / cssViewportWidth");
    expect(nativeApp).toContain('position_palette_windows(&app)?;\n    window_material::sync_window_material(&app);');
    expect(nativeApp).toContain("window_material::animate_palette_material(app.clone(), true)");
    expect(nativeApp).toContain("window_material::animate_palette_material(app.clone(), false)");
    expect(nativeApp).toContain("Duration::from_millis(250)");
    expect(nativeApp).toContain('[\"widget\", \"palette\", \"palette-editor\", \"account-switcher\"].contains(&window.label())');
    expect(nativeApp).toContain('emit_to(\"account-switcher\", \"account-switcher-opened\", theme)');
    expect(nativeApp).toContain('\"account-switcher-theme-changed\"');
    expect(nativeApp).toContain("LogicalSize::new(320.0, 190.0)");
    expect(nativeApp).toContain('window.label() == \"account-switcher\" && matches!(event, WindowEvent::Resized(_))');
    expect(frontendBridge).toContain("ACCOUNT_SWITCHER_COMPACT_HEIGHT = 190");
    expect(frontendBridge).toContain("ACCOUNT_SWITCHER_EXPANDED_HEIGHT = 240");
    expect(frontendBridge).toContain("ACCOUNT_SWITCHER_NOTICE_EXTRA_HEIGHT = 24");
    expect(frontendBridge).toContain("accountSwitcherResizeGeneration");
  });

  it("keeps the more transparent glass colors and accessibility fallbacks", () => {
    expect(styles).toContain("--glass-card-surface: rgba(255,255,255,.32)");
    expect(styles).toContain("--glass-orb-surface: rgba(255,255,255,.26)");
    expect(styles).toContain("--glass-control-surface: rgba(255,255,255,.34)");
    expect(styles).toContain("@media (prefers-reduced-transparency: reduce)");
    expect(styles).toContain("@media (prefers-contrast: more)");
    expect(styles).toContain("@media (prefers-reduced-motion: reduce)");
  });

  it("does not paint an outer shadow into the transparent widget corners", () => {
    expect(styles).not.toContain("0 10px 32px rgba(37,45,60,.12)");
    expect(styles).not.toContain("0 5px 16px rgba(37,45,60,.1)");
    expect(styles).not.toContain("0 8px 22px rgba(72,88,112,.15)");
    expect(styles).not.toContain("0 4px 14px rgba(72,88,112,.08)");
    expect(styles).not.toContain("0 8px 24px rgba(37,45,60,.1)");
  });

  it("keeps account actions aligned while hiding only the scrollbar chrome", () => {
    expect(styles).toContain(".account-icon-button--rename { grid-column: 6; }");
    expect(styles).toContain(".account-icon-button--delete { grid-column: 7; }");
    expect(styles).toContain("overflow-y: auto; scrollbar-width: none; -ms-overflow-style: none;");
    expect(styles).toContain(".account-switcher__body::-webkit-scrollbar { display: none; width: 0; height: 0; }");
    expect(styles).toContain(".account-switcher__form-shell { min-height: 0; display: grid; grid-template-rows: 0fr;");
    expect(styles).toContain(".account-switcher__form-shell--open { grid-template-rows: 1fr;");
    expect(styles).not.toContain(".account-restart-button");
    expect(styles).toContain("grid-template-rows: auto minmax(0,1fr) auto auto; gap: 0;");
    expect(styles).toContain(".account-switcher__header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; margin-bottom: 9px;");
    expect(styles).toContain(".account-switcher__form-shell--open { grid-template-rows: 1fr; margin-top: 9px;");
    expect(styles).toContain("linear-gradient(var(--glass-control-tint");
    expect(styles).toContain(".account-switcher--neutral { --glass-control-tint: rgba(237,243,248,.18); }");
    expect(styles).toContain(".account-row--active { border-color: color-mix(in srgb, var(--card-base");
    expect(styles).not.toContain("rgba(226,235,243,.84)");
  });

  it("draws one internal stroke for every glass card", () => {
    expect(styles).toContain("--glass-stroke-width: .5px");
    expect(styles.match(/box-shadow: inset 0 0 0 var\(--glass-stroke-width\) var\(--glass-border\)/g)?.length).toBeGreaterThanOrEqual(5);
    expect(styles).not.toContain("inset 0 0 0 1px var(--glass-border)");
    expect(styles).not.toContain("box-shadow: inset 0 1px 0 var(--glass-highlight)");
    expect(styles).toContain(".palette-preview { width: 100%; height: 100%; overflow: hidden; padding: 15px 18px 14px; border: 1px solid transparent;");
    expect(styles).toContain(".palette-editor { width: 100%; height: 100%; overflow: hidden; padding: 11px 16px 10px; border: 1px solid transparent;");
    expect(styles.match(/backdrop-filter: blur\(13\.5px\) saturate\(1\.18\)/g)?.length).toBeGreaterThanOrEqual(3);
    expect(styles.match(/backdrop-filter: blur\(12px\) saturate\(1\.18\)/g)?.length).toBeGreaterThanOrEqual(2);
  });
});
