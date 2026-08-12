import { describe, expect, it } from "vitest";
import tauriConfig from "../src-tauri/tauri.conf.json";
import cargoManifest from "../src-tauri/Cargo.toml?raw";
import cargoLock from "../src-tauri/Cargo.lock?raw";
import packageJson from "../package.json";
import packageLock from "../package-lock.json";
import versionFile from "../VERSION?raw";
import ciWorkflow from "../.github/workflows/ci.yml?raw";
import releaseWorkflow from "../.github/workflows/release.yml?raw";
import bundleVerifier from "../.github/scripts/verify-macos-bundle.sh?raw";

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

describe("macOS release isolation", () => {
  it("only creates releases from Mac version tags", () => {
    expect(releaseWorkflow).toMatch(
      /on:\r?\n\s+push:\r?\n\s+tags:\r?\n\s+- "macos-v\*"/,
    );
    expect(releaseWorkflow).not.toMatch(/\r?\n\s+branches:/);
    expect(releaseWorkflow).not.toContain("pull_request:");
  });

  it("builds and verifies only a macOS Universal bundle", () => {
    for (const workflow of [ciWorkflow, releaseWorkflow]) {
      expect(workflow).toContain("runs-on: macos-latest");
      expect(workflow).toContain(
        "rustup target add aarch64-apple-darwin x86_64-apple-darwin",
      );
      expect(workflow).toContain("build --target universal-apple-darwin");
      expect(workflow).toContain(".github/scripts/verify-macos-bundle.sh");
    }

    expect(releaseWorkflow).not.toContain("windows-latest");
    expect(releaseWorkflow).not.toContain("quota-beacon-windows");
    expect(releaseWorkflow).not.toMatch(/\.(exe|msi|msix)\b/i);
  });

  it("keeps Mac release assets isolated and the release in draft", () => {
    expect(releaseWorkflow).toContain("quota-beacon-macos-universal-ad-hoc.zip");
    expect(releaseWorkflow).toContain("bundle/dmg/*.dmg");
    expect(releaseWorkflow).toContain("quota-beacon-macos-universal-ad-hoc.sha256");
    expect(releaseWorkflow).toContain("draft: true");
  });

  it("verifies ad-hoc signing, both architectures, DMG integrity, and SHA-256", () => {
    expect(bundleVerifier).toContain("codesign --verify --deep --strict");
    expect(bundleVerifier).toContain("grep -q '^Signature=adhoc$'");
    expect(bundleVerifier).toContain("lipo -archs");
    expect(bundleVerifier).toContain("grep -qw 'arm64'");
    expect(bundleVerifier).toContain("grep -qw 'x86_64'");
    expect(bundleVerifier).toContain("hdiutil verify");
    expect(bundleVerifier).toContain("shasum -a 256");
  });
});
