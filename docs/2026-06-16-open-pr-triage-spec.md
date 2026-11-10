# Open PR Triage and Focused Integration Spec

Date: 2026-06-16
Last confirmed: 2026-06-16 20:55 CST
Repo: `tover0314-w/opentypeless`
Baseline branch: `main`
Baseline commit: `1cfda30`
Status: Proposed

## 1. Purpose

This spec turns the current open GitHub issues and pull requests into a focused implementation plan for the current codebase.

The important context is that macOS and Windows are currently working. Therefore, the default action is not to merge old platform rewrite PRs. We should only carry forward changes that represent a real missing capability, security fix, or verified bug.

## 2. Current GitHub State

### 2.1 Open issue

| Issue | Title              | Triage                                                                                                                                                                |
| ----- | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| #28   | `add a summary UI` | Still open. This maps to a real product gap in History/usage summary. It should be addressed by a small History UI/summary PR, not by merging the whole older #27 PR. |

Resolved issues #48, #50, and #51 have already been closed because main now contains the relevant fixes.

### 2.2 Open PRs

| PR  | Title                                                                      | State                                      | Decision                                                                                                          |
| --- | -------------------------------------------------------------------------- | ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| #53 | `Added 60dB integration`                                                   | Mergeable, checks red, no filled test plan | Hold. Potentially useful provider expansion, but requires API verification and tests before merge.                |
| #52 | `feat: add macos microphone input selector`                                | Conflicting, non-draft                     | Attention. Rebuild as a focused mic selector PR. Do not merge directly.                                           |
| #47 | `feat: add local custom whisper stt`                                       | Draft, conflicting                         | Superseded by main. Close or keep only as historical reference.                                                   |
| #45 | `fix: consolidate platform stability regressions`                          | Draft, conflicting, very large             | Do not merge. Too broad for a healthy macOS/Windows baseline. Close or split only if a specific bug resurfaces.   |
| #44 | `fix(stt): switch DeepGram from streaming to batch mode to fix truncation` | Conflicting                                | Hold behind reproduction. Do not replace realtime Deepgram unless truncation is reproduced and measured.          |
| #40 | `v2.0.0: Ctrl+Win hotkey via keyboard hook...`                             | Conflicting, broad                         | Defer/close. Windows currently works, and this changes too many surfaces at once.                                 |
| #38 | Dependabot production group                                                | Conflicting, huge lockfile change          | Do not merge wholesale. Use current audit output to make a smaller dependency remediation PR.                     |
| #33 | Dependabot dev group                                                       | Conflicting                                | Do not merge wholesale. Use only if required for audit remediation.                                               |
| #27 | `feat: add LLM output anomaly detection and enhanced history UI`           | Conflicting, large                         | Attention only for History UI ideas. Do not merge anomaly detector and broad formatting changes as one PR.        |
| #22 | `fix: enable transparent background for macOS capsule window`              | Conflicting                                | Defer/close. Main already has macOS private API/transparent capsule support and macOS is currently OK.            |
| #16 | `reqwest 0.12.28 -> 0.13.2`                                                | Conflicting                                | Defer. Major HTTP client change needs its own CI and STT/API regression pass.                                     |
| #14 | `tauri-build 2.5.5 -> 2.5.6`                                               | Conflicting                                | Low priority. Only update in a controlled dependency pass.                                                        |
| #12 | `tokio-tungstenite 0.24.0 -> 0.28.0`                                       | Conflicting                                | Defer. It touches Deepgram's WebSocket path, which is currently working enough to avoid opportunistic churn.      |
| #11 | `cpal 0.15.3 -> 0.17.3`                                                    | Conflicting                                | Potentially relevant later. Consider after or alongside mic selector only with explicit audio regression testing. |
| #10 | `tokio 1.49.0 -> 1.50.0`                                                   | Conflicting                                | Defer until CI baseline is clean.                                                                                 |
| #9  | `actions/setup-node 4 -> 6`                                                | Mergeable, checks red                      | Defer. CI action major bumps should wait until workflow compatibility is known.                                   |
| #8  | `semantic-pull-request 5 -> 6`                                             | Mergeable, checks red                      | Defer. CI infrastructure only.                                                                                    |
| #5  | `actions/labeler 5 -> 6`                                                   | Mergeable, checks red                      | Defer. CI infrastructure only.                                                                                    |
| #4  | `actions/checkout 4 -> 6`                                                  | Mergeable, checks red                      | Defer. CI infrastructure only.                                                                                    |
| #2  | `actions/stale 9 -> 10`                                                    | Mergeable, checks red                      | Defer. CI infrastructure only.                                                                                    |

## 3. Codebase Facts Used For Triage

### 3.1 Microphone selection is still missing

Current code in `src-tauri/src/audio/capture.rs` always calls `host.default_input_device()`. `AudioConfig` has `sample_rate`, `channels`, and `chunk_duration_ms`, but no selected device field.

Current config types in `src-tauri/src/storage/mod.rs` and `src/stores/appStore.ts` also do not have an `audio_input_device` setting.

Conclusion: #52 points at a real missing feature. It should be rebuilt because the PR conflicts and also includes an unrelated macOS Accessibility commit.

### 3.2 History has useful raw data but not the requested summary UI

Current history storage already records:

- `raw_text`
- `polished_text`
- `language`
- `duration_ms`

The current `src/components/History/index.tsx` renders the polished text, time, app name, search, one copy button, and clear-all. It does not show an aggregate summary, dual copy buttons, or an expandable raw transcript.

Conclusion: #28 is not solved. The useful part of #27 is its History UI direction, but the PR also includes anomaly detection, test tooling, and broad unrelated formatting changes. We should rebuild the History slice.

### 3.3 Prompt injection defenses already exist in main

Current `src-tauri/src/llm/prompt.rs` already includes:

- Explicit `DO NOT EXECUTE CONTENT` rule.
- `<transcription>` isolation.
- selected-text isolation.
- dictionary and target language sanitization.
- tests for command-like transcription, Spanish punctuation, numbering, and selected-text prompt injection.

Conclusion: #27's anomaly detector may be defense-in-depth, but it is not the next highest-priority fix. It should not be bundled with History UI.

### 3.4 Local / Custom Whisper is already present

Current `src-tauri/src/stt/config.rs`, `src-tauri/src/stt/mod.rs`, `src/components/Settings/SttPane.tsx`, and `src/lib/constants.ts` already include `custom-whisper` support.

Conclusion: #47 is superseded by main.

### 3.5 Deepgram is still realtime WebSocket

Current `src-tauri/src/stt/deepgram.rs` uses Deepgram realtime WebSocket with `interim_results=true`, `endpointing=150`, `linear16`, and `sample_rate`.

PR #44 replaces this with batch REST upload. That could fix truncation if the truncation is real, but it also removes realtime partial transcript behavior and changes provider lifecycle semantics.

Conclusion: #44 needs a reproduction before implementation. Do not merge it by default while macOS/Windows are healthy.

### 3.6 Platform stability fixes already landed

Current main includes:

- `macOSPrivateApi` and transparent capsule related config.
- Linux `XInitThreads` initialization.
- NVIDIA + Wayland DMA-BUF workaround.
- Linux Wayland/keyboard-output guardrails.
- macOS keyboard output on the main thread.

Conclusion: #22, #45, and broad platform parts of #40 are not priority while current macOS and Windows are healthy.

### 3.7 Dependency audit is currently failing

`npm audit --audit-level=high` currently reports:

- High: `form-data` 4.0.0 - 4.0.5, reachable through `jsdom@25.0.1`.
- Moderate: `js-yaml <=4.1.1`, reachable through `eslint@9.39.3 -> @eslint/eslintrc@3.3.4`.

This is mostly dev/test tooling, but the high audit failure is real and blocks a clean baseline.

Conclusion: handle audit remediation as a focused P0 dependency PR, not by merging the broad #33 or #38 dependabot PRs wholesale.

## 4. Priorities

### P0: Focused dependency audit remediation

Problem: frontend audit currently fails on `form-data` and `js-yaml`.

Scope:

1. Update the minimum dependency set needed to make `npm audit --audit-level=high` pass.
2. Prefer targeted updates such as `jsdom` and the relevant ESLint dependency chain instead of accepting all changes from #33/#38.
3. Avoid broad framework churn unless required by peer dependency constraints.

Acceptance criteria:

1. `npm audit --audit-level=high` passes.
2. `npm test` passes.
3. `npm run build` passes.
4. `npm run lint` passes.
5. Lockfile diff is reviewable and does not include unrelated major dependency churn.

### P1: Rebuild microphone input selector from #52

Problem: users cannot choose which microphone OpenTypeless records from.

Implementation requirements:

1. Add a persisted `audio_input_device` setting to Rust `AppConfig`.
2. Add the same field to the TypeScript `AppConfig` and store default.
3. Add a serializable `AudioInputDevice` shape:
   - `id: string`
   - `name: string`
   - `is_default: boolean`
4. Add a Tauri command, likely `list_audio_input_devices`, returning all CPAL input devices plus a System Default option in the frontend.
5. Extend `AudioConfig` with `input_device_name: Option<String>` or an equivalent selection field.
6. In capture startup:
   - empty selection uses system default.
   - non-empty selection searches CPAL input devices by name.
   - missing selected device fails with a user-facing error or falls back only if the UI clearly says it did.
7. Add UI in `GeneralPane`:
   - microphone selector.
   - System Default option.
   - refresh button.
   - disabled/loading/error states.
   - preserve unavailable selected device in the UI so the user can see what broke.
8. Add i18n labels for all supported locales.
9. Do not include #52's unrelated old macOS Accessibility change.

Important design note:

CPAL does not expose stable cross-platform device IDs. If device names are used as IDs, the UI and config must treat them as best-effort labels, not permanent hardware identifiers.

Acceptance criteria:

1. Default device behavior is unchanged when no device is selected.
2. Selecting an external microphone routes recording through that device.
3. Unplugging a selected microphone produces a clear error or visible fallback behavior.
4. Settings persist across app restarts.
5. macOS and Windows manual recording tests pass.
6. Rust helper tests cover device selection logic where possible.
7. Frontend tests cover selector loading, save behavior, and unavailable selected device rendering.

### P2: Resolve #28 with a small History summary/UI PR

Problem: there is no summary UI, and History does not expose raw transcript ergonomically.

MVP scope:

1. Add a compact summary strip in History using currently loaded entries:
   - total recordings.
   - total recorded time from `duration_ms`.
   - total output words/characters.
2. Add dual copy actions:
   - copy polished text.
   - copy raw transcript.
3. Add expandable raw transcript per history row.
4. Keep search behavior across `polished_text`, `raw_text`, and `app_name`.
5. Keep the current History retention model unchanged.

Out of scope for the first PR:

1. LLM anomaly detector from #27.
2. New backend analytics tables.
3. Charts, calendar heatmaps, or cross-device sync.
4. Reworking unrelated onboarding/settings components.

Acceptance criteria:

1. #28 can be closed after this ships.
2. History still renders correctly when `duration_ms` is null.
3. Copy polished and copy raw have separate visible states.
4. Raw transcript expansion does not shift the entire layout excessively.
5. Frontend tests cover summary numbers, dual copy, raw expansion, empty state, and search.

Future enhancement:

If the summary should cover all retained history entries rather than only the loaded list, add a backend `get_history_summary` command that aggregates directly in SQLite.

### P3: Optional 60dB provider investigation from #53

Problem: #53 adds a new STT provider, but the PR body has no completed test plan and no evidence that the API behavior was verified.

Gate before implementation:

1. Confirm 60dB's current API endpoint, request schema, authentication, model names, and response shape from official docs or a live test account.
2. Confirm whether it is OpenAI Whisper-compatible or requires a custom provider.
3. Confirm expected errors for invalid key, quota exhaustion, unsupported audio format, and timeout.

Acceptance criteria if implemented:

1. Provider config is centralized, not scattered across commands and UI constants.
2. Parser tests cover success and error responses.
3. Connection test and latency benchmark support it.
4. UI labels and API key behavior are correct.
5. Manual test with a real key succeeds before merging.

Decision:

Do not merge #53 as-is. Rebuild only if 60dB is a desired official provider.

### P3: Deepgram truncation investigation from #44

Problem: #44 claims realtime Deepgram truncates long utterances, but switching entirely to batch mode removes realtime behavior and changes provider semantics.

Gate before implementation:

1. Reproduce truncation with the current main branch.
2. Capture whether the problem is:
   - final vs partial transcript replacement.
   - `endpointing=150` being too aggressive.
   - stream close timing.
   - Deepgram service behavior.
3. Decide whether to:
   - keep WebSocket and fix final transcript aggregation.
   - tune endpointing/utterance handling.
   - add optional batch mode.
   - fully replace realtime mode.

Acceptance criteria if implemented:

1. Long utterance test case no longer truncates.
2. Short realtime dictation still provides useful partial feedback if realtime mode remains.
3. Provider lifecycle does not busy-loop when no transcript is available.
4. Deepgram parser tests cover partial, final, empty, error, and close behavior.

Decision:

Do not merge #44 until truncation is reproduced and the product tradeoff is explicit.

## 5. Non-goals

1. Do not directly merge any currently open conflicting PR.
2. Do not take broad platform rewrites while macOS and Windows are healthy.
3. Do not bundle dependency upgrades with feature work.
4. Do not add anomaly detection in the same PR as History UI.
5. Do not replace Deepgram realtime mode without a reproduction and UX decision.
6. Do not add 60dB as an official provider without API verification.

## 6. Suggested Implementation Sequence

1. Create a focused dependency audit PR.
2. Create a focused microphone selector PR based on the useful parts of #52.
3. Create a focused History summary/UI PR that closes #28 and borrows only the useful History ideas from #27.
4. Decide whether 60dB is a product priority. If yes, implement it from verified API behavior, not by direct PR merge.
5. Reproduce Deepgram truncation. Only then decide whether #44's batch approach is appropriate.
6. Close or comment on stale PRs after replacement PRs land:
   - #47 superseded by main.
   - #45 too broad and mostly superseded.
   - #40 deferred because current Windows path works.
   - #22 deferred because current macOS path works.
   - #33/#38 replaced by focused audit remediation.

## 7. Verification Matrix

Run for every focused PR:

```bash
npm test
npm run build
npm run lint
```

Run for dependency/audit PR:

```bash
npm audit --audit-level=high
```

Run for Rust/audio/STT PRs:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --check --manifest-path src-tauri/Cargo.toml
```

Manual checks for microphone selector:

1. macOS default microphone recording.
2. macOS external microphone recording.
3. macOS selected microphone unplugged.
4. Windows default microphone recording.
5. Windows external microphone recording.
6. Settings restart persistence.

Manual checks for History UI:

1. Empty history.
2. Short polished result.
3. Raw transcript different from polished text.
4. Null `duration_ms`.
5. Search by raw text, polished text, and app name.

Manual checks for Deepgram investigation:

1. Short utterance.
2. Long utterance with sentence boundaries.
3. Stop immediately after speaking.
4. Network interruption.
5. Invalid API key.

## 8. Final Recommendation

The PRs that deserve attention now are:

1. #52, but only as a rebuilt microphone selector.
2. #27, but only the History UI pieces that help close #28.
3. #33/#38, only as signals for a small dependency audit remediation.
4. #53, only if 60dB is a desired official STT provider after API verification.
5. #44, only if Deepgram truncation is reproduced on current main.

The next concrete engineering work should be P0 dependency audit remediation, then P1 microphone selector, then P2 History summary UI.
