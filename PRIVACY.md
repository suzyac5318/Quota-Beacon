# Privacy

Quota Beacon is designed to be local-first and minimal.

## What It Reads

- The app reads the local Codex Desktop login file from `CODEX_HOME/auth.json` or the user's `.codex/auth.json`.
- The app sends the existing Codex access token only to the ChatGPT quota endpoints needed to read Codex usage.
- The app may read the account identifier from the login file or token payload only to set the request header expected by the quota service.

## What It Stores

Quota Beacon stores widget preferences in its own application config directory:

- locked state
- always-on-top state
- pinned provider
- auto-rotate interval
- language and custom quota palette

When the user explicitly saves a Codex account on macOS, Quota Beacon stores that account's login data only in the local macOS Keychain under the app-scoped service `app.quotabeacon.desktop.accounts`. The app config directory stores only the account alias, masked email, one-way account fingerprint, random profile ID, and active profile ID; it does not store the token or complete account ID there.

Quota Beacon does not store raw quota responses, user prompts, or chat history. Saved credentials and account metadata are not uploaded to Quota Beacon or any third party.

## What It Sends

The app only calls these quota-related HTTPS endpoints from the local desktop process:

- `https://chatgpt.com/backend-api/wham/usage`
- `https://chatgpt.com/backend-api/wham/rate-limit-reset-credits`

No telemetry, analytics, crash reporting, or third-party tracking is included.

## Logging

Logs are intentionally generic. They must not include tokens, account IDs, raw backend responses, request headers, local auth paths, or personal file paths.

## Accuracy Boundary

Quota Beacon displays quota windows returned by the Codex quota service. It does not estimate quota from local token usage and does not fabricate values when the response shape is unknown.
