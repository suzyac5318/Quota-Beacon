## Product line (select exactly one)

- [ ] Windows: target branch is `Windows`; commits use `windows-...`; changes and assets are Windows-only.
- [ ] macOS: target branch is `macos`; commits use `macos-...`; changes and assets are macOS-only.
- [ ] This PR does not mix Windows and macOS, and contains no Personal-edition, credential, build-output, or private files.

## Commit and release identity

- [ ] The selected platform uses its matching commit prefix: `windows-...` or `macos-...`.
- [ ] The selected platform uses its matching tag: `windows-v<semver>` or `macos-v<semver>`.
- [ ] The Release title explicitly contains `Windows` or `macOS`.
- [ ] Release assets belong only to the selected platform.

## Verification

- [ ] `npm run preflight:product-line -- --expect <windows|macos>`
- [ ] `npm test`
- [ ] `npm run build`
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml`
- [ ] `git diff --check`

Describe any unverified Windows installer or real-device behavior explicitly.
