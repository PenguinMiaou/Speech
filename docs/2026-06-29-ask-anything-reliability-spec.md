# Ask Anything Reliability Design Spec

Date: 2026-06-29
Status: implemented; automated release checks passed on 2026-06-30; post-release platform smoke test still required
Scope: OpenTypeless desktop Ask Anything + talkmore cloud proxy

## Executive Summary

Ask Anything 不能只修一个超时或弹窗问题。它是一条跨桌面端、麦克风、STT、LLM、云端计费、结果窗口的链路。发布前必须保证每个失败点都有明确的用户反馈，并且失败时不会继续调用后续付费阶段。

当前根因判断：

1. 结果弹窗丢失是真问题。快捷键路径在 Rust 里打开 Ask 窗口后立即发 `ask:result`，如果前端监听还没挂上，事件会丢，用户会看到胶囊完成但没有结果弹窗内容。
2. 无声音长时间 thinking 是真问题。当前 Ask STT finalize 最长等 `120s`，没有语音或音频流无法结束时，用户会长时间卡在 thinking。
3. STT、LLM、云端 quota、网络错误必须原样返回给用户。不能把 STT quota/auth/service error 覆盖成 `No speech detected`。
4. 空语音必须在桌面端止住：没有有效 transcript 时不能调用 `/api/proxy/ask`，因此不能产生 Ask LLM 计费。
5. 云端已有部分计费保护：AppSumo cloud words 模式下，`/api/proxy/stt` 空文本会 release reservation，`/api/proxy/ask` 只有被调用并成功回答后才 settle。但桌面端仍必须阻止空语音进入 ask 阶段。

## User Contract

用户触发 Ask Anything 后，只会看到这些状态：

1. Recording: 胶囊显示正在录音。
2. Thinking: 用户停止录音后，开始处理 STT 和 LLM。
3. Result popup: 成功时弹窗只显示最终答案，不显示输入框、不显示发送按钮、不显示上下文。
4. Error popup: 失败时弹窗只显示一条可理解错误，不显示输入框、不显示发送按钮。

失败不能静默。只要链路中任一阶段失败，就必须让用户看到错误，并且恢复 idle。

## End-to-End Chain

### Desktop Hotkey Flow

1. User presses Ask hotkey.
2. `src-tauri/src/hotkey.rs` calls `start_ask_dictation`.
3. `start_ask_dictation` loads config, validates STT auth/config, connects STT provider, starts audio capture, emits `PipelineState::Recording`.
4. User presses Ask hotkey again.
5. `stop_ask_dictation` stops audio capture, emits `PipelineState::Polishing`, waits for STT finalize, validates transcript, then calls LLM.
6. On success, Rust returns `AskDictationResult { question, answer }`.
7. Hotkey handler opens/focuses the `ask` window and delivers the result.
8. `src/components/AskPanel/AskPanel.tsx` renders answer-only popup content.

### Cloud STT Flow

1. Desktop cloud STT uses the session token as bearer token.
2. Cloud STT request includes operation metadata:
   - `operationId`
   - `stageKey = <operationId>:stt`
   - `requestType`
   - `clientVersion`
3. talkmore `/api/proxy/stt` authenticates, reserves quota, calls Groq STT.
4. If STT provider fails, reservation is released and an error is returned.
5. In AppSumo cloud words mode, empty transcript releases reservation.
6. In legacy dual-meter mode, STT quota is adjusted to actual parsed duration after a successful STT call.

### Cloud Ask LLM Flow

1. Desktop calls `/api/proxy/ask` only after a non-empty validated transcript exists.
2. Request includes operation metadata:
   - `operationId`
   - `stageKey = <operationId>:ask`
   - `requestType = ask_anything`
   - `clientVersion`
3. talkmore reserves ask stage quota before calling OpenRouter.
4. On OpenRouter failure, reservation is released.
5. On success, quota settles using question + answer cloud words and LLM token metadata.

## Failure Matrix

| Stage       | Failure                                        | Required user feedback                                                 | Continue to next stage? | Billing requirement                                          |
| ----------- | ---------------------------------------------- | ---------------------------------------------------------------------- | ----------------------- | ------------------------------------------------------------ |
| Start       | Ask already processing                         | Ignore duplicate hotkey                                                | No                      | No new billing                                               |
| Config      | Config load fails                              | `Could not load settings. Please retry.`                               | No                      | No billing                                                   |
| Auth        | Cloud selected but no session token            | `Sign in to use Cloud Ask, or switch to BYOK.`                         | No                      | No billing                                                   |
| STT config  | Provider requires key but key missing          | `Configure speech recognition before using Ask.`                       | No                      | No billing                                                   |
| STT connect | Provider connect fails                         | Provider error text, sanitized                                         | No                      | No billing                                                   |
| Audio       | No input device                                | `Microphone unavailable. Check your input device.`                     | No                      | No billing                                                   |
| Audio       | Permission denied                              | `Microphone permission is required.`                                   | No                      | No billing                                                   |
| Recording   | Send audio fails                               | STT error text, sanitized                                              | No                      | Release any reservation                                      |
| STT         | Provider returns auth/quota/service error      | Exact mapped error: auth, quota, or service                            | No                      | Release reservation on server error                          |
| STT         | Empty transcript                               | `No speech detected. Please try again.`                                | No                      | No Ask LLM billing; cloud words STT releases in AppSumo path |
| STT         | Finalize timeout with no transcript            | `No speech detected. Please try again.`                                | No                      | No Ask LLM billing                                           |
| STT         | Finalize timeout after final transcript exists | Continue with collected final transcript                               | Yes                     | Normal billing                                               |
| Transcript  | Over 500 chars                                 | `Question is too long.`                                                | No                      | No Ask LLM billing                                           |
| LLM config  | No BYOK LLM and no cloud token                 | `Sign in or configure an LLM provider.`                                | No                      | No LLM billing                                               |
| LLM cloud   | Quota exceeded                                 | `Cloud words used up. Please switch to BYOK mode or wait until reset.` | No                      | Release failed reservation                                   |
| LLM cloud   | OpenRouter/service error                       | `Ask service error. Please try again.`                                 | No                      | Release failed reservation                                   |
| Popup       | Native result event is missed                  | Frontend fetches pending result once on mount                          | N/A                     | No extra billing                                             |
| Popup       | Native error event is missed                   | Frontend fetches pending error once on mount                           | N/A                     | No extra billing                                             |

## Error Handling Requirements

Desktop must use one canonical Ask message shape:

```ts
type AskPopupMessage =
  | { kind: 'result'; payload: { question: string; answer: string } }
  | { kind: 'error'; payload: string }
```

Rules:

1. Hotkey path stores the latest result/error in Rust before showing the Ask window.
2. Ask window listens for `ask:result` and `ask:error`.
3. Ask window also calls `take_pending_ask_message` once after listeners are ready.
4. `take_pending_ask_message` consumes the message exactly once.
5. Event delivery and pending-message delivery must both render identical answer-only/error-only popup UI.
6. On any error, desktop emits idle state and clears busy/recording state.
7. STT errors must not be overwritten by later empty-transcript validation.
8. Empty transcript must never call `answer_question`.
9. Error strings shown to users must be sanitized and bounded.

## Billing Requirements

Ask Anything has two billable phases in cloud mode:

1. STT phase: speech to text.
2. Ask phase: LLM answer.

Hard rules:

1. No valid transcript means no `/api/proxy/ask` request.
2. STT start/connect/audio failures must not call LLM.
3. STT provider auth/quota/service failures must not call LLM.
4. Empty transcript must not call LLM.
5. `question` validation must run before cloud ask reservation.
6. Cloud ask output stays capped at `ASK_OUTPUT_TOKEN_LIMIT = 80`.
7. talkmore AppSumo cloud words path must release reservations on provider failure.
8. talkmore AppSumo cloud words path must release STT reservation on empty transcript.
9. talkmore ask path must settle LLM tokens and provider/model only after success.
10. Repeated popup retries must not re-call STT or LLM.

## Current Code Evidence

Desktop:

1. `src-tauri/src/commands/ask.rs` validates empty questions before LLM.
2. `src-tauri/src/commands/ask.rs` currently waits too long for STT finalize in released builds and needs a shorter bounded wait.
3. `src-tauri/src/hotkey.rs` currently emits result/error immediately after showing the Ask window; this can lose events.
4. `src/components/AskPanel/AskPanel.tsx` renders non-embedded popup as result/error-only, but needs pending-message recovery for missed native events.

Cloud:

1. `talkmore/src/app/api/proxy/stt/route.ts` rejects missing auth before paid STT API calls.
2. `talkmore/src/app/api/proxy/stt/route.ts` releases AppSumo cloud reservation when STT returns empty text.
3. `talkmore/src/app/api/proxy/ask/route.ts` caps output at `80` tokens.
4. `talkmore/src/app/api/proxy/ask/route.ts` releases reservation when OpenRouter fails.
5. `talkmore/src/lib/cloud-quota.ts` records cloud STT billable seconds and LLM tokens during settlement.

## Required Implementation Tasks

### Task 1: Result/Error Popup Delivery

Files:

1. `src-tauri/src/commands/ask.rs`
2. `src-tauri/src/hotkey.rs`
3. `src-tauri/src/lib.rs`
4. `src/lib/tauri.ts`
5. `src/components/AskPanel/AskPanel.tsx`
6. `src/components/AskPanel/__tests__/AskPanel.test.tsx`
7. `src/lib/__tests__/tauri-ask.test.ts`

Acceptance:

1. Add pending one-shot Ask result/error state in Rust.
2. Register `take_pending_ask_message`.
3. Frontend fetches pending message after listeners attach.
4. Tests cover missed native result event.
5. Tests cover Tauri wrapper invocation.

Commands:

```bash
npm test -- --run src/components/AskPanel/__tests__/AskPanel.test.tsx src/lib/__tests__/tauri-ask.test.ts
cargo test ask::tests --manifest-path src-tauri/Cargo.toml
```

### Task 2: No-Speech and Timeout Handling

Files:

1. `src-tauri/src/commands/ask.rs`
2. `src-tauri/src/audio/capture.rs`
3. `src-tauri/src/stt/*.rs` as needed after provider-specific review

Acceptance:

1. Stop path uses a short bounded STT finalize wait.
2. No transcript returns `No speech detected. Please try again.`
3. Empty transcript never calls LLM.
4. Existing STT provider error wins over no-speech.
5. If a final transcript already exists when finalize times out, continue with that transcript.
6. Manual Windows test covers silence and real speech.
7. Manual macOS test covers silence and real speech.

Commands:

```bash
cargo test ask::tests --manifest-path src-tauri/Cargo.toml
npm test -- --run src/components/AskPanel/__tests__/AskPanel.test.tsx
```

### Task 3: Error Mapping Audit

Files:

1. `src-tauri/src/commands/ask.rs`
2. `src-tauri/src/stt/cloud.rs`
3. `src-tauri/src/stt/config.rs`
4. `src/lib/i18n` locale files if Ask-specific localized errors are added

Acceptance:

1. Missing cloud session has a cloud-specific message, not generic API-key copy.
2. Microphone permission/device errors become user-readable messages.
3. STT quota/auth/network/service errors remain visible and are not replaced by no-speech.
4. LLM quota/auth/network/service errors remain visible.
5. Errors are short enough for the Ask popup.

### Task 4: Cloud Billing Regression Tests

Files:

1. `talkmore/src/app/api/proxy/stt/route.ts`
2. `talkmore/src/app/api/proxy/ask/route.ts`
3. Existing or new talkmore API tests

Acceptance:

1. Empty AppSumo STT response releases cloud reservation.
2. STT provider error releases cloud reservation.
3. Ask provider error releases cloud reservation.
4. Successful Ask settles cloud words and LLM tokens.
5. Ask endpoint rejects empty question before reservation.

## Manual Release Gate

Before release, verify all rows below:

| Platform | Case                                    | Expected result                        |
| -------- | --------------------------------------- | -------------------------------------- |
| Windows  | Press Ask, say nothing, stop            | Error popup appears; no LLM call       |
| Windows  | Press Ask, speak one question, stop     | Answer-only popup appears              |
| Windows  | Disable/deny mic                        | Error popup appears                    |
| Windows  | Cloud quota exhausted test account      | Quota error popup appears              |
| macOS    | Press Ask, say nothing, stop            | Error popup appears; no LLM call       |
| macOS    | Press Ask, speak one question, stop     | Answer-only popup appears              |
| macOS    | Ask window was not loaded before result | Pending result still appears           |
| Linux    | Press Ask, speak one question, stop     | Answer-only popup appears              |
| Cloud    | Empty transcript path                   | STT reservation released; no ask stage |
| Cloud    | STT success + Ask fail                  | STT settled; ask reservation released  |

## Release Decision

This feature is release-ready for CI packaging only when:

1. Unit tests for pending popup and no-speech pass.
2. Desktop build passes.
3. talkmore build passes.
4. Cloud quota tests or direct deployment logs confirm no empty-speech LLM billing.
5. No new release is published from partial local changes.

Post-release smoke validation must still cover:

1. Windows manual Ask tests pass.
2. macOS manual Ask tests pass.
3. Published artifacts install, sign, and launch on target platforms.
