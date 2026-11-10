# Latest Open PR Review and Remediation Spec

Date: 2026-06-25
Last confirmed: 2026-06-25 16:53 CST
Repo: `tover0314-w/opentypeless`
Baseline branch: `main`
Baseline commit: `1cfda30`
Status: proposed

## 1. Executive Summary

This spec reviews the latest four open PRs and turns the review into an implementation plan.

Reviewed PRs:

| PR | Title | State | Decision |
| --- | --- | --- | --- |
| #61 | `feat: add Requesty as an OpenAI-compatible LLM provider` | Mergeable | Can merge after normal smoke check. No hard blocker found. |
| #60 | `feat: Recordings - save, play, re-transcribe, delete` | Mergeable | Do not merge yet. Feature is useful but has data-loss, memory, disk-cleanup, and long-recording correctness issues. |
| #53 | `Added 60dB integration` | Mergeable | Do not merge yet. The request body does not match 60dB's documented API. |
| #52 | `feat: add macos microphone input selector` | Conflicting | Do not merge as-is. Resolve conflicts and tighten device-selection behavior before review. |

Short version:

1. #61 is small and mostly fine.
2. #53 is broken at the API boundary: 60dB expects multipart field `audio`; the PR sends `file`.
3. #60 is the highest-risk PR. It adds a real product feature, but it can silently truncate STT, load too much audio into memory, leave hidden files on disk, and produce orphan recordings after timeout.
4. #52 is a real missing capability, but its PR is currently conflicting and should be rebuilt carefully.

## 2. Sources and Verification

Local review context:

1. `git fetch --prune origin` completed on 2026-06-25 16:53 CST.
2. `main` and `origin/main` both pointed at `1cfda30`.
3. Current local worktree already had unrelated local edits before review; this spec does not revert or depend on them.
4. Current local edits mainly add Volcengine/Doubao provider work and entitlement changes. They do not already contain Requesty, 60dB, Recordings, or microphone selector implementations.
5. Temporary detached worktrees were used under `/tmp/opentypeless-pr-review.TYqvqR`.
6. Latest reviewed PR heads:
   - #61 head `7179acf`, base `1cfda30`, mergeable but checks unstable.
   - #60 head `f3b43d0`, base `1cfda30`, mergeable but checks unstable.
   - #53 head `18b6244`, base `99a63e0`, mergeable but stale/unstable.
   - #52 head `05e8caa`, base `99a63e0`, conflicting.
7. Existing PR review/comment context:
   - #53 has a maintainer comment from 2026-06-16 saying it should not merge until API assumptions, response/error handling, local tests, and a completed test plan are added.
   - #60/#61/#52 had no PR comments or reviews at this check.
8. PR #61 frontend verification after `npm ci`:
   - `npm run build`: pass
   - `npm run test`: pass, 14 files / 125 tests
   - `npm ci` reported existing dependency audit issues: 1 moderate, 1 high.
9. Current local worktree frontend verification:
   - `npm run build`: pass
   - `npm run test`: pass, 14 files / 135 tests
10. PR #60 frontend build passed in the earlier clean worktree after `npm ci`:
   - `npm run build`: pass
   - Vite emitted only existing style warnings about dynamic imports and chunk size.
11. PR #60 Rust compile check passed once in the earlier clean worktree:
   - `cargo check --manifest-path src-tauri/Cargo.toml`: pass
12. Later Rust checks/tests for #52/#53/#60/#61/current worktree hit local macOS filesystem/toolchain errors:
   - `failed to open object file: Operation not permitted`
   - `Unable to find libclang`
   - The latest retry used `CARGO_TARGET_DIR=/tmp/opentypeless-cargo-check-current` and still failed while building third-party dependency archives such as `time-core`, `cfb`, and `html5ever`.
   These were local environment failures, not source-level assertion failures.
13. GitHub checks for the reviewed PRs are currently red/unstable:
   - #61: `check`, `update-release-draft`, `welcome` failed.
   - #60: `check`, `update-release-draft` failed.
   - #53: `check`, `update-release-draft`, `welcome` failed.
   - #52: `check`, `update-release-draft` failed and the PR is conflicting.
   These are not sufficient full-green CI evidence.

External API docs checked:

1. Requesty docs confirm OpenAI-compatible base URL `https://router.requesty.ai/v1` and model IDs such as `openai/gpt-4o`.
   Source: https://docs.requesty.ai/quickstart
2. 60dB docs confirm `POST https://api.60db.ai/stt`, multipart form field `audio`, supported formats MP3/WAV/FLAC/OGG/M4A, 25MB max file size, and 10 minute max duration.
   Source: https://60db.mintlify.app/api-reference/stt/speech-to-text

## 3. Highest-Priority Findings

### P0: #60 can silently truncate long recordings for Whisper-compatible cloud STT

Problem:

#60 increases `max_recording_seconds` from `300` to `3600` in `src/components/Settings/GeneralPane.tsx`, but existing Whisper-compatible cloud STT providers still cap buffered PCM at about 24MB, roughly 12.5 minutes at 16kHz mono.

Relevant code:

1. `src/components/Settings/GeneralPane.tsx`
   - slider max is now `3600`.
2. `src-tauri/src/stt/whisper_compat.rs`
   - `MAX_AUDIO_BYTES_CLOUD = 24 * 1024 * 1024`.
3. `src-tauri/src/pipeline.rs`
   - `provider.send_audio(&data).await` result is ignored.

Failure mode:

1. User sets recording duration to 60 minutes.
2. User uses `openai-whisper`, `groq-whisper`, `glm-asr`, or another cloud Whisper-compatible provider.
3. After provider buffer reaches its limit, `send_audio` returns an error.
4. Pipeline ignores that error.
5. Recording continues, and the app may save the full recording file.
6. STT only transcribes the portion that fit in provider memory.
7. User sees a partial transcript without a clear error.

Why this matters:

This is worse than a normal failure because it looks successful while losing most of the transcript.

Required fix:

1. Never ignore `send_audio` errors.
2. Add provider limit metadata:
   - max upload bytes.
   - max recording duration.
   - whether long recording is supported.
3. Enforce the smaller of app max duration and provider duration before recording starts.
4. If a provider limit is reached mid-session, stop recording and show a user-facing error.
5. If local/custom Whisper intentionally supports longer recordings, keep it separate from cloud providers.

Acceptance criteria:

1. A 60-minute recording cannot silently produce a 12-minute transcript.
2. Cloud Whisper providers show or enforce their upload limit.
3. `send_audio` failure is covered by a Rust test or pipeline-level unit helper test.
4. UI explains provider-specific max duration if user selects an incompatible setting.

### P0: #53 sends the wrong multipart field to 60dB

Problem:

60dB documents the file part as `audio`, but #53 uses `file` in both the probe command and provider implementation.

Relevant code:

1. `src-tauri/src/commands/stt.rs`
   - `Form::new().part("file", file_part)`
2. `src-tauri/src/stt/sixtydb.rs`
   - `Form::new().part("file", file_part)`

Documented API:

```bash
curl -X POST https://api.60db.ai/stt \
  -H "Authorization: Bearer your-api-key" \
  -F "audio=@recording.mp3" \
  -F "language=en" \
  -F "timestamps=true"
```

Failure mode:

1. User configures 60dB.
2. Test connection sends `file`, not `audio`.
3. Real transcription also sends `file`, not `audio`.
4. 60dB rejects or ignores the uploaded audio.
5. Provider appears added in UI but does not actually work.

Secondary issue:

#53 also hardcodes 60dB's upload limit as 10MB / about 5.4 minutes, while the current official docs state 25MB and 10 minutes. This is less severe than the wrong multipart field, but it would make valid recordings fail earlier than they need to.

Required fix:

1. Change both request paths to use `audio`.
2. Add tests that assert multipart field name, ideally with an HTTP mock server.
3. Update provider limits to match docs:
   - max file size: 25MB.
   - max duration: 10 minutes.
4. Do not rely on undocumented "silent audio is free" behavior for connection testing unless verified.

Acceptance criteria:

1. `test_stt_connection` uses documented request shape.
2. `SixtyDbProvider::disconnect` uses documented request shape.
3. Valid 60dB keys pass connection test.
4. Invalid keys return a clear auth error.
5. Oversized recordings fail before upload with a user-facing message.

### P1: #60 preloads up to 200 full audio files into JS memory

Problem:

`src/components/Recordings/index.tsx` loads 200 recording rows, then immediately loops through every row and calls `readRecordingBytes`.

Relevant code:

1. `PAGE_SIZE = 200`
2. `useEffect(() => { for (const r of recordings) loadAudio(r) }, [recordings])`
3. `readRecordingBytes(entry.id)` returns full file bytes.
4. JS turns each file into a `Blob` URL.

Failure mode:

1. User enables saved recordings.
2. User records long sessions.
3. User opens Recordings page.
4. App reads every listed audio file into memory.
5. UI stalls or app memory spikes.

Required fix:

1. Load only metadata on page open.
2. Load audio bytes only when the user presses play or expands a row.
3. Keep at most one or a small number of blob URLs alive at a time.
4. Add explicit revoke behavior when switching rows, deleting a row, or navigating away.
5. Consider streaming via asset/file URL if WebKit constraints allow it after testing.

Acceptance criteria:

1. Opening Recordings with 200 rows does not read audio bytes.
2. Pressing play on one row loads only that row's audio.
3. Deleting a row revokes its blob URL.
4. Memory stays bounded in a manual test with many long recordings.

### P1: #60 clear history and history retention can orphan files

Problem:

#60 stores file paths on history rows, but existing history cleanup paths only delete database rows. They do not delete associated audio files.

Relevant code:

1. `src-tauri/src/commands/history.rs`
   - `clear_history` calls `state.clear()`.
2. `src-tauri/src/storage/mod.rs`
   - `HistoryStore::clear` runs `DELETE FROM history`.
   - `HistoryStore::add` prunes rows beyond `MAX_HISTORY_ENTRIES` with `DELETE FROM history WHERE id NOT IN (...)`.

Failure modes:

1. User clicks "clear history".
   - History rows are gone.
   - Audio files remain in app data directory.
   - UI no longer has references to delete them.
2. User exceeds 5000 history rows.
   - Older rows are deleted.
   - Their audio files remain on disk.

Required fix:

1. Before deleting history rows, collect all non-null `recording_file` paths.
2. Delete files after the DB transaction or return paths to the command layer for deletion.
3. Make missing files non-fatal.
4. Add a startup cleanup job for orphan files in `<app_data_dir>/recordings`.
5. Ensure `max_saved_recordings` and `MAX_HISTORY_ENTRIES` cleanup share one file-deletion helper.

Acceptance criteria:

1. Clear history removes audio files.
2. History retention removes audio files for deleted rows.
3. Manual delete still preserves transcript row.
4. Missing file references do not fail clear-history.
5. Tests cover clear, retention prune, manual delete, and orphan cleanup.

### P1: #60 STT timeout can create stale/orphan recording state

Problem:

The STT task writes the recording file only after `provider.disconnect()` returns. `wait_for_stt` times out after 600 seconds but does not abort or invalidate the STT task. The task may later finish and write a recording file after `stop()` has already returned.

Relevant code:

1. `src-tauri/src/pipeline.rs`
   - `STT_FINALIZE_TIMEOUT_SECS = 600`
   - `wait_for_stt` logs timeout and continues.
   - STT task writes recording file after `provider.disconnect()`.

Failure mode:

1. User stops recording.
2. STT provider hangs or takes longer than timeout.
3. `stop()` returns and may clear state.
4. STT task eventually completes.
5. It writes a file into app data.
6. No matching history row is saved, or stale `recording_file` state is consumed by a later session.

Required fix:

1. On STT timeout, mark the session stale.
2. Notify the STT task to abort.
3. Ensure stale tasks cannot write `recording_file`.
4. Save the recording independently from provider disconnect, or save it before STT finalization starts.
5. Use a session-scoped recording result instead of one shared `recording_file` mutex.

Acceptance criteria:

1. Timeout does not leave a file without a DB row.
2. Timeout does not allow stale data to affect a later recording.
3. A timed-out STT session has a visible user-facing error.
4. A timed-out saved recording is either deleted or persisted with an explicit `transcription_status = "failed"` row.

### P2: #60 has inconsistent frontend/backend defaults for saved recording format

Problem:

The Rust default config sets `recording_format` to `flac`, while the frontend store default sets it to `wav`.

Relevant code:

1. `src-tauri/src/storage/mod.rs`
   - `recording_format: "flac".to_string()`
2. `src/stores/appStore.ts`
   - `recording_format: "wav"`

Why this matters:

The backend config should usually overwrite the frontend default after load, so this is not a merge blocker by itself. But it can create confusing first-render behavior, test assumptions, or onboarding/settings screenshots that disagree with the actual persisted default.

Required fix:

1. Pick one default format.
2. Keep Rust, frontend store, tests, and settings copy aligned.
3. If FLAC is preferred, make sure playback support is manually verified on the supported WebView targets.

## 4. PR-by-PR Review

## 4.1 PR #61: Requesty provider

Summary:

This PR adds Requesty to the LLM provider list, default config, provider type union, and locale labels.

Code path:

1. Frontend provider list adds `requesty`.
2. Default config uses:
   - base URL: `https://router.requesty.ai/v1`
   - model: `openai/gpt-4o-mini`
3. Rust LLM factory already routes all non-cloud providers through the OpenAI-compatible provider.

Findings:

1. No hard blocker found.
2. Requesty docs confirm the OpenAI-compatible base URL.
3. Requesty docs show model IDs in provider-prefixed form such as `openai/gpt-4o`; `openai/gpt-4o-mini` is consistent with that pattern.
4. Optional Requesty analytics headers (`HTTP-Referer`, `X-Title`) are recommended by docs but not required.

Required before merge:

1. Manual connection smoke test with a Requesty API key.
2. Model fetch smoke test if Requesty exposes `/models` in the expected OpenAI-compatible shape.
3. Confirm UI displays Requesty in Settings and Onboarding.

Acceptance criteria:

1. Selecting Requesty updates base URL and default model.
2. Test connection succeeds with a valid key.
3. LLM polish request succeeds and streams/outputs as expected.
4. Invalid key returns normal LLM connection failure.

Merge recommendation:

Merge after smoke test. This PR is small and low risk.

## 4.2 PR #60: Recordings feature

Summary:

The feature is valuable: saving audio, playing it back, re-transcribing, deleting, and limiting saved recording count are all useful. The implementation is not ready because it changes recording duration and persistence semantics without enough guardrails.

Intended user value:

1. Save raw audio for later audit.
2. Re-run transcription if the first provider failed.
3. Keep transcript history while deleting audio.
4. Use compressed storage formats.
5. Avoid unbounded saved recording count.

Main architectural issue:

The PR treats a recording as "extra data attached to history", but the pipeline actually needs a session-scoped recording lifecycle:

1. recording starts.
2. audio chunks are captured.
3. audio is written or buffered.
4. audio is finalized.
5. STT succeeds, fails, or times out.
6. history row and recording file are committed together.
7. cleanup runs if anything fails.

The PR does not fully model those states.

Required redesign:

### 4.2.1 Recording lifecycle

Add an explicit session-scoped lifecycle:

```text
capturing -> finalizing_audio -> transcribing -> completed
                              -> transcription_failed
                              -> aborted
                              -> cleanup_failed
```

Implementation requirements:

1. Do not keep all PCM only in a long-lived shared mutex or global field.
2. Prefer streaming PCM to a temp file during capture.
3. On stop:
   - close the temp writer.
   - finalize encoded file.
   - insert or update a DB row in one clear path.
4. On abort:
   - stop capture.
   - delete temp file.
   - do not write history.
5. On STT failure:
   - keep recording if `save_recordings` is enabled.
   - write history row with empty transcript and `transcription_status = "failed"` or equivalent metadata.
6. On no speech:
   - keep recording only if this is intentional product behavior.
   - make the UI label it clearly.
7. On timeout:
   - abort STT.
   - keep or delete recording according to explicit policy.

Data model recommendation:

Add recording metadata beyond a nullable file path:

```rust
recording_file: Option<String>
recording_format: Option<String>
recording_bytes: Option<i64>
recording_status: Option<String> // ready, stt_failed, stt_timeout, deleted
recording_provider: Option<String>
```

This avoids guessing format from path and lets UI explain failed recordings.

### 4.2.2 Provider limits and long recording behavior

Requirements:

1. Add provider capability metadata:
   - `supports_streaming`
   - `supports_batch_upload`
   - `max_recording_seconds`
   - `max_upload_bytes`
   - `can_retranscribe_saved_file`
2. UI should not let user configure a duration that selected provider cannot transcribe.
3. `send_audio` errors must be handled.
4. If a provider limit is reached:
   - stop capture or refuse start.
   - emit localized error.
   - keep saved recording only if policy says so.

Provider-specific rules:

1. OpenAI/Groq/GLM/SiliconFlow Whisper-compatible cloud providers:
   - enforce upload byte cap.
   - do not silently accept 60-minute recordings.
2. Local/custom Whisper:
   - may allow longer recordings if local server supports them.
   - still needs bounded memory/disk behavior.
3. Deepgram/AssemblyAI streaming:
   - can support longer live sessions if backend/provider supports it.
   - saved file should not change streaming transcript behavior.
4. Cloud STT:
   - limit should come from server entitlement/API behavior, not hardcoded guesses.

### 4.2.3 Storage cleanup

Requirements:

1. `clear_history` must delete associated files.
2. `MAX_HISTORY_ENTRIES` pruning must delete associated files.
3. `max_saved_recordings` pruning must delete files and clear DB paths.
4. Manual recording delete should keep transcript row.
5. App startup should remove orphan files that are not referenced by any history row.
6. File deletion should be best-effort but logged.

Suggested helper:

```rust
async fn delete_recording_files(paths: Vec<String>) -> DeleteSummary
```

`DeleteSummary` should include:

1. deleted count.
2. missing count.
3. failed paths.

### 4.2.4 Recordings UI behavior

Requirements:

1. Fetch metadata only when opening Recordings.
2. Load audio only on play.
3. Do not preload all rows.
4. Support pagination or incremental loading.
5. Show status for failed/no-speech/timeout recordings.
6. Disable re-transcribe when current provider does not support it.
7. Show specific error for unsupported re-transcription provider.
8. Do not call `entry.format.toUpperCase()` unless `format` is guaranteed non-null.
9. Search should probably be case-insensitive.

Recommended UI states:

1. ready.
2. loading audio.
3. playing.
4. audio load failed.
5. re-transcribing.
6. re-transcription unsupported.
7. transcription failed but audio available.

### 4.2.5 Re-transcription behavior

Current behavior:

1. Re-transcription uses the current STT provider, not the provider used at recording time.
2. Only Whisper-compatible providers are supported.
3. UI lets user click re-transcribe for every row.

Required behavior:

1. UI should know if current provider supports re-transcription.
2. Unsupported providers should show a clear disabled state.
3. Re-transcription should update raw transcript.
4. Decide whether re-transcription should:
   - update polished text to raw transcript.
   - re-run LLM polish.
   - preserve old polished text until user repolishes.
5. Store provider and timestamp for the new transcript.

Recommended MVP:

1. Re-transcribe updates `raw_text`.
2. Set `polished_text = raw_text` only if polish is off or if UI labels it as "new raw transcript".
3. Add a future "re-polish" action separately.

### 4.2.6 Tests required before merge

Rust tests:

1. `send_audio` error is not ignored.
2. Provider max duration/bytes is enforced.
3. saved recording row is created on STT no-speech when enabled.
4. saved recording row is created on STT failure when enabled.
5. timeout cannot write stale file after session is invalidated.
6. clear history deletes files.
7. history retention deletes files.
8. manual delete removes file and keeps transcript row.
9. orphan cleanup removes unreferenced files.
10. re-transcribe rejects unsupported providers with a typed error.

Frontend tests:

1. Recordings page fetches metadata only.
2. Play loads only the selected recording bytes.
3. Blob URL is revoked on delete/unmount.
4. Unsupported re-transcribe state is visible.
5. Format null/missing does not crash render.
6. Search works on raw and polished text.
7. Settings duration UI respects provider caps.

Manual tests:

1. Save a short WAV recording and play it.
2. Save a short FLAC recording and play it.
3. Save MP3 if feature remains enabled by default.
4. Delete recording and verify file disappears from app data.
5. Clear history and verify files disappear.
6. Record with no speech and verify chosen behavior.
7. Record past provider cap and verify explicit failure, not partial success.
8. Re-transcribe with OpenAI Whisper-compatible provider.
9. Re-transcribe with Deepgram selected and verify disabled/clear message.

Merge recommendation:

Do not merge #60 until P0/P1 issues are fixed. The feature should probably be split into:

1. DB/file lifecycle and cleanup.
2. recording save/play UI with lazy loading.
3. re-transcription.
4. long-recording/provider-limit support.

## 4.3 PR #53: 60dB STT provider

Summary:

The product idea is fine: adding another hosted STT provider helps users. The implementation is not ready because the request shape does not match the documented API.

Required changes:

1. Replace multipart field `file` with `audio`.
2. Align limit checks with 60dB docs:
   - 25MB max file size.
   - 10 minute max duration.
3. Use a documented auth/profile endpoint for connection tests if available.
4. If using a tiny audio probe, confirm valid 60dB behavior with a real key and document it.
5. Parse and map relevant errors:
   - 401/403 auth.
   - quota/credits.
   - file too large.
   - duration too long.
   - unsupported format.
6. Add i18n for user-facing errors if they surface through pipeline.
7. Update the stale docs link in code from `https://docs.60db.ai/...` to the currently used docs URL if that is the canonical source.

Tests required:

1. unit/mock test for multipart field name.
2. unit/mock test for Authorization header.
3. response parsing for success body containing `text`.
4. error mapping for auth.
5. oversized audio rejection.
6. language omitted when `multi`, sent when concrete.

Manual tests:

1. `test_stt_connection` with valid key.
2. `test_stt_connection` with invalid key.
3. short English recording.
4. short non-English recording.
5. long/oversized recording near provider limit.

Merge recommendation:

Do not merge #53 until the multipart field is fixed and a real-key smoke test has been recorded in the PR.

## 4.4 PR #52: microphone input selector

Summary:

The feature is real and useful. Main currently records from the system default input device. A user-facing microphone selector should exist.

Current blocker:

GitHub marks the PR as `CONFLICTING`, so it cannot be merged as-is.

Conflict verification:

Running a temporary `git merge --no-commit --no-ff origin/main` inside the #52 worktree produced conflicts in:

1. `src-tauri/src/pipeline.rs`
2. `src/components/Settings/GeneralPane.tsx`

The merge was aborted afterwards; no repo files were changed by this check.

Review notes:

1. The PR uses CPAL device names as IDs.
2. CPAL names may collide or change.
3. The UI hides duplicate names with a `HashSet`.
4. If two devices have the same visible name, the user cannot select the second one.
5. If a selected device disappears, recording start fails with "Input device not found".
6. Persisted config compatibility is handled on the Rust side with `#[serde(default)]`.

Required changes:

1. Rebase or rebuild on current `main`.
2. Keep the feature scoped to microphone selection.
3. Avoid unrelated macOS Accessibility changes in this PR.
4. Improve duplicate-device handling:
   - display duplicate names with suffixes.
   - preserve enough information to disambiguate within a session.
   - document that persisted selection is best-effort by name.
5. Add user-facing fallback behavior:
   - either fail clearly when selected device is missing.
   - or fall back to system default only if UI/config makes this explicit.
6. Add refresh behavior and loading/error states.

Tests required:

1. config missing `audio_input_device` defaults to empty.
2. empty selected device uses default input.
3. selected device name is passed into `AudioConfig`.
4. missing selected device produces clear error.
5. frontend selector renders system default.
6. frontend selector preserves unavailable selected device.
7. duplicate-name behavior is covered at helper level if possible.

Manual tests:

1. macOS built-in microphone.
2. macOS external USB/Bluetooth microphone.
3. unplug selected microphone and start recording.
4. Windows default microphone.
5. Windows external microphone.

Merge recommendation:

Rebuild as a focused mic selector PR after resolving conflicts. Do not direct-merge current #52.

## 5. Recommended Merge Order

1. Merge #61 after a quick Requesty smoke test.
2. Fix #53 or leave it open; do not merge until API request shape is corrected.
3. Rebuild #52 after current branch conflicts are resolved.
4. Split #60 into smaller PRs or require fixes before merging as one large feature.

Preferred order if all four features are wanted:

1. #61 Requesty provider.
2. #52 microphone selector.
3. #53 60dB provider.
4. #60 recordings, after lifecycle and cleanup redesign.

Reasoning:

1. #61 is independent and low risk.
2. #52 changes capture device selection but not storage semantics.
3. #53 adds provider behavior and must be correct before recordings/re-transcription rely on provider support.
4. #60 touches storage, pipeline, UI, encoding, and provider semantics; it should land only after its invariants are clean.

## 6. Release Gate Checklist

Before merging any of #52/#53/#60:

1. `npm ci`
2. `npm run build`
3. `npm run lint`
4. `npm run test`
5. `cargo fmt --check --manifest-path src-tauri/Cargo.toml`
6. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
7. `cargo test --manifest-path src-tauri/Cargo.toml`
8. manual macOS recording smoke test.
9. manual Windows recording smoke test.

Additional gate for #60:

1. test with at least 50 saved recordings.
2. test with at least one long recording.
3. verify memory behavior when opening Recordings.
4. verify app data directory after delete/clear/prune.
5. verify unsupported re-transcribe provider UX.

Additional gate for #53:

1. test with a valid 60dB key.
2. test with an invalid 60dB key.
3. test short file under documented API shape.
4. test provider limit behavior.

Additional gate for #52:

1. test at least two microphones.
2. test unplug/replug.
3. test fallback/default behavior.

## 7. Suggested PR Comments

### Suggested comment for #60

This feature is useful, but I do not think it is safe to merge yet. The biggest issue is long-recording correctness: the UI now allows 3600s recordings, while Whisper-compatible cloud providers still cap upload buffers around 24MB. `provider.send_audio()` errors are ignored, so a long session can silently produce a partial transcript. The Recordings page also eagerly loads up to 200 full audio files into JS blobs, and history cleanup paths delete DB rows without deleting audio files. I would split or revise this around a session-scoped recording lifecycle, provider duration limits, lazy playback loading, and file cleanup tests.

### Suggested comment for #53

The 60dB integration needs one API-boundary fix before it can work reliably. 60dB's docs show the multipart file field as `audio`, but this PR sends `file` in both the connection probe and the real provider upload. Please change those to `audio`, align limits with the documented 25MB/10 minute cap, and add a mock/request-shape test so this does not regress.

### Suggested comment for #52

This points at a real missing feature, but the branch is currently conflicting and should be refreshed before merge. While rebuilding, please keep the PR scoped to microphone selection and avoid unrelated macOS accessibility changes. One behavior to tighten: CPAL device names are used as IDs, so duplicate or changing names need a visible fallback/disambiguation story.

### Suggested comment for #61

This looks good from code review. The Requesty base URL matches the official OpenAI-compatible docs, and the existing Rust LLM factory already routes non-cloud providers through the OpenAI-compatible provider. I would merge after a quick connection/polish smoke test with a valid Requesty key.

## 8. Open Questions

1. For #60, should saved recordings be kept when STT fails, or only when transcription succeeds?
2. For #60, should re-transcription also re-run LLM polish?
3. For #60, should the cap be number of recordings, total disk bytes, or both?
4. For #60, should long recordings be a local/custom-provider-only feature?
5. For #52, is best-effort name-based device persistence acceptable, or do we need platform-specific stable IDs later?
6. For #53, do we want to add 60dB as a normal hosted provider now, or wait until we have provider capability metadata that can express its 10 minute limit?

## 9. Final Recommendation

Do not merge #60 or #53 in their current form.

Merge #61 after smoke testing.

Rebuild #52 after conflict resolution.

Treat #60 as a feature spec, not a ready PR. The right target is not "recordings exist"; the right target is "recordings cannot lose data, cannot leak files, and cannot make the app fall over when users actually use long recordings."
