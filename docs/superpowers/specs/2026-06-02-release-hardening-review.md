# Release Hardening Review Spec

Date: 2026-06-02
Target release: v0.1.31

## Scope

Review the current release candidate after issue #46 and harden the behavior before publishing the next release.

## Problems Found

1. Recording cancel can still let stale STT work finish later.
   - The previous STT completion notification was shared across sessions.
   - A canceled STT task could still run provider finalization and append text after the user had already canceled or started another session.

2. Custom Whisper reused the hosted STT API key.
   - The optional local/custom API key field was backed by `stt_api_key`.
   - Switching from hosted STT to Custom Whisper could send a hosted provider key to a local server.

3. Selected Text Context needed stronger untrusted-content wording.
   - The selected text is context, not an instruction source.
   - The prompt needed explicit guidance to ignore directives inside `<selected_text>`.

4. Selected text capture rejected valid selections that matched the previous clipboard text.
   - The previous equality check compared copied text with the clipboard backup.
   - If the real selection happened to equal the previous clipboard text, it was discarded.

5. About/release version display was stale.
   - `APP_VERSION` was hardcoded to `v0.1.0`.
   - The release workflow updated package metadata but did not inject the frontend version.

## Expected Behavior

1. Canceling a recording immediately makes the active STT session stale, wakes any waiter, and prevents stale STT output from being emitted or accumulated.
2. Custom Whisper stores and sends `stt_custom_api_key`; hosted STT providers keep using `stt_api_key`.
3. Backup upload excludes both hosted and custom STT API keys.
4. Selected text capture uses a unique clipboard sentinel so unchanged copy attempts are ignored while valid text equal to the old clipboard is accepted.
5. Release builds expose `VITE_APP_VERSION=v<release>` so About displays the actual release version.

## Verification

- `npm test`
- `npm run build`
- `npm run lint`
- `npm run format:check`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
