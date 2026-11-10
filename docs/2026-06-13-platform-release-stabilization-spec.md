# Platform Release Stabilization Spec

Date: 2026-06-13
Target release: next patch release after v0.1.34, tentatively v0.1.35
Status: implemented locally, release pending

## Executive Summary

当前 v0.1.34 之后的高风险问题确实主要集中在 macOS 和 Ubuntu/Linux，Windows 暂时没有新的平台级 blocker。

结论依据如下：

1. Windows: 当前 main 的前端测试、类型检查、lint、format 和 build 均通过。Open issues 里唯一明确来自 Windows 的 #50 是 v0.1.27 的 LLM polish 行为问题，main 已经有更强的 prompt 隔离；#51 是 STT/TTS 配置诉求或术语混淆，不是 Windows 平台崩溃。当前不应为了 stale PR 里的 Windows 改动主动重写 Windows 输入链路。
2. Ubuntu/Linux: #48 是 v0.1.34 上明确可复现的 Ubuntu 22.04 MATE/X11 崩溃，日志指向 `xcb_xlib_threads_sequence_lost` 和缺少 `XInitThreads`。这是当前最明确的 release blocker。
3. macOS: #42/#43 都围绕 macOS 选中文本复制、中文输入法、Accessibility 权限、Enigo 主线程兼容性。当前 main 仍在后台线程创建 Enigo 并使用 `AXIsProcessTrustedWithOptions` 请求权限，因此 macOS 仍是高风险区。
4. 跨平台/依赖: #33 无法安装依赖；`npm audit --audit-level=high` 当前仍有 high/critical 前端依赖漏洞。这不是某个平台独有，但会影响 release 质量门禁。

本 spec 的策略是：先修最小平台 blocker，保护 Windows 稳定路径，不直接合并过期/混合 PR；再按独立 PR 处理 Deepgram、依赖审计和非 blocker feature。

## Verified Baseline

在当前 main 上已经完成的本地验证：

1. `npm ci`: pass
2. `npm test`: pass, 117 tests
3. `npx tsc --noEmit`: pass
4. `npm run lint`: pass with existing warnings
5. `npm run format:check`: pass
6. `npm run build`: pass
7. `npm audit --audit-level=high --json`: fail, 10 vulnerabilities, including high and critical findings around Vite/Vitest/better-auth dependency graph
8. `cargo`/`rustc`: local machine unavailable, Rust checks must run in CI or on machines with Rust toolchain

Repository state at review time:

1. Branch: `main`
2. Remote: `origin/main`
3. Current tag on main: `v0.1.34`
4. Latest commit reviewed: `99a63e0`
5. GitHub status/workflow data returned no runs for that commit, consistent with Actions/billing being blocked or unavailable.

## Local Implementation Results

Implemented on branch `fix/platform-release-v0.1.35`:

1. Added Linux-only early `XInitThreads` initialization before Tauri/GTK/WebKit startup.
2. Replaced macOS Accessibility prompt FFI with opening the macOS Accessibility settings pane.
3. Changed macOS selected-text Cmd+C simulation to `/usr/bin/osascript` while preserving Windows/Linux Enigo Ctrl+C behavior.
4. Routed macOS keyboard output through Tauri `run_on_main_thread` with oneshot acknowledgement and bounded timeout.
5. Kept Windows/Linux keyboard output on the existing `spawn_blocking` Enigo path.
6. Updated dependency ranges and lockfile to clear npm high/critical audit findings without adopting ESLint 10.

Local verification completed:

1. `npm ci`: pass
2. `npx tsc --noEmit`: pass
3. `npm run lint`: pass with the existing 3 warnings
4. `npm run format:check`: pass
5. `npx prettier --check docs/2026-06-13-platform-release-stabilization-spec.md`: pass
6. `npm test`: pass, 117 tests
7. `npm run build`: pass
8. `npm audit --audit-level=high`: pass, 0 vulnerabilities
9. `cargo fmt --check --manifest-path src-tauri/Cargo.toml`: pass
10. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`: pass
11. `cargo test --manifest-path src-tauri/Cargo.toml`: pass, 110 Rust tests
12. `git diff --check`: pass
13. `npm run tauri -- build`: pass on local macOS aarch64, producing:
    - `src-tauri/target/release/bundle/macos/OpenTypeless.app`
    - `src-tauri/target/release/bundle/dmg/OpenTypeless_0.1.0_aarch64.dmg`

Remaining release work:

1. Run the full matrix on GitHub Actions or equivalent machines: Windows, macOS x64, macOS arm64, Ubuntu 22.04.
2. Manually verify #48 on Ubuntu 22.04 MATE/X11.
3. Manually verify macOS Accessibility, selected text, Chinese IME, keyboard output, and clipboard fallback.
4. Publish a tagged release only after the repository owner billing/Actions path is working or after an explicitly approved self-hosted/local-artifact workaround.

## Open Issue Triage

| Item                                                                       | Platform                    | Severity | Current conclusion                                                                                                                                                                                                                  | Required action                                                                                                 |
| -------------------------------------------------------------------------- | --------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| #48 Crash on Ubuntu 22.04 MATE/X11 with `xcb_xlib_threads_sequence_lost`   | Ubuntu/Linux X11            | P0       | Real v0.1.34 blocker. Root cause is likely Xlib/XCB thread initialization order before GTK/WebKit/Tauri touch X11.                                                                                                                  | Integrate targeted `XInitThreads` fix from #55, then verify on Ubuntu 22.04 MATE/X11.                           |
| #50 Polish LLM punctuation/list numbering/instruction execution in Spanish | Windows report, logic-level | P1/P2    | Report was on v0.1.27. Current main already strengthened prompt by marking transcription/selected text as untrusted. Punctuation and duplicate numbering still need regression tests because model output remains nondeterministic. | Add deterministic prompt/unit coverage and manual Spanish regression; do not treat as Windows platform blocker. |
| #51 Local TTS model URL on Windows client                                  | Windows UI/config request   | P2       | Product is STT-focused; current main already supports Local/Custom Whisper STT config. If user literally means TTS, that is a new feature outside current release stabilization.                                                    | Reply/clarify in issue after review. No release-blocking code change unless missing STT UI is confirmed.        |
| #28 Summary UI and Doubao API compatibility                                | Cross-platform feature      | P3       | Feature request, not release blocker.                                                                                                                                                                                               | Defer until after platform stabilization.                                                                       |

## Open PR Triage

| PR                                                           | Platform/scope                                | Merge decision                                     | Detailed reason                                                                                                                                                                                                                                       |
| ------------------------------------------------------------ | --------------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| #55 `fix: initialize Xlib threading on Linux`                | Ubuntu/Linux                                  | Accept as base, after validation                   | This is the targeted fix for #48. It adds Linux-only `libloading`, loads `libX11.so.6`, resolves `XInitThreads`, calls it at the start of `run()`, and intentionally keeps the library handle alive. Needs Rust/CI and Ubuntu X11 smoke verification. |
| #52 `feat: add macos microphone input selector`              | Audio device selection, mostly UI/Rust config | Defer from blocker release or merge after P0 fixes | Useful feature but not crash fix. Uses CPAL device names as stable IDs, which can collide or change. UI appears cross-platform despite macOS title. Needs device-list tests and manual selection tests before release inclusion.                      |
| #53 `Added 60dB integration`                                 | New STT provider                              | Defer or require rewrite                           | `sixtydb_probe` appears to treat only HTTP success as valid API-key proof while comments say silent audio may be rejected as low-SNR/no credits. Empty PR checklist. Needs API-doc confirmation and provider integration tests.                       |
| #47 `feat: add local custom whisper stt`                     | Custom local STT                              | Close/supersede                                    | Main already contains Local/Custom Whisper. The PR is stale and conflicts with main in config, settings, onboarding, locales, constants, and store. Only cherry-pick docs/tests if something is still missing.                                        |
| #45 `fix: consolidate platform stability regressions`        | Mixed platform/deps/STT/tray                  | Do not merge                                       | Large stale mixed PR with conflicts. It combines unrelated changes and risks destabilizing Windows/Linux/macOS together. Extract only independently reviewed fixes into focused branches.                                                             |
| #44 `fix(stt): switch DeepGram from streaming to batch mode` | Deepgram STT                                  | Rebuild before merge                               | Current PR has a hot-loop/starvation bug: batch `recv_transcript()` returns `Ok(None)` immediately. It also conflicts with main. Fix must make file/batch providers await pending work instead of returning an immediately-ready no-op future.        |
| #43 `fix(mac): move Enigo::new() to main thread`             | macOS input/output                            | Rebuild, do not direct-merge                       | Direction is relevant for macOS, but current implementation routes all platforms through Tauri main thread, blocks on `std::sync::mpsc::recv()` inside async methods, ignores `run_on_main_thread` scheduling failure, and risks clipboard race.      |
| #42 `fix(mac): use osascript for Cmd+C`                      | macOS selected-text copy                      | Rebuild as macOS-only fix                          | The merge is conflict-free but changes non-mac selected-text capture behavior to a queued main-thread Enigo path. The title says macOS only, so Windows/Linux behavior must remain unchanged.                                                         |
| #33 Dependabot dev group                                     | Frontend dependencies                         | Do not merge                                       | `npm ci` fails with peer dependency conflict: `eslint@10.2.0` is incompatible with current `eslint-plugin-react-hooks` peer range. Use targeted dependency bumps instead.                                                                             |
| #10 tokio 1.49 to 1.50                                       | Rust dependency                               | Optional after CI                                  | Low-risk lockfile-only update, but local Rust is unavailable. Gate on `cargo test`, `cargo clippy`, and platform CI.                                                                                                                                  |
| #9 setup-node v4 to v6                                       | CI/release action                             | Optional after Actions health restored             | Low-risk workflow action bump. Not useful until Actions/billing path is healthy.                                                                                                                                                                      |
| #8 semantic-pull-request v5 to v6                            | PR-title workflow                             | Optional after Actions health restored             | Low-risk, but should wait until CI can run reliably.                                                                                                                                                                                                  |

## Release Goals

1. Fix the Ubuntu 22.04 MATE/X11 startup crash without changing Windows behavior.
2. Stabilize macOS text capture/output and Accessibility permission flow without moving Linux/Windows onto new input primitives.
3. Keep the release scoped: no broad mixed PR, no stale conflict-heavy merge, no feature PR unless it has direct release value.
4. Restore a trustworthy release pipeline despite current account billing constraints.
5. Reduce high/critical dependency audit findings or document any unavoidable exception before publishing.

## Non-Goals

1. Do not redesign STT provider architecture in this release.
2. Do not implement the summary UI from #28.
3. Do not add literal TTS support unless #51 is confirmed to be about text-to-speech rather than local/custom speech-to-text.
4. Do not rewrite Windows keyboard hooks or output unless a new Windows-specific reproduction appears.
5. Do not merge PR #45 as a bundle.

## Implementation Plan

### Phase 1: Branch Hygiene and PR Disposition

Create a fresh branch from current `main`, for example:

```bash
git checkout main
git pull --ff-only origin main
git checkout -b fix/platform-release-v0.1.35
```

PR decisions before coding:

1. Mark #55 as the source for the Linux/X11 fix.
2. Close or supersede #47 because main already contains local/custom STT.
3. Close or replace #33 because it cannot install dependencies.
4. Do not merge #45; reference it only as historical context.
5. Rebuild #42/#43 as focused macOS changes.
6. Rebuild #44 separately or defer it if the platform release must stay narrow.
7. Defer #52/#53/#8/#9/#10 unless there is enough CI capacity after P0 fixes.

### Phase 2: Ubuntu/Linux X11 Crash Fix

Target files:

1. `src-tauri/Cargo.toml`
2. `src-tauri/Cargo.lock`
3. `src-tauri/src/lib.rs`
4. `src-tauri/src/linux_x11.rs` as a new Linux-only module

Required behavior:

1. On Linux only, call `XInitThreads()` before any GTK, WebKit, Tauri builder, window, tray, plugin, tracing subscriber side effect, or X11/XCB consumer is initialized.
2. Load `libX11.so.6` dynamically using a Linux-only dependency such as `libloading`.
3. Resolve `XInitThreads` with signature `unsafe extern "C" fn() -> std::ffi::c_int`.
4. If the library cannot be loaded or the symbol cannot be resolved, log a warning and continue. Do not hard-crash on Wayland-only systems or minimal environments.
5. If `XInitThreads()` returns 0, log a warning and continue. Treat it as degraded startup, not fatal.
6. Keep the loaded X11 library handle alive for the process lifetime after resolving/calling the symbol.
7. Preserve the existing NVIDIA + Wayland DMA-BUF workaround and run it after the Xlib thread initialization.
8. Do not modify Windows or macOS startup paths.

Tests and verification:

1. Add unit tests around Linux helper status mapping where possible without requiring X11 at test time.
2. CI: `cargo fmt --check --manifest-path src-tauri/Cargo.toml`.
3. CI: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`.
4. CI: `cargo test --manifest-path src-tauri/Cargo.toml`.
5. Manual Ubuntu 22.04 MATE/X11: launch installed app or AppImage from terminal; confirm no `xcb_xlib_threads_sequence_lost`.
6. Manual Ubuntu 22.04 MATE/X11: start/stop recording, verify tray/menu still works.
7. Manual Ubuntu Wayland: app should still launch; keyboard output may fall back according to existing Wayland guardrails.
8. Manual Linux without `xdotool`: confirm user-visible fallback/warning still behaves as before.

Acceptance criteria:

1. #48 reproduction no longer crashes on startup.
2. No behavior change on Windows.
3. No behavior change on macOS startup.
4. Linux startup logs show Xlib threading was initialized or clearly explain why it was skipped.

### Phase 3: macOS Accessibility Permission Stability

Current risk:

1. `request_accessibility_permission()` calls `AXIsProcessTrustedWithOptions` and manually builds a CoreFoundation dictionary.
2. PR #43 indicates this path can crash on newer macOS versions.
3. The existing implementation leaks CoreFoundation objects and uses unsafe FFI directly.

Required behavior:

1. Keep `is_accessibility_trusted()` using `AXIsProcessTrusted()`.
2. Replace the prompt path with a safer flow inside the existing
   `request_accessibility_permission()` Tauri command:
   - Check `AXIsProcessTrusted()`.
   - If not trusted, open System Settings to the Accessibility Privacy pane using
     `/usr/bin/open x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
   - Return `false` so the frontend can show the existing permission-needed state.
3. Do not call `AXIsProcessTrustedWithOptions` in the release fix unless it is wrapped safely with correct CoreFoundation ownership and tested on the affected macOS version.
4. Do not request permission repeatedly in a loop.
5. Keep non-macOS behavior as `true`.

Manual verification:

1. macOS with permission absent: app opens the correct Settings pane and does not crash.
2. macOS after permission granted: `is_accessibility_trusted()` returns true and recording/output works.
3. macOS after permission revoked: app returns to permission-needed state without crashing.

Acceptance criteria:

1. No crash during permission request on macOS.
2. User can recover by granting Accessibility permission.
3. Linux and Windows permission paths remain unchanged.

### Phase 4: macOS Selected Text Capture

Current main behavior:

1. `capture_selected_text()` backs up clipboard.
2. It writes a sentinel.
3. It uses Enigo to send Cmd+C on macOS or Ctrl+C elsewhere.
4. It sleeps for clipboard settle time.
5. It restores the clipboard and ignores the sentinel.

Risk:

1. On macOS, Enigo Cmd+C can conflict with Chinese IME or be unreliable.
2. PR #42 uses `osascript`, which is directionally correct, but its merge result changes non-mac behavior.

Required behavior:

1. On macOS only, send Cmd+C through `/usr/bin/osascript`:

```applescript
tell application "System Events" to keystroke "c" using command down
```

2. Keep the sentinel clipboard protection exactly as main has it.
3. Keep the clipboard backup/restore behavior exactly as main has it.
4. Keep Windows/Linux direct Enigo Ctrl+C behavior unchanged unless a separate platform bug is reproduced.
5. If `osascript` fails, log the failure and either:
   - return `None`, or
   - fall back to a macOS-only main-thread Enigo helper with acknowledgement and timeout.
6. If a fallback helper is added, it must:
   - report scheduling failure from `run_on_main_thread`,
   - send completion over a oneshot channel,
   - have a bounded timeout,
   - never block the async runtime indefinitely,
   - never sleep and assume the main-thread action already ran.

Manual verification:

1. macOS with English input method: selected text is captured and clipboard is restored.
2. macOS with Chinese input method active: selected text is captured and no stray character/composition is inserted.
3. macOS with no selected text: sentinel is ignored and stale clipboard is not treated as context.
4. Windows: Ctrl+C selected-text capture behavior is unchanged.
5. Ubuntu/X11: Ctrl+C selected-text capture behavior is unchanged.

Acceptance criteria:

1. macOS selected-text capture becomes more reliable.
2. No Windows/Linux selected-text regression.
3. Clipboard restore behavior remains deterministic.

### Phase 5: macOS Keyboard Output Main-Thread Safety

Current main behavior:

1. `KeyboardOutput::type_text()` uses `tokio::task::spawn_blocking`.
2. It creates `Enigo` in that worker thread.
3. It chunks text and sends keyboard events.

Risk:

1. PR #43 suggests `Enigo::new()` or CGEvent usage can be unsafe off main thread on newer macOS.
2. Directly moving all platforms to Tauri main thread is too risky.
3. Blocking on `std::sync::mpsc::recv()` inside async code can freeze or starve execution.

Required behavior:

1. On macOS only, route Enigo creation and key/text events through a helper that runs on the Tauri main thread.
2. Preserve existing Windows/Linux `spawn_blocking` implementation.
3. The macOS helper must:
   - accept an `AppHandle` or main-thread dispatcher explicitly,
   - return a structured `Result<(), AppError>`,
   - acknowledge scheduling success/failure,
   - use a bounded timeout for scheduling/completion,
   - avoid holding locks across main-thread callbacks,
   - chunk long text using the current `TYPE_CHUNK_SIZE`,
   - preserve Shift+Return line break behavior.
4. Use the smallest explicit-context design:
   - pass `&tauri::AppHandle` into `output::output_with_fallback(...)`,
   - pass the same handle into `output::create_output(...)`,
   - let `KeyboardOutput` clone/store the handle only under `#[cfg(target_os = "macos")]`,
   - keep `ClipboardOutput` unchanged.
5. Avoid a global dispatcher or singleton for this release.

Manual verification:

1. macOS 26.5 or affected version: keyboard output no longer crashes.
2. macOS: long multi-line output is typed with correct line breaks.
3. macOS: output failure falls back to clipboard through `output_with_fallback()`.
4. Windows: keyboard output still works through existing path.
5. Ubuntu/X11: keyboard output still works through existing path.

Acceptance criteria:

1. macOS no longer creates Enigo off-main-thread for keyboard output.
2. Windows/Linux code path remains unchanged.
3. Failure fallback to clipboard still works.

### Phase 6: Deepgram Batch PR Decision

This phase is optional for the platform patch unless Deepgram truncation is currently release-critical.

Known issue in #44:

1. Batch `recv_transcript()` returns `Ok(None)` immediately.
2. The pipeline waits on providers using `tokio::select!`.
3. An immediately-ready `Ok(None)` branch can spin or starve real work.

Required behavior if included:

1. Rebase/rebuild against current main.
2. For file/batch providers, make `recv_transcript()` await forever or await a cancellation-aware pending future after upload finalization is handled elsewhere.
3. Prefer reusing shared WAV-building logic where possible rather than duplicating fragile audio serialization.
4. Preserve Custom Whisper and hosted STT provider creation behavior from main.
5. Add a regression test that proves batch provider receive does not hot-loop.

Acceptance criteria if included:

1. Deepgram no longer truncates due to streaming finalization behavior.
2. Pipeline CPU does not spike after recording stop.
3. Custom Whisper and other STT providers still pass existing tests.

### Phase 7: Dependency and Security Hardening

Current risk:

1. `npm audit --audit-level=high` fails.
2. #33 cannot be merged because of ESLint 10 peer dependency conflicts.
3. CI audit currently uses `continue-on-error: true`, so high/critical findings are visible but not blocking.

Required behavior:

1. Do not merge #33.
2. Apply targeted dependency updates instead:
   - update `vitest` to a patched version at or above the audit-safe range,
   - update `vite` to a patched version beyond the affected range,
   - update `better-auth` to the audit-safe range,
   - keep `eslint` on v9 unless all plugins explicitly support v10.
3. Run `npm ci` after lockfile changes.
4. Run the full frontend suite:
   - `npx tsc --noEmit`
   - `npm run lint`
   - `npm run format:check`
   - `npm test`
   - `npm run build`
5. Re-run `npm audit --audit-level=high`.
6. If an audit finding cannot be fixed without a large upgrade, document:
   - package,
   - severity,
   - reachable or not reachable,
   - why it is accepted for this release,
   - target follow-up version.
7. After audit is clean or exceptions are documented, consider changing the CI high audit step from `continue-on-error: true` to blocking in a separate focused PR.

Acceptance criteria:

1. No dependency install conflict.
2. No high/critical audit finding remains, or each has an explicit release exception.
3. No broad tooling upgrade destabilizes the release.

### Phase 8: CI and Release Workflow

Current workflow facts:

1. Release workflow builds Windows, macOS arm64, macOS x64, and Ubuntu 22.04.
2. Release workflow can be triggered by tag push or `workflow_dispatch` with tag input.
3. Release workflow uses `GITHUB_TOKEN`.
4. GitHub-hosted Actions billing is tied to the repository owner/account/organization, not merely the local `gh` login.

Important billing conclusion:

Switching the local GitHub CLI account alone will not fix GitHub-hosted Actions if the repository owner's billing is blocked. To publish through GitHub-hosted runners, the repository owner billing must be healthy, or the build must run under a repository/account/organization with working Actions entitlement.

Release paths:

1. Preferred path after billing is fixed:
   - push final branch,
   - merge after CI,
   - create tag `v0.1.35` or the chosen next version,
   - run Release workflow,
   - verify draft release assets,
   - publish draft.
2. Billing workaround path:
   - use an alternate account only if it has admin/write access and a repository context where Actions can run,
   - transfer/fork temporarily only if acceptable for project ownership and release provenance,
   - or use a self-hosted runner/local builds and upload assets to the original release with a token that has `contents:write`.
3. Re-publish with current account after billing is fixed:
   - do not create a second tag with the same version,
   - keep or recreate the same tag intentionally,
   - delete or replace draft assets if they were built by a workaround path,
   - rerun the Release workflow from the original repository owner context,
   - compare asset names, checksums, and app version metadata before publishing.

Release asset acceptance criteria:

1. Windows installer is present.
2. macOS arm64 artifact is present.
3. macOS x64 artifact is present.
4. Linux AppImage/deb/rpm artifacts expected by Tauri action are present.
5. About/version UI displays the release tag through `VITE_APP_VERSION`.
6. `src-tauri/tauri.conf.json`, `package.json`, and `src-tauri/Cargo.toml` are synced by workflow to the tag version.

## Platform Verification Matrix

| Platform                 | Required for release | Test focus                                                                                                                                      |
| ------------------------ | -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows 11 x64           | Smoke required       | Ensure no regression: install, launch, hotkey, selected text, keyboard output, clipboard fallback, STT/LLM happy path.                          |
| macOS Apple Silicon      | Required             | Accessibility permission, selected text with English and Chinese IME, keyboard output, clipboard fallback, long multiline output, app relaunch. |
| macOS Intel              | Strongly recommended | Same as Apple Silicon, because release builds both architectures.                                                                               |
| Ubuntu 22.04 MATE/X11    | Required             | #48 reproduction, app launch, tray, recording, selected text, keyboard output, no XCB thread crash.                                             |
| Ubuntu Wayland           | Recommended          | Launch stability, NVIDIA DMA-BUF workaround, keyboard unavailable/fallback messaging.                                                           |
| Ubuntu without `xdotool` | Recommended          | Existing guardrail still reports/falls back cleanly.                                                                                            |

## Final Release Checklist

Code and tests:

1. `git status --short` shows only intentional files.
2. `npm ci` passes.
3. `npx tsc --noEmit` passes.
4. `npm run lint` passes.
5. `npm run format:check` passes.
6. `npm test` passes.
7. `npm run build` passes.
8. `npm audit --audit-level=high` passes or has documented exceptions.
9. `cargo fmt --check --manifest-path src-tauri/Cargo.toml` passes.
10. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` passes.
11. `cargo test --manifest-path src-tauri/Cargo.toml` passes.
12. Release workflow or equivalent local/self-hosted build produces all target assets.

Issue and PR hygiene:

1. #48 references the Linux/X11 fix and asks reporter to verify.
2. #55 is merged or superseded by the focused Linux fix PR.
3. #42/#43 are superseded by focused macOS fix PR(s).
4. #44 is either fixed separately or explicitly deferred.
5. #47 is closed/superseded.
6. #45 is closed/superseded as too broad.
7. #33 is closed/superseded due to dependency conflict.
8. #50 has a note explaining what was already fixed and what regression coverage was added or deferred.
9. #51 has a clarification reply distinguishing STT local/custom URL from literal TTS.
10. #28 remains deferred as feature work.

Manual sign-off:

1. Windows smoke test signed off.
2. macOS Apple Silicon signed off.
3. macOS Intel signed off or explicitly marked not available.
4. Ubuntu 22.04 MATE/X11 signed off.
5. Release artifacts installed and launched on at least one clean machine per OS family.

## Risk Register

| Risk                                                        | Impact                       | Mitigation                                                                                                                   |
| ----------------------------------------------------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Linux `XInitThreads` is called too late                     | #48 remains unfixed          | Call helper at the first line of `run()` before tracing/Tauri/plugins.                                                       |
| macOS main-thread helper deadlocks                          | App freeze during output     | Use oneshot acknowledgement and timeout; never block async code on unbounded `recv()`.                                       |
| macOS fix accidentally changes Windows/Linux Enigo behavior | Windows/Linux regression     | Use `#[cfg(target_os = "macos")]` boundaries and smoke-test Windows/Linux.                                                   |
| Dependency bump pulls ESLint 10 again                       | `npm ci` fails               | Keep ESLint 9 unless peer ranges prove support.                                                                              |
| Billing workaround creates untrusted artifacts              | Release provenance confusion | Prefer original repository workflow after billing fix; otherwise document build machine, commit, checksums, and token scope. |
| Deepgram batch work expands release scope                   | Delayed platform fix         | Defer #44 unless it is confirmed as release-critical.                                                                        |

## Recommended Execution Order

1. Land Linux #48/#55 targeted fix.
2. Land macOS Accessibility permission fix.
3. Land macOS selected text Cmd+C fix.
4. Land macOS keyboard output main-thread safety fix.
5. Run dependency audit targeted bumps.
6. Run full CI and platform manual matrix.
7. Decide whether Deepgram #44 is in or out for the patch.
8. Prepare release tag and draft release through the billing-safe path.
9. Once current account billing is healthy, rebuild/re-publish from the original repository context.
