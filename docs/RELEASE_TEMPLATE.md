# Quota Beacon macOS 1.7.0

Quota Beacon is a local-first floating desktop widget for checking Codex usage limits from the local Codex Desktop login state.

## Downloads

- macOS Universal ad-hoc: `.dmg` or `quota-beacon-macos-universal-ad-hoc.zip`
- macOS SHA-256: `quota-beacon-macos-universal-ad-hoc.sha256`

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

Quota Beacon does not store Codex tokens, account IDs, prompts, chats, raw quota responses, or local auth paths. It stores only widget preferences. See `PRIVACY.md`.

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
- macOS Universal ad-hoc bundle, DMG, and SHA-256 generated and verified by CI.
- Sensitive-content scan passed for source package.
