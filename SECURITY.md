# Security

## Supported Use

Quota Beacon is a local desktop utility that reads Codex quota using the user's existing Codex Desktop login state.

## Reporting Issues

Report vulnerabilities privately through [GitHub Security Advisories](https://github.com/suzyac5318/Quota-beacon/security/advisories/new). Please do not open a public issue for a suspected vulnerability.

Do not include tokens, account IDs, raw backend responses, unredacted screenshots, or local file paths. Redact sensitive information before sharing logs or reproduction details. Maintainers will acknowledge a valid report through the private advisory and coordinate remediation before public disclosure.

## Security Boundaries

- The app does not persist Codex credentials.
- The app does not log request headers or raw quota responses.
- The app caps auth file reads at 256 KB and quota responses at 1 MB.
- The app does not follow redirects for quota HTTP requests.
- The app does not redeem reset credits or change account settings.

## Release Notes For Maintainers

Before publishing a release, verify:

- Source archives do not include local installers, build outputs, `.codex`, QA screenshots, or environment files.
- Windows bundles are built by Windows CI or a clean Windows machine; macOS artifacts are maintained only on the separate macOS product line.
- Unsigned builds are clearly labeled as unsigned.
- Signed releases are produced only with maintainer-controlled certificates and secrets.
