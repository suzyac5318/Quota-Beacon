import { describe, expect, it } from "vitest";
import tauriConfig from "../src-tauri/tauri.conf.json";
import cargoManifest from "../src-tauri/Cargo.toml?raw";
import cargoLock from "../src-tauri/Cargo.lock?raw";
import packageJson from "../package.json";
import packageLock from "../package-lock.json";
import versionFile from "../VERSION?raw";
import accountVaultSource from "../src-tauri/src/account_vault.rs?raw";

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
    expect(tauriConfig.bundle.macOS.signingIdentity).toBe("-");
  });

  it("keeps every transparent window background fully transparent", () => {
    expect(transparentWindows.map((window) => window.label)).toEqual([
      "widget",
      "palette",
      "palette-editor",
      "account-switcher",
    ]);
    expect(transparentWindows.every((window) => window.backgroundColor === "#00000000")).toBe(true);
  });
});

describe("macOS version isolation", () => {
  it("keeps every Mac release version source in sync", () => {
    const cargoVersion = cargoManifest.match(/^version = "([^"]+)"/m)?.[1];
    const cargoLockVersion = cargoLock.match(
      /name = "quota-beacon"\r?\nversion = "([^"]+)"/,
    )?.[1];

    expect(versionFile.trim()).toBe(packageJson.version);
    expect(packageLock.version).toBe(packageJson.version);
    expect(packageLock.packages[""].version).toBe(packageJson.version);
    expect(tauriConfig.version).toBe(packageJson.version);
    expect(cargoVersion).toBe(packageJson.version);
    expect(cargoLockVersion).toBe(packageJson.version);
  });
});

describe("macOS account credential isolation", () => {
  it("uses Security.framework and a stable app-scoped Keychain service", () => {
    expect(cargoManifest).toContain("[target.'cfg(target_os = \"macos\")'.dependencies]");
    expect(cargoManifest).toContain('security-framework = "3.7"');
    expect(accountVaultSource).toContain('KEYCHAIN_SERVICE: &str = "app.quotabeacon.desktop.accounts"');
    expect(accountVaultSource).not.toContain("dpapi");
  });
});
