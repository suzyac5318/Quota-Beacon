# Quota Beacon macOS X.Y.Z

Quota Beacon is a local-first floating desktop widget for checking Codex usage limits from the local Codex Desktop login state.

## Downloads

- macOS Universal ad-hoc DMG: `quota-beacon-macos-universal-ad-hoc.dmg`
- macOS Universal ad-hoc ZIP: `quota-beacon-macos-universal-ad-hoc.zip`
- DMG SHA-256: `quota-beacon-macos-universal-ad-hoc.dmg.sha256`
- ZIP SHA-256: `quota-beacon-macos-universal-ad-hoc.zip.sha256`

## Install

1. Sign in to Codex Desktop on the same machine.
2. Download the macOS Universal package.
3. Follow the macOS instructions below.

### macOS ad-hoc app note

This macOS build uses an ad-hoc signature and is not notarized. If macOS blocks the first launch:

1. Drag the app from the DMG to Applications.
2. Right-click the app in Applications and choose Open.
3. Choose Open again in the system prompt.
4. If needed, choose Open Anyway in System Settings -> Privacy & Security.

## Privacy

Quota Beacon stores explicitly saved Mac account credentials only in the local macOS Keychain. Its metadata files do not contain tokens or complete account IDs, and it does not store prompts, chats, or raw quota responses. See `PRIVACY.md`.

## Notes

- macOS uses an ad-hoc signature without notarization and may show a security warning.
- Codex quota is read from non-public quota service responses and may stop working if the response shape changes.
- The app shows stale/unavailable states instead of estimating quota.
- Windows and macOS share the React/CSS UI and behavior layer but use separate branches, version tags, and releases.
- The macOS bundle is CI-built and automatically checked but still awaits real-Mac interaction testing.

## Checks

- Frontend tests passed.
- Rust tests passed.
- Web build passed.
- Release tag, source versions, app bundle versions, and Mac product-line history verified by CI.
- Final DMG and ZIP contents, signatures, Universal architectures, executable permission, and separate SHA-256 files verified by CI.
- Release asset allowlist contains exactly the two packages and their two matching checksum files.
- Sensitive-content scan passed for source package.
