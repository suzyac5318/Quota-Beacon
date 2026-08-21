#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
verifier="$root/.github/scripts/verify-macos-release-version.sh"
bundle_verifier="$root/.github/scripts/verify-macos-bundle.sh"
asset_verifier="$root/.github/scripts/verify-macos-release-assets.sh"

expect_failure() {
  if "$@" >/dev/null 2>&1; then
    echo "Expected command to fail: $*" >&2
    exit 1
  fi
}

version="$(tr -d '[:space:]' < "$root/VERSION")"
"$verifier" --root "$root"
"$verifier" --root "$root" --tag "macos-v$version" --skip-lineage
expect_failure "$verifier" --root "$root" --tag macos-v1.2 --skip-lineage
expect_failure "$verifier" --root "$root" --tag macos-v999.0.0 --skip-lineage

fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/src-tauri"
cp "$root/VERSION" "$root/README.md" "$root/package.json" "$root/package-lock.json" "$fixture/"
cp "$root/src-tauri/Cargo.toml" "$root/src-tauri/Cargo.lock" "$root/src-tauri/tauri.conf.json" "$fixture/src-tauri/"
ROOT_PATH="$fixture" node <<'NODE'
const fs = require("fs");
const path = require("path");
const file = path.join(process.env.ROOT_PATH, "package.json");
const data = JSON.parse(fs.readFileSync(file, "utf8"));
data.version = "0.0.0";
fs.writeFileSync(file, JSON.stringify(data));
NODE
expect_failure "$verifier" --root "$fixture"

cp "$root/package.json" "$fixture/package.json"
ROOT_PATH="$fixture" node <<'NODE'
const fs = require("fs");
const path = require("path");
const file = path.join(process.env.ROOT_PATH, "README.md");
const content = fs.readFileSync(file, "utf8").replace(
  /> 当前 macOS 版本：`[^`]+`/,
  "> 当前 macOS 版本：`0.0.0`",
);
fs.writeFileSync(file, content);
NODE
expect_failure "$verifier" --root "$fixture"

mkdir -p "$fixture/bundle/macos/One.app" "$fixture/bundle/macos/Two.app" "$fixture/bundle/dmg"
touch "$fixture/bundle/dmg/one.dmg"
expect_failure "$bundle_verifier" "$fixture/bundle" "$(tr -d '[:space:]' < "$root/VERSION")"
rm -rf "$fixture/bundle/macos/Two.app"
touch "$fixture/bundle/dmg/two.dmg"
expect_failure "$bundle_verifier" "$fixture/bundle" "$(tr -d '[:space:]' < "$root/VERSION")"
rm -f "$fixture/bundle/dmg/one.dmg" "$fixture/bundle/dmg/two.dmg"
expect_failure "$bundle_verifier" "$fixture/bundle" "$(tr -d '[:space:]' < "$root/VERSION")"

assets="$fixture/assets"
mkdir -p "$assets"
printf 'dmg' > "$assets/quota-beacon-macos-universal-ad-hoc.dmg"
printf 'zip' > "$assets/quota-beacon-macos-universal-ad-hoc.zip"
(
  cd "$assets"
  shasum -a 256 quota-beacon-macos-universal-ad-hoc.dmg > quota-beacon-macos-universal-ad-hoc.dmg.sha256
  shasum -a 256 quota-beacon-macos-universal-ad-hoc.zip > quota-beacon-macos-universal-ad-hoc.zip.sha256
)
"$asset_verifier" "$assets"
printf 'tampered' >> "$assets/quota-beacon-macos-universal-ad-hoc.dmg"
expect_failure "$asset_verifier" "$assets"
printf 'dmg' > "$assets/quota-beacon-macos-universal-ad-hoc.dmg"
printf 'tampered' >> "$assets/quota-beacon-macos-universal-ad-hoc.zip"
expect_failure "$asset_verifier" "$assets"
printf 'zip' > "$assets/quota-beacon-macos-universal-ad-hoc.zip"
touch "$assets/unexpected.txt"
expect_failure "$asset_verifier" "$assets"
rm "$assets/unexpected.txt" "$assets/quota-beacon-macos-universal-ad-hoc.zip.sha256"
expect_failure "$asset_verifier" "$assets"
(
  cd "$assets"
  shasum -a 256 quota-beacon-macos-universal-ad-hoc.zip > quota-beacon-macos-universal-ad-hoc.zip.sha256
)
rm "$assets/quota-beacon-macos-universal-ad-hoc.dmg"
expect_failure "$asset_verifier" "$assets"

echo "Verified release guard positive and negative cases."
