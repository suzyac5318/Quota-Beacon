import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");

function fail(message) {
  console.error(`Product-line preflight failed: ${message}`);
  process.exit(1);
}

function readText(relativePath) {
  return readFileSync(join(repositoryRoot, relativePath), "utf8");
}

function git(...args) {
  return execFileSync("git", ["-C", repositoryRoot, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function argumentValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const identityPath = join(repositoryRoot, "PRODUCT_LINE.json");
if (!existsSync(identityPath)) {
  fail("PRODUCT_LINE.json is missing.");
}

const identity = JSON.parse(readFileSync(identityPath, "utf8"));
const expectedProductLine = argumentValue("--expect");
if (expectedProductLine && identity.productLine !== expectedProductLine) {
  fail(`expected ${expectedProductLine}, but this worktree declares ${identity.productLine}.`);
}

const gitRoot = resolve(git("rev-parse", "--show-toplevel"));
if (gitRoot.toLowerCase() !== repositoryRoot.toLowerCase()) {
  fail(`script root ${repositoryRoot} does not match Git root ${gitRoot}.`);
}

const branch = git("branch", "--show-current");
const allowedPrefixes = identity.allowedBranchPrefixes ?? [];
const branchAllowed =
  branch === identity.expectedBranch ||
  allowedPrefixes.some((prefix) => branch.startsWith(prefix));
if (!branchAllowed) {
  fail(
    `branch ${branch || "<detached>"} is not ${identity.expectedBranch} or an allowed ${identity.productLine} work branch.`,
  );
}

const version = readText("VERSION").trim();
const packageVersion = JSON.parse(readText("package.json")).version;
const packageLockVersion = JSON.parse(readText("package-lock.json")).packages?.[""]?.version;
const tauriConfig = JSON.parse(readText("src-tauri/tauri.conf.json"));
const cargoToml = readText("src-tauri/Cargo.toml");
const cargoVersion = cargoToml.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
const cargoLockVersion = readText("src-tauri/Cargo.lock").match(
  /\[\[package\]\]\s+name\s*=\s*"quota-beacon"\s+version\s*=\s*"([^"]+)"/m,
)?.[1];
const versionSources = {
  VERSION: version,
  "package.json": packageVersion,
  "package-lock.json": packageLockVersion,
  "src-tauri/Cargo.toml": cargoVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
  "src-tauri/tauri.conf.json": tauriConfig.version,
};
for (const [source, sourceVersion] of Object.entries(versionSources)) {
  if (sourceVersion !== version) {
    fail(`${source} has version ${sourceVersion ?? "<missing>"}; expected ${version}.`);
  }
}

if (identity.productLine === "macos") {
  if (identity.tagPrefix !== "macos-v") {
    fail(`macOS tag prefix must be macos-v, not ${identity.tagPrefix}.`);
  }
  if (identity.nativeCredentialStore !== "Keychain") {
    fail("macOS native credential store must be Keychain.");
  }
  if (tauriConfig.app?.macOSPrivateApi !== true) {
    fail("macOS requires app.macOSPrivateApi=true.");
  }
  if (!cargoToml.includes("macos-private-api")) {
    fail("macOS requires the Cargo macos-private-api feature.");
  }
  if (!existsSync(join(repositoryRoot, "src/macosWindowConfig.test.ts"))) {
    fail("macOS configuration regression test is missing.");
  }
  if (existsSync(join(repositoryRoot, "src-tauri/src/window_material.rs"))) {
    fail("Windows window_material.rs must not exist on the macOS product line.");
  }
} else if (identity.productLine === "windows") {
  if (identity.tagPrefix !== "v") {
    fail(`Windows tag prefix must be v, not ${identity.tagPrefix}.`);
  }
  if (identity.nativeCredentialStore !== "DPAPI") {
    fail("Windows native credential store must be DPAPI.");
  }
  if (tauriConfig.app?.macOSPrivateApi === true) {
    fail("Windows must not enable app.macOSPrivateApi.");
  }
  if (!existsSync(join(repositoryRoot, "src-tauri/src/window_material.rs"))) {
    fail("Windows native window_material.rs is missing.");
  }
  if (!readText("src-tauri/src/account_vault.rs").includes("CryptProtectData")) {
    fail("Windows DPAPI account-vault implementation is missing.");
  }
} else {
  fail(`unsupported product line ${identity.productLine}.`);
}

console.log(
  `Product-line preflight passed: ${identity.productLine} | branch ${branch} | version ${version} | tag ${identity.tagPrefix}${version}`,
);
