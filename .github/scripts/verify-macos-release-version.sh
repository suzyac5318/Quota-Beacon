#!/usr/bin/env bash
set -euo pipefail

root="."
tag=""
bundle_app=""
lineage_root="83046c8eada1ecfb1d68c18b6b8507d86a21155f"
skip_lineage=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root) root="${2:?missing value for --root}"; shift 2 ;;
    --tag) tag="${2:?missing value for --tag}"; shift 2 ;;
    --bundle-app) bundle_app="${2:?missing value for --bundle-app}"; shift 2 ;;
    --lineage-root) lineage_root="${2:?missing value for --lineage-root}"; shift 2 ;;
    --skip-lineage) skip_lineage=true; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

root="$(cd "$root" && pwd)"
version="$(tr -d '[:space:]' < "$root/VERSION")"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "VERSION must contain a stable semantic version, got: $version" >&2
  exit 1
}

read_json_version() {
  ROOT_PATH="$root" JSON_FILE="$1" JSON_EXPR="$2" node <<'NODE'
const fs = require("fs");
const path = require("path");
const data = JSON.parse(fs.readFileSync(path.join(process.env.ROOT_PATH, process.env.JSON_FILE), "utf8"));
const value = process.env.JSON_EXPR.split(".").reduce((current, key) => current[key], data);
process.stdout.write(String(value));
NODE
}

check_version() {
  local source="$1"
  local actual="$2"
  [[ "$actual" == "$version" ]] || {
    echo "$source has version $actual, expected $version" >&2
    exit 1
  }
}

check_version package.json "$(read_json_version package.json version)"
check_version package-lock.json "$(read_json_version package-lock.json version)"
check_version package-lock-root "$(read_json_version package-lock.json packages..version)"
check_version Cargo.toml "$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/src-tauri/Cargo.toml" | head -n 1)"
check_version Cargo.lock "$(awk '/^name = "quota-beacon"$/{getline; if ($1 == "version") {gsub(/"/, "", $3); print $3; exit}}' "$root/src-tauri/Cargo.lock")"
check_version tauri.conf.json "$(read_json_version src-tauri/tauri.conf.json version)"
check_version README.md "$(sed -n 's/^> 当前 macOS 版本：`\([^`]*\)`（独立 `macos` 版本线）$/\1/p' "$root/README.md")"

if [[ -n "$tag" ]]; then
  [[ "$tag" =~ ^macos-v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || {
    echo "Release tag must match macos-v<semver>, got: $tag" >&2
    exit 1
  }
  [[ "${BASH_REMATCH[1]}" == "$version" ]] || {
    echo "Tag version ${BASH_REMATCH[1]} does not match source version $version" >&2
    exit 1
  }

  if [[ "$skip_lineage" != true ]]; then
    git -C "$root" rev-parse --verify "refs/tags/$tag" >/dev/null
    tag_commit="$(git -C "$root" rev-list -n 1 "$tag")"
    head_commit="$(git -C "$root" rev-parse HEAD)"
    [[ "$tag_commit" == "$head_commit" ]] || {
      echo "Tag $tag points to $tag_commit, but checkout is $head_commit" >&2
      exit 1
    }
    commit_subject="$(git -C "$root" log -1 --format=%s "$head_commit")"
    [[ "$commit_subject" == "$tag:"* ]] || {
      echo "Release commit must start with $tag:, got: $commit_subject" >&2
      exit 1
    }
    git -C "$root" merge-base --is-ancestor "$lineage_root" "$head_commit" || {
      echo "Tag commit is not descended from the macOS product-line root $lineage_root" >&2
      exit 1
    }
  fi
fi

if [[ -n "$bundle_app" ]]; then
  "$root/.github/scripts/verify-macos-bundle.sh" --app "$bundle_app" "$version"
fi

echo "Verified macOS release version $version across all configured sources${tag:+ and tag $tag}."
