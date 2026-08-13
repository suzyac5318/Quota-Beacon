#!/usr/bin/env bash
set -euo pipefail

verify_app() {
  local app_path="$1"
  local expected_version="$2"
  local info_plist="$app_path/Contents/Info.plist"
  local executable_name executable_path architectures short_version bundle_version

  [[ -f "$info_plist" ]] || {
    echo "Missing Info.plist in $app_path" >&2
    return 1
  }

  executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$info_plist")"
  executable_path="$app_path/Contents/MacOS/$executable_name"
  [[ -x "$executable_path" ]] || {
    echo "Bundle executable is missing or not executable: $executable_path" >&2
    return 1
  }

  codesign --verify --deep --strict --verbose=2 "$app_path"
  codesign --display --verbose=4 "$app_path" 2>&1 | grep -q '^Signature=adhoc$'

  architectures="$(lipo -archs "$executable_path")"
  grep -qw 'arm64' <<<"$architectures"
  grep -qw 'x86_64' <<<"$architectures"

  short_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$info_plist")"
  bundle_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$info_plist")"
  [[ "$short_version" == "$expected_version" ]] || {
    echo "CFBundleShortVersionString is $short_version, expected $expected_version" >&2
    return 1
  }
  [[ "$bundle_version" == "$expected_version" ]] || {
    echo "CFBundleVersion is $bundle_version, expected $expected_version" >&2
    return 1
  }
}

if [[ "${1:-}" == "--app" ]]; then
  [[ $# -eq 3 ]] || {
    echo "usage: verify-macos-bundle.sh --app <app-path> <expected-version>" >&2
    exit 2
  }
  verify_app "$2" "$3"
  echo "Verified app signature, architectures, version, and executable permission."
  exit 0
fi

[[ $# -eq 2 ]] || {
  echo "usage: verify-macos-bundle.sh <bundle-dir> <expected-version>" >&2
  exit 2
}

bundle_dir="$1"
expected_version="$2"
app_count="$(find "$bundle_dir/macos" -maxdepth 1 -type d -name '*.app' -print | wc -l | tr -d '[:space:]')"
dmg_count="$(find "$bundle_dir/dmg" -maxdepth 1 -type f -name '*.dmg' -print | wc -l | tr -d '[:space:]')"

[[ "$app_count" == "1" ]] || {
  echo "Expected exactly one app bundle, found $app_count under $bundle_dir/macos" >&2
  exit 1
}
[[ "$dmg_count" == "1" ]] || {
  echo "Expected exactly one DMG, found $dmg_count under $bundle_dir/dmg" >&2
  exit 1
}

app_path="$(find "$bundle_dir/macos" -maxdepth 1 -type d -name '*.app' -print)"
dmg_path="$(find "$bundle_dir/dmg" -maxdepth 1 -type f -name '*.dmg' -print)"
verify_app "$app_path" "$expected_version"
hdiutil verify "$dmg_path"

echo "Verified exactly one app and DMG, ad-hoc signature, arm64/x86_64 architectures, bundle version, executable permission, and DMG integrity."
