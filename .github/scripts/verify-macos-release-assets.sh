#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 1 ]] || {
  echo "usage: verify-macos-release-assets.sh <assets-dir>" >&2
  exit 2
}

assets_dir="$1"
expected="quota-beacon-macos-universal-ad-hoc.dmg
quota-beacon-macos-universal-ad-hoc.dmg.sha256
quota-beacon-macos-universal-ad-hoc.zip
quota-beacon-macos-universal-ad-hoc.zip.sha256"
actual="$(find "$assets_dir" -maxdepth 1 -type f -exec basename {} \; | sort)"
[[ "$actual" == "$expected" ]] || {
  printf 'Unexpected release asset list:\n%s\n' "$actual" >&2
  exit 1
}

(
  cd "$assets_dir"
  shasum -a 256 -c quota-beacon-macos-universal-ad-hoc.dmg.sha256
  shasum -a 256 -c quota-beacon-macos-universal-ad-hoc.zip.sha256
)

echo "Verified the fixed four-file release asset allowlist and both SHA-256 checksums."
