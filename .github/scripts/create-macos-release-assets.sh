#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 3 ]] || {
  echo "usage: create-macos-release-assets.sh <bundle-dir> <assets-dir> <version>" >&2
  exit 2
}

bundle_dir="$1"
assets_dir="$2"
version="$3"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ "$assets_dir" != "/" && "$assets_dir" != "." ]] || {
  echo "Refusing unsafe assets directory: $assets_dir" >&2
  exit 2
}

"$script_dir/verify-macos-bundle.sh" "$bundle_dir" "$version"
app_path="$(find "$bundle_dir/macos" -maxdepth 1 -type d -name '*.app' -print)"
source_dmg_path="$(find "$bundle_dir/dmg" -maxdepth 1 -type f -name '*.dmg' -print)"

rm -rf "$assets_dir"
mkdir -p "$assets_dir"
zip_path="$assets_dir/quota-beacon-macos-universal-ad-hoc.zip"
dmg_path="$assets_dir/quota-beacon-macos-universal-ad-hoc.dmg"

ditto -c -k --sequesterRsrc --keepParent "$app_path" "$zip_path"
cp "$source_dmg_path" "$dmg_path"

extract_dir="$(mktemp -d)"
trap 'rm -rf "$extract_dir"' EXIT
ditto -x -k "$zip_path" "$extract_dir"
extracted_count="$(find "$extract_dir" -maxdepth 1 -type d -name '*.app' -print | wc -l | tr -d '[:space:]')"
[[ "$extracted_count" == "1" ]] || {
  echo "Expected exactly one app after ZIP extraction, found $extracted_count" >&2
  exit 1
}
extracted_app="$(find "$extract_dir" -maxdepth 1 -type d -name '*.app' -print)"
"$script_dir/verify-macos-bundle.sh" --app "$extracted_app" "$version"
hdiutil verify "$dmg_path"

(
  cd "$assets_dir"
  shasum -a 256 "$(basename "$dmg_path")" > quota-beacon-macos-universal-ad-hoc.dmg.sha256
  shasum -a 256 "$(basename "$zip_path")" > quota-beacon-macos-universal-ad-hoc.zip.sha256
  shasum -a 256 -c quota-beacon-macos-universal-ad-hoc.dmg.sha256
  shasum -a 256 -c quota-beacon-macos-universal-ad-hoc.zip.sha256
)

"$script_dir/verify-macos-release-assets.sh" "$assets_dir"

echo "Created and verified exactly four release assets in $assets_dir."
