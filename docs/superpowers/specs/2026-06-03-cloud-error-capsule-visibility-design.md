# Cloud STT Error and Capsule Visibility Fix Design

## Background

Issue #46 was closed after fixing two crash/freeze paths:

- clicking the recording capsule cancel button no longer bubbles into the outer capsule stop handler;
- Cloud STT buffering no longer busy-spins while waiting for upload-based transcription.

A follow-up manual test exposed a remaining user-facing failure mode. A Cloud STT request returned HTTP 403 from `/api/proxy/stt`, but the desktop capsule later showed `No speech detected. Please try again.` This hides the real reason and makes a quota/auth failure look like a microphone or silence problem.

The desktop capsule also has an existing `capsule_auto_hide` setting, but the right-click capsule menu does not expose it. Users see a permanent desktop microphone icon unless they discover the setting page.

## Current Evidence

- Cloud STT provider maps every HTTP 403 from the cloud STT endpoint to `AppError::Auth`.
- The current Cloud 403 fallback text is `STT quota exceeded`; this makes empty or unparsable 403 bodies look like quota failures.
- `AppError::Auth` maps to the frontend error code `stt_invalid_key`.
- The STT task can emit an STT error, then `wait_for_stt()` sees an empty accumulated transcript and emits the plain string `No speech detected. Please try again.`
- The frontend already supports structured `pipeline:error` payloads shaped as `{ code, details, retry_count }`, but locale files currently do not define the STT error codes under `errors`.
- `capsule_auto_hide` already exists in Rust storage and frontend state.
- The main window and capsule window each keep their own Zustand state. The capsule window loads config once at startup, so using its full config object for persistence can overwrite newer settings saved from the main window.
- `useCapsuleResize()` hides the capsule only on `becameIdle` and shows it only on `leftIdle`. It does not handle "still idle, auto-hide just enabled" or "still idle, error just appeared."
- The system tray currently reads `ui_language` from the top level of `settings.json`, while config is saved under `app_config.ui_language`.
- The settings dirty-bar save path calls full `update_config`, but that Rust command currently saves only; it does not broadcast config changes to the capsule webview or refresh tray labels after language changes.
- `CapsuleRecording` already shows volume-driven waveform bars through `audioVolume`.
- `DurationTimer` already shows recording duration and auto-stops at the configured maximum.
- `CapsuleProcessing` and `CapsulePolishing` already represent transcribing/thinking states.

## Goals

1. Preserve the real cloud STT failure reason in the desktop UI.
2. Distinguish Cloud STT quota exhaustion from invalid auth/session errors.
3. Make no-speech a structured, localized error shown only when STT actually succeeds with empty text.
4. Add complete locale coverage for the new and existing STT error codes.
5. Let users hide the idle desktop capsule from the capsule right-click menu.
6. Keep the capsule visible during recording, transcribing, polishing, outputting, context menu display, and short-lived error display.
7. Give users a recovery path when the idle capsule is hidden and the right-click menu is inaccessible.
8. Sync capsule visibility changes across main and capsule webviews without overwriting unrelated unsaved settings.
9. Support a hidden-by-default experience for new installs without silently changing existing users' persisted preference.

## Non-Goals

- Do not redesign the cloud quota policy or change server-side quota reservation amounts in this desktop fix.
- Do not replace the existing waveform, timer, transcribing, polishing, or outputting capsule components.
- Do not remove the system tray icon; it remains the always-available access point for settings, history, recording, and quit.
- Do not introduce a second setting that duplicates `capsule_auto_hide`.
- Do not make the capsule menu persist the entire config object.
- Do not force existing users who deliberately keep the capsule visible into auto-hide.

## Recommended Approach

Reuse the existing `capsule_auto_hide` configuration, but change how it is toggled and synchronized.

The capsule and tray menus should not call the general full-config `update_config` command. They should call a dedicated partial command that loads the latest config from Rust storage, updates only `capsule_auto_hide`, saves it, and broadcasts a patch event.

This keeps the existing behavior model, makes the feature discoverable, and avoids stale webview state overwriting newer settings.

Rejected alternatives:

- Always hide the capsule while idle with no setting. This removes user control and makes the feature harder to discover.
- Keep the setting only in the settings page. This preserves the current poor discoverability.
- Create a separate "desktop icon hidden" flag. This duplicates `capsule_auto_hide` and risks conflicting states.
- Let the capsule menu persist a full config object. The capsule window can hold stale config and overwrite settings changed elsewhere.

## Desired Behavior

### Cloud STT Errors

When Cloud STT receives HTTP 403:

- If the response has a stable quota code such as `quota_exceeded` or `stt_quota_exceeded`, desktop emits `stt_quota_exceeded`.
- If there is no stable code but the response error message clearly contains quota exhaustion wording, desktop emits `stt_quota_exceeded`.
- If the body is empty, unparsable, or does not clearly indicate quota exhaustion, desktop emits `stt_invalid_key` with a generic access-denied detail.
- The frontend renders the localized `errors.<code>` message.
- The pipeline does not emit `stt_no_speech_detected` afterward for the same recording session.

When Cloud STT succeeds but returns empty text:

- Desktop emits `stt_no_speech_detected`.
- The error is localized.
- This message is reserved for real silence or unintelligible audio, not quota/auth/network failures.

### Capsule Visibility

The capsule window's desired visibility must be computed from state every time relevant inputs change:

```ts
const shouldShowCapsule =
  !capsuleAutoHide ||
  contextMenuOpen ||
  capsuleExpanded ||
  hasError ||
  pipelineState !== 'idle'
```

When `capsule_auto_hide` is enabled:

- idle with no error, no menu, and no expansion: capsule window is hidden;
- recording: capsule window shows waveform, recording duration, and cancel action;
- transcribing: capsule window shows transcribing state and partial transcript when available;
- polishing: capsule window shows thinking state;
- outputting: capsule window shows completion state briefly;
- error: capsule window shows the localized error briefly, then returns to idle hidden state;
- opening the context menu keeps the capsule window visible until the menu closes.

When `capsule_auto_hide` is disabled:

- idle capsule remains visible as the small microphone capsule.

State transitions must include these cases:

- enabling auto-hide while already idle hides the capsule after the menu closes;
- disabling auto-hide while already idle shows the capsule;
- receiving an error while already idle shows the capsule;
- clearing the error while idle and auto-hide is enabled hides the capsule.

### Menus and Recovery

Capsule right-click menu:

- shows `Hide capsule when idle` when `capsule_auto_hide` is disabled;
- shows `Keep capsule visible` when `capsule_auto_hide` is enabled;
- calls the dedicated `set_capsule_auto_hide(enabled)` Tauri command;
- does not call the full-config `update_config` command;
- closes after the command succeeds;
- hides the capsule immediately after closing if toggled on while idle and no error is active;
- shows the capsule immediately if toggled off while idle.

System tray menu:

- adds the same capsule visibility toggle as a recovery path;
- calls the same Rust helper as the Tauri command;
- remains available even when the desktop capsule is hidden;
- reads the language from `app_config.ui_language`.

Settings page:

- keeps the existing `Hide capsule when idle` toggle and dirty-bar save flow;
- stays in sync after right-click or tray toggles through a config patch event;
- saving settings through the dirty bar also broadcasts persisted config fields that affect other webviews;
- preserves unrelated dirty settings when an external capsule visibility patch arrives.

Default behavior:

- completely new installs with no stored `app_config` default `capsule_auto_hide` to `true`;
- existing stored configs with an explicit `capsule_auto_hide` keep that value;
- existing stored configs that are missing `capsule_auto_hide` default that one field to `false`, preserving the historical visible-idle behavior;
- frontend defaults should match the new-install default, but loaded Rust config is the source of truth.

Onboarding:

- If the new-install default remains hidden while idle, onboarding copy must say that the capsule appears while recording and can be restored from the tray/settings.
- The completion step must not imply that an idle capsule is always visible and clickable.

## Architecture

### Rust Error Model

Add an explicit quota error variant:

- `AppError::Quota(String)`

Mapping:

- `Quota` is not retryable.
- `Quota` maps to `UserError { code: "stt_quota_exceeded", details, retry_count: 0 }`.
- 401 and non-quota 403 continue to map to `stt_invalid_key`.

Cloud STT should use a testable helper, for example:

```rust
fn cloud_stt_forbidden_error(body: &str) -> AppError
```

Classification order:

1. Parse JSON and inspect stable fields such as `code`, `error_code`, or `type`.
2. If a stable field says quota exhaustion, return `AppError::Quota`.
3. Otherwise inspect `error` or `message` text for quota exhaustion wording.
4. If quota is still not clearly indicated, return `AppError::Auth("Cloud STT access denied".to_string())`.

The helper must not default an empty or unparsable body to quota.

### Pipeline Error Latching

Track the last STT failure for the active session. Once an STT provider error is emitted, `wait_for_stt()` must not emit no-speech for that same session.

Use a session-aware value, for example:

```rust
Arc<Mutex<Option<(u64, UserError)>>>
```

Rules:

- clear the latched value when a new recording successfully transitions from idle to recording;
- set the latched value only after `should_finalize_stt_task(...)` returns true;
- `wait_for_stt(stt_control)` only consumes a latched error whose session id matches the active `stt_control`;
- stale tasks must not latch errors for newer sessions;
- real empty successful STT can still emit `stt_no_speech_detected`.

### Frontend i18n

Add an `errors` object to every locale file if it is missing. Include at least:

- `stt_timeout`
- `stt_invalid_key`
- `stt_failed`
- `stt_quota_exceeded`
- `stt_no_speech_detected`
- `output_fallback_clipboard`

The frontend should continue to support plain string errors temporarily for compatibility, but new backend STT errors should use structured payloads.

### Capsule Visibility Commands and Sync

Add a dedicated Tauri command:

```rust
set_capsule_auto_hide(enabled: bool) -> Result<(), String>
```

The command should:

- load the latest config from `ConfigManager`;
- update only `capsule_auto_hide`;
- save the config;
- emit a `config:patch` event to all open webviews with `{ "capsule_auto_hide": enabled }`;
- refresh tray labels/menu.

The tray menu should call the same Rust helper internally rather than duplicating save logic.

Update the existing full `update_config` command after save:

- compare the previous persisted config with the new config before overwriting the cache;
- emit `config:patch` for persisted fields that affect the capsule or tray, at minimum `capsule_auto_hide`, `max_recording_seconds`, and `ui_language` when they changed;
- refresh tray labels/menu when `ui_language` changes;
- refresh tray labels/menu when `capsule_auto_hide` changes so the tray visibility item is immediately correct;
- keep full-config saves as the only path that persists ordinary settings-page edits.

Add a frontend store action for persisted patches, for example:

```ts
applyPersistedConfigPatch(patch: Partial<AppConfig>): void
```

Behavior:

- merge the patch into `config`;
- merge the same patch into `savedConfig` when `savedConfig` is not null;
- when the patch contains `ui_language`, update i18n and `localStorage` in that webview;
- preserve unrelated dirty fields in `config`;
- keep `savedConfig` as the persisted baseline for patched fields.

This is required so a tray/capsule visibility change does not erase unsaved settings edits.

### Tray Labels

Fix tray language lookup before adding the new visibility item:

- read `app_config.ui_language` from `settings.json`, or use a shared helper that loads `AppConfig`;
- keep a fallback to English if the store is missing or malformed;
- add localized labels for both visibility states.

Language changes:

- language selection may update frontend state immediately for responsiveness;
- persisted tray labels must refresh only after the language is saved through `update_config` or another backend save helper;
- the old front-end-only `refresh_tray_labels` call is not sufficient because it can run before the new language is persisted.

### Default Migration

Do not rely on `#[serde(default)]` alone for `capsule_auto_hide`.

Use a config loading helper that distinguishes:

- no stored `app_config`: new install defaults, including `capsule_auto_hide: true`;
- stored `app_config` with explicit `capsule_auto_hide`: preserve explicit value;
- stored `app_config` missing `capsule_auto_hide`: legacy default, `capsule_auto_hide: false`.

## Implementation Notes

- `useCapsuleResize()` should stop relying only on `leftIdle` and `becameIdle`; it should reconcile actual window visibility with the computed `shouldShowCapsule`.
- The context menu itself forces the capsule window visible while open.
- The right-click menu should close after toggling visibility.
- Use lucide `EyeOff` for hiding and `Eye` for keeping visible.
- Menu text must be localized under `capsule.menu`.
- The settings dirty bar may still be used for ordinary settings page edits; the capsule menu and tray menu are immediate actions.
- If the settings page has unsaved changes, an external capsule visibility patch must update only the visibility field's persisted baseline.

## Test Plan

Rust:

- `AppError::Quota` is not retryable.
- `AppError::Quota` maps to `stt_quota_exceeded`.
- Cloud STT 403 quota code maps to `AppError::Quota`.
- Cloud STT 403 quota message maps to `AppError::Quota`.
- Cloud STT 403 empty, unparsable, or non-quota body maps to `AppError::Auth`.
- A latched STT error prevents no-speech emission for the matching session.
- A latched STT error from a stale session does not affect the current session.
- Real empty successful STT can still emit `stt_no_speech_detected`.
- Tray menu builds the capsule visibility item for enabled and disabled states.
- Tray language lookup reads `app_config.ui_language`.
- Full `update_config` emits `config:patch` when `capsule_auto_hide`, `max_recording_seconds`, or `ui_language` changes.
- Full `update_config` refreshes tray labels/menu after persisted `ui_language` changes.
- Full `update_config` refreshes tray labels/menu after persisted `capsule_auto_hide` changes.
- Config loading defaults new installs to `capsule_auto_hide: true`.
- Config loading preserves explicit existing `capsule_auto_hide` values.
- Config loading treats existing configs missing `capsule_auto_hide` as `false`.
- The capsule visibility save helper patches only `capsule_auto_hide`.

Frontend:

- Every locale defines all required `errors` keys.
- Capsule right-click menu shows `Hide capsule when idle` when auto-hide is off.
- Capsule right-click menu shows `Keep capsule visible` when auto-hide is on.
- Capsule right-click menu calls `set_capsule_auto_hide`, not the full-config `updateConfig`.
- `config:patch` updates `config.capsule_auto_hide` and `savedConfig.capsule_auto_hide`.
- `config:patch` updates capsule-relevant persisted settings from full settings saves, including `max_recording_seconds`.
- `config:patch` with `ui_language` updates i18n and `localStorage` in both main and capsule webviews.
- `config:patch` preserves unrelated dirty config fields.
- When a patch field is also dirty locally, the persisted patch wins for that field and updates `savedConfig` for the same field.
- Auto-hide visibility logic hides when auto-hide is enabled while already idle.
- Auto-hide visibility logic shows when an error appears while already idle.
- Auto-hide visibility logic hides again when the error clears while idle.
- Existing recording cancel button tests continue to pass.
- Onboarding copy does not promise an always-visible idle capsule when new installs default to auto-hide.

Manual:

- With Cloud free quota exhausted, recording shows a localized quota message, not no-speech.
- With a real silent recording and available quota, recording shows localized no-speech.
- With auto-hide on, idle capsule disappears; hotkey recording makes it reappear with waveform and timer.
- Right-click menu can enable auto-hide without changing unrelated settings.
- System tray can restore a hidden idle capsule.
- Changing language updates tray labels from the persisted `app_config.ui_language`.
- Saving `Hide capsule when idle` from Settings updates the capsule window without restart.
- Opening settings, making an unrelated unsaved edit, then toggling capsule visibility from tray does not erase the unsaved edit.

## Release and Communication

Release notes should describe this as a desktop UX/error hardening fix:

- Cloud STT quota/auth errors now show the correct localized message.
- Empty speech is no longer used as a fallback message for cloud failures.
- The desktop capsule can be hidden while idle from the right-click menu and restored from the tray/settings.
- New installs hide the idle capsule by default, while existing users keep their persisted preference.

## Confirmed Spec Review

Confirmed issues fixed in this revision:

- P1: Capsule menu full-config persistence could overwrite newer settings from another webview. The revised design requires a partial Rust command.
- P1: External tray/capsule saves could erase dirty settings. The revised design requires a persisted patch action that updates only `capsule_auto_hide` in both `config` and `savedConfig`.
- P1: Auto-hide did not handle idle-to-idle config changes or idle errors. The revised design defines a single visibility formula and transition requirements.
- P1: Hidden-by-default new installs could alter legacy configs missing the field. The revised design requires explicit migration behavior.
- P2: Tray labels read language from the wrong store path. The revised design requires `app_config.ui_language`.
- P2: Empty or unparsable 403 bodies could be misclassified as quota. The revised design forbids quota as the fallback.
- P2: Onboarding copy can conflict with hidden-by-default behavior. The revised design requires copy updates if the new-install default is hidden.
- P2: Full settings saves could persist capsule visibility or language without notifying the capsule window or tray. The revised design requires `update_config` to emit relevant patches and refresh tray labels after persisted language/visibility changes.

Implementation constraints clarified:

- The STT error latch must be session-aware and use the existing stale-session guard.
- The auto-hide logic should expose testable visibility decisions instead of relying only on manual window testing.

Self-review result:

- No placeholders remain.
- The cloud error and capsule visibility requirements are separated but share one release scope.
- The design avoids stale full-config writes.
- The implementation scope is suitable for one TDD plan.
