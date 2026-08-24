# Quota Beacon Windows v<version>

Quota Beacon Windows is a local-first floating desktop widget for checking Codex usage limits from the local Codex Desktop login state.

## Download

- `quota-beacon-windows-unsigned.zip`
- `quota-beacon-windows-unsigned.zip.sha256`

## Install

1. Sign in to Codex Desktop on the same Windows machine.
2. Verify the ZIP with the published SHA-256 file.
3. Extract the ZIP and run the Windows installer.
4. Because the current build is unsigned, confirm the download came from this repository before accepting any SmartScreen prompt.

## Checks

- Windows product-line preflight passed.
- Frontend and Rust tests passed.
- Web and Windows desktop builds passed.
- Windows ZIP SHA-256 verified.
- Windows installed-app smoke test completed or explicitly recorded as unverified.
- No macOS, Personal-edition, credential, or private artifacts are attached.
