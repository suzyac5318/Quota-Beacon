# Contributing

Thanks for helping improve Quota Beacon.

## Before Opening Issues

Do not paste tokens, account IDs, raw backend responses, local auth paths, or screenshots containing personal data.

## Development

```bash
npm install
npm run test
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Use `npm run tauri dev` for desktop testing. Browser preview uses mock data and cannot verify real quota reads.

## Pull Requests

- Target `Windows` only for Windows work. macOS work must target the separate `macos` branch and must not be copied or merged into `Windows`.
- Use `windows-v<semver>: <description>` for Windows version commits and `windows-<type>: <description>` for other Windows commits.
- Use `windows-v<semver>` for new Windows release tags. Legacy `v*` tags are historical and must not be reused for new releases.
- Do not combine Windows and macOS files, CI jobs, build outputs, or release assets in one PR or Release.
- Keep changes small and focused.
- Preserve the privacy boundary documented in `PRIVACY.md`.
- Do not add telemetry or raw response logging.
- Add or update tests when changing quota parsing, snapshot merging, or formatting.
