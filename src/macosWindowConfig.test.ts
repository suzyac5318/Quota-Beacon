import { describe, expect, it } from "vitest";
import tauriConfig from "../src-tauri/tauri.conf.json";
import cargoManifest from "../src-tauri/Cargo.toml?raw";
import releaseWorkflow from "../.github/workflows/release.yml?raw";

type TransparentWindowConfig = {
  label: string;
  transparent?: boolean;
  backgroundColor?: string;
};

describe("macOS transparent windows", () => {
  const windows = tauriConfig.app.windows as TransparentWindowConfig[];
  const transparentWindows = windows.filter((window) => window.transparent);

  it("enables the Tauri private API required for transparent WKWebView windows", () => {
    expect(tauriConfig.app.macOSPrivateApi).toBe(true);
    expect(cargoManifest).toContain('features = ["macos-private-api", "tray-icon"]');
  });

  it("keeps every transparent window background fully transparent", () => {
    expect(transparentWindows.map((window) => window.label)).toEqual([
      "widget",
      "palette",
      "palette-editor",
    ]);
    expect(transparentWindows.every((window) => window.backgroundColor === "#00000000")).toBe(true);
  });
});

describe("macOS release isolation", () => {
  it("uses Mac-only tags, runners, and artifacts", () => {
    expect(releaseWorkflow).toContain('      - "macos-v*"');
    expect(releaseWorkflow).toContain("runs-on: macos-latest");
    expect(releaseWorkflow).not.toContain("windows-latest");
    expect(releaseWorkflow).not.toContain("quota-beacon-windows");
  });
});
