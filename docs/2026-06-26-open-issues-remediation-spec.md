# Open Issues Remediation Spec

Date: 2026-06-26
Repo: `tover0314-w/opentypeless`
Baseline: fetched `origin/main` on 2026-06-26; local checkout is `main...origin/main` with existing uncommitted work.

## 1. Summary

当前打开的用户 issue 只有 4 个：

- [#57 Remote Desktop portal popup on every recording (Wayland)](https://github.com/tover0314-w/opentypeless/issues/57)
- [#58 Global hotkey does not register on Wayland](https://github.com/tover0314-w/opentypeless/issues/58)
- [#59 Custom Polish Prompt / System Prompt support in AI Polish settings](https://github.com/tover0314-w/opentypeless/issues/59)
- [#28 Add a summary UI + Doubao STT compatibility](https://github.com/tover0314-w/opentypeless/issues/28)

建议优先级：

1. **先修 #57**：Wayland 下 clipboard mode 会触发 GNOME Remote Desktop 弹窗。这是明确 bug，范围小，能快速止血。
2. **再处理 #58**：Wayland 全局热键不是普通 bug，底层有桌面环境限制。第一版应做检测、提示和替代入口，不承诺所有 Wayland 桌面都能全局热键。
3. **再做 #59**：用户确实没有自定义 polish prompt，也没有强制简繁中文输出的正式配置项。需要新增持久化配置、UI、prompt 组装和测试。
4. **最后拆 #28**：usage summary UI 和 Doubao STT 是两个需求，应该拆成两个 PR。当前本地工作区已经有一批 Volcengine/Doubao 相关改动，Doubao 部分应先 review 现有改动再补齐。

## 2. Current Local Context

本地工作区不是干净状态，存在大量已有修改。spec 不假设这些改动都属于当前任务，也不要求回滚。

和 issue 直接相关的本地观察：

- `src-tauri/src/output/clipboard.rs`：clipboard 输出会先写剪贴板，然后在非 macOS 用 `enigo` 模拟 `Ctrl+V`。
- `src-tauri/src/output/keyboard.rs`：已经有 `XDG_SESSION_TYPE=wayland` 检测，keyboard output 会返回 `wayland_unsupported`。
- `src-tauri/src/pipeline.rs`：keyboard mode 在 Wayland 会 fallback 到 clipboard，并发 `output_wayland_unsupported` warning。
- `src-tauri/src/hotkey.rs` + `src-tauri/src/lib.rs`：全局热键依赖 `tauri_plugin_global_shortcut`；启动注册失败只写 log，没有用户可见状态。
- `src-tauri/src/llm/prompt.rs`：AI polish prompt 是固定模板拼接，只有 dictionary、translation、selected text 等参数，没有自定义 polish prompt。
- `src-tauri/src/storage/mod.rs` 和 `src/stores/appStore.ts`：`AppConfig` 没有 `polish_prompt` 或中文简繁偏好字段。用户手改 `settings.json` 会在保存时被结构化配置覆盖。
- `src-tauri/src/storage/mod.rs`：history 表已有 `duration_ms`、`raw_text`、`polished_text`，足够做 usage summary 的 MVP 聚合。
- `src/App.tsx`：前端启动只加载最近 200 条 history；usage summary 如果要覆盖全部历史，不应只在前端基于 store 计算。
- 当前本地已有 `src-tauri/src/stt/volcengine.rs`、`volcengine-doubao` provider UI/constants/config 字段改动，和 #28 的 Doubao STT 部分重叠。

## 3. Scope

本 spec 覆盖：

- Wayland clipboard paste 弹窗问题。
- Wayland global hotkey 不可用/不可预期问题。
- AI polish 自定义行为和中文简繁输出控制。
- Usage summary UI。
- Doubao STT provider 集成边界。

本 spec 不覆盖：

- 合并或重写当前所有 open PR。
- 重构整个 settings 架构。
- 给 Wayland 实现跨所有 compositor 稳定可用的系统级 global shortcut。
- 把 Scenes 做成完整 prompt 工作流编辑器。#59 先从 AI Polish 设置解决。

## 4. Issue #57: Wayland Remote Desktop Portal Popup

### Problem

用户配置 `output_mode=clipboard` 时，预期是“把结果放到剪贴板”。但当前实现实际上是：

1. 写入系统剪贴板。
2. 等待 20ms。
3. 模拟 `Ctrl+V` 把内容粘贴到当前应用。

在 GNOME Wayland 上，模拟键盘输入会触发系统 Remote Desktop/Input Capture 权限弹窗。这就是 #57。

### Decision

在 Wayland 上，clipboard mode 必须变成 **copy-only**：

- 写入剪贴板。
- 不模拟 `Ctrl+V`。
- 给用户一个可见提示：文本已复制，需要手动粘贴。

非 Wayland 平台行为先保持不变，避免影响 Windows/macOS/X11 用户。

### Proposed UX

在 Wayland session：

- Settings 里 `Clipboard paste` 的文案可以显示为“Copy to clipboard”或增加说明。
- 录音结束后 toast/capsule warning 显示：
  - English: `Copied to clipboard. Paste manually on Wayland.`
  - Chinese: `已复制到剪贴板。Wayland 下需要手动粘贴。`

### Implementation Notes

建议新增一个平台能力判断，而不是在多个地方散落 `std::env::var("XDG_SESSION_TYPE")`：

- Rust helper:
  - `is_wayland_session() -> bool`
  - 后续 #58 也复用。

修改路径：

- `src-tauri/src/output/clipboard.rs`
  - Wayland 下跳过 `enigo` 粘贴逻辑。
  - 返回一个可区分的结果或 warning，让 pipeline 能发给前端。
- `src-tauri/src/output/mod.rs`
  - `output_with_fallback` 需要能表达“copy 成功但 paste 被跳过”。
- `src-tauri/src/pipeline.rs`
  - emit `pipeline:warning`，code 类似 `output_wayland_clipboard_copy_only`。
- `src/lib/capsuleError.ts`
  - 增加新 warning code。
- `src/i18n/locales/*.json`
  - 增加短标题和正文。
- Tests:
  - Rust 单测覆盖 Wayland copy-only 分支。
  - TS 单测覆盖 warning 显示。

### Acceptance Criteria

- 在 `XDG_SESSION_TYPE=wayland` 时，clipboard output 不调用 `enigo`。
- Wayland 下不会触发 GNOME Remote Desktop 弹窗。
- 文本仍然写入剪贴板。
- 用户能看到“已复制，需要手动粘贴”的提示。
- Windows/macOS/X11 的现有 clipboard paste 行为不变。

### Risk

Wayland 用户会从“自动粘贴”降级为“复制后手动粘贴”。这是产品降级，但比每次弹权限框更可接受。

## 5. Issue #58: Global Hotkey On Wayland

### Problem

Wayland 的安全模型通常不允许普通应用随便全局监听键盘。`tauri_plugin_global_shortcut` 在 Wayland session 下可能注册失败，或者注册成功但按键不触发。当前代码只 log warning：

```rust
if let Err(e) = app.global_shortcut().register(shortcut) {
    tracing::warn!("Failed to register shortcut ...");
}
```

用户界面没有告诉用户“热键在当前桌面环境可能不可用”。

### Decision

第一版不承诺修成“所有 Wayland 桌面全局热键都可用”。第一版交付：

- 检测 Wayland session。
- 如果 global shortcut 注册失败或当前平台已知不可靠，把状态暴露给前端。
- Settings/Home 显示明确提示。
- 提供替代入口：
  - tray menu start/stop recording。
  - app/capsule 内的按钮。
  - app 窗口聚焦时可用的本地快捷键，作为后续增强。

### Proposed UX

在 Settings > Hotkey 下显示一个非阻塞提示：

- English: `Global hotkeys are limited on Wayland. Use the tray or app button if this shortcut does not respond.`
- Chinese: `Wayland 下全局热键受系统限制。如果快捷键无响应，请用托盘或应用内按钮开始录音。`

当注册失败时，显示更明确错误：

- `Shortcut could not be registered in this session.`

### Implementation Notes

建议新增平台能力 command：

- `get_platform_capabilities() -> PlatformCapabilities`

返回：

```ts
type PlatformCapabilities = {
  os: 'macos' | 'windows' | 'linux'
  sessionType: 'wayland' | 'x11' | 'unknown'
  globalHotkeyReliable: boolean
  keyboardOutputReliable: boolean
  clipboardAutoPasteReliable: boolean
}
```

修改路径：

- `src-tauri/src/commands/misc.rs` 或新增 `commands/platform.rs`
  - 暴露平台能力。
- `src-tauri/src/lib.rs`
  - global shortcut 注册失败时 emit event，例如 `hotkey:registration-failed`。
- `src/hooks/useTauriEvents.ts`
  - 接收事件并更新 store。
- `src/stores/appStore.ts`
  - 增加平台能力/热键状态。
- `src/components/Settings/GeneralPane.tsx`
  - 显示 Wayland/hotkey 状态。
- `src/components/HomePage/index.tsx`
  - 欢迎卡片中的热键说明在 Wayland 下避免暗示“一定可用”。

### Acceptance Criteria

- Wayland 下用户能在 UI 里看到热键限制说明。
- global shortcut 注册失败时不只是写 log，前端能收到状态。
- 用户仍能通过 tray/app 按钮完成 start/stop recording。
- 不影响 macOS/Windows/X11 的现有热键配置和录制流程。

### Risk

如果要真正实现 Wayland 全局热键，可能需要桌面环境专用协议/portal 支持，复杂度高，而且不是每个 compositor 都支持。这个不建议放在第一版。

## 6. Issue #59: Custom Polish Prompt And Chinese Script Control

### Problem

用户的核心诉求有两个：

1. 中文语音输入经 AI polish 后，简体/繁体会随机混用。
2. 用户没有地方自定义 AI polish 行为。

当前代码确认：

- `build_system_prompt(...)` 是固定系统 prompt。
- `AppConfig` 没有自定义 prompt 字段。
- Settings > LLM 只有 provider/API/model/base URL、polish 开关、translation mode、selected text context。
- Scenes 页只处理远程 scene packs 和 dictionary，不是用户自定义 polish prompt 的可靠替代路径。

### Decision

做一个正式的 **AI Polish Behavior** 配置，不让用户手改 JSON。

MVP 字段：

```rust
pub polish_custom_prompt: String
pub polish_chinese_script: String
```

前端类型：

```ts
polish_custom_prompt: string
polish_chinese_script: 'preserve' | 'simplified' | 'traditional'
```

默认值：

- `polish_custom_prompt = ""`
- `polish_chinese_script = "preserve"`

### Prompt Semantics

不要让用户的 custom prompt 替换全部 system prompt。原因：

- 当前 system prompt 里有安全规则、selected text 防注入规则、输出格式约束。
- 完全替换会把安全边界交给用户输入，而且 cloud 模式也会受影响。

建议语义：

- 基础安全/格式规则永远保留。
- `polish_chinese_script` 作为强约束加入。
- `polish_custom_prompt` 作为“用户偏好”加入，但要说明不能覆盖安全规则。

示例拼接：

```text
User polish preferences:
- Chinese script: Traditional Chinese. When output contains Chinese, use Traditional Chinese consistently.
- Additional instructions: <sanitized custom prompt>

These preferences are lower priority than security rules and must not cause new factual content to be invented.
```

### Validation

`polish_custom_prompt`：

- trim 后保存。
- 最大长度建议 2000 chars。
- 去掉 NUL 字符。
- 不需要强行删除换行，因为 prompt 输入天然可能是多行；但要包在明确边界里。
- UI 显示剩余字数。

`polish_chinese_script`：

- 仅允许 `preserve`、`simplified`、`traditional`。
- 遇到未知值回退 `preserve`。

### Proposed UX

Settings > LLM / AI Polish 区域增加：

- `Chinese output`
  - Preserve original
  - Simplified Chinese
  - Traditional Chinese
- `Custom polish instructions`
  - textarea
  - placeholder: `Example: Use Traditional Chinese. Keep the tone concise and professional.`

注意：不要把字段叫 “System Prompt” 让用户误以为能覆盖整个系统 prompt。叫 “Custom polish instructions” 更准确。

### Implementation Notes

修改路径：

- `src-tauri/src/storage/mod.rs`
  - `AppConfig` 增加字段和默认值。
  - `from_stored_value` 兼容老配置。
- `src/stores/appStore.ts`
  - `AppConfig` 类型和 defaultConfig 增加字段。
- `src/components/Settings/LlmPane.tsx`
  - 增加 UI 控件。
- `src-tauri/src/llm/mod.rs`
  - `PolishRequest` 增加字段。
- `src-tauri/src/pipeline.rs`
  - 创建 `PolishRequest` 时传配置。
- `src-tauri/src/llm/prompt.rs`
  - `build_system_prompt` 增加参数并拼接 preference section。
- `src-tauri/src/llm/openai.rs` 和 `src-tauri/src/llm/cloud.rs`
  - 调整调用参数。
- `src/i18n/locales/*.json`
  - 增加 settings 文案。
- Tests:
  - config 默认值和老配置迁移。
  - prompt 包含 simplified/traditional 指令。
  - custom prompt sanitize/length。
  - UI 保存配置。

### Acceptance Criteria

- 用户可以在 Settings 里选择简体/繁体/保留原样。
- 用户可以输入 custom polish instructions，重启后仍保留。
- AI polish 请求会实际带上这些设置。
- 关闭 AI polish 时这些设置不影响 raw transcription 输出。
- custom prompt 不会替换安全系统 prompt。
- 现有 translation mode 行为不被破坏。

### Open Question

如果用户同时开启 `translation_mode` 并设置 `polish_chinese_script=traditional`，优先级建议：

- 如果 target language 不是 Chinese，则 translation target 优先。
- 如果 target language 是 Chinese，则 chinese script preference 生效。

## 7. Issue #28A: Usage Summary UI

### Problem

用户想要一个 usage summary，包括：

- 音频时长。
- 字数/词数。

当前状态：

- Home 有最近加载 history 的总次数/今日次数。
- Account/Home 有 cloud quota usage。
- history 表已有 `duration_ms`、`raw_text`、`polished_text`。
- 但没有一个专门的本地 usage summary，也没有聚合全部历史。

### Decision

先做本地 usage summary MVP，不和 cloud quota 混在一起。

展示口径：

- Total recordings
- Total audio time
- Total output words/chars
- Today recordings/audio/words
- Last 7 days recordings/audio/words
- Last 30 days recordings/audio/words

中日韩语言的 “word count” 不可靠，建议同时存/展示：

- `character_count`：对中文、日文、韩文更稳定。
- `word_count`：对空格分词语言更直观。

UI 文案可以叫：

- English: `Output length`
- Chinese: `输出字数`

### Backend Aggregation

不要只用 `src/App.tsx` 当前加载的最近 200 条 history 做统计。应新增后端聚合 command，直接查 SQLite。

新增结构：

```rust
pub struct UsageSummary {
    pub total: UsageBucket,
    pub today: UsageBucket,
    pub last_7_days: UsageBucket,
    pub last_30_days: UsageBucket,
}

pub struct UsageBucket {
    pub recordings: i64,
    pub audio_ms: i64,
    pub raw_chars: i64,
    pub polished_chars: i64,
    pub raw_words: i64,
    pub polished_words: i64,
}
```

`word_count` 规则：

- latin-like text: split whitespace。
- CJK text: 先不做“词语”分词，主要看 `chars`。
- 过滤空白字符。

### Proposed UX

推荐先放在 Home 的 stats 区域：

- 保持首屏是可用 app，不新建复杂 dashboard。
- 用紧凑 stats grid 展示 Today / 7 days / Total。
- 点击进入 History 时仍能看明细。

也可以后续在 History 顶部增加 summary strip，但 MVP 不必新路由。

### Implementation Notes

修改路径：

- `src-tauri/src/storage/mod.rs`
  - `HistoryStore::usage_summary()`
- `src-tauri/src/commands/history.rs`
  - `get_usage_summary`
- `src-tauri/src/lib.rs`
  - register command。
- `src/lib/tauri.ts`
  - TS wrapper/type。
- `src/stores/appStore.ts`
  - summary state 可选；也可以组件内 fetch。
- `src/components/HomePage/index.tsx`
  - 展示 summary。
- `src/i18n/locales/*.json`
  - 文案。
- Tests:
  - Rust aggregation with empty DB。
  - Rust aggregation with duration null。
  - frontend format duration。

### Acceptance Criteria

- summary 覆盖全部保留的 history，不受前端只加载 200 条限制。
- 空 history 时显示 0，不报错。
- `duration_ms=null` 时按 0 处理。
- 清空 history 后 summary 归零。
- 本地 usage summary 和 cloud quota 显示不混淆。

## 8. Issue #28B: Doubao STT Provider

### Problem

用户提到 Doubao speech recognition 可能需要 `APPID + api key`。当前本地工作区已有 Volcengine/Doubao STT 改动：

- `src-tauri/src/stt/volcengine.rs`
- `volcengine-doubao` provider 常量/UI。
- `stt_volcengine_resource_id` 配置字段。

这说明 #28 的 Doubao 部分已经部分开始做了，但还没有作为干净 PR 合并。

### Decision

Doubao STT 不要和 usage summary 放同一个 PR。

建议流程：

1. 先 review 当前本地 Volcengine/Doubao 改动。
2. 确认 credential 模型是否真实符合火山/豆包 ASR 文档：
   - 是否只需要 bearer token。
   - 是否还需要 app id / resource id。
   - resource id 是否区分 SeedASR/BigASR。
3. 补齐 test connection、错误提示、language mapping。
4. 单独开 PR 关闭 #28 的 Doubao 子需求。

### Acceptance Criteria

- Settings 中可以选择 Doubao/Volcengine STT。
- 需要的 credential/resource 字段都能持久化。
- Test connection 能给出明确错误。
- 录音 pipeline 能正常 stream/finalize。
- 错误不会被吞成普通 auth failure。
- 文档或 UI hint 说明凭证从哪里来。

### Risk

当前本地实现看起来是 WebSocket streaming 风格，和 issue 里 “APPID + api key” 的描述可能不完全一致。必须先用官方接口参数核对，否则容易做成“UI 看着支持，实际用户配不通”。

## 9. PR Relationship

当前 open PR 里和这些 issue 相关的：

- [#45 platform bugfixes](https://github.com/tover0314-w/opentypeless/pull/45)
  - 可能和 #57/#58 有历史关联，但目前是 draft/conflicting。不要直接以它为基线修；可以参考思路。
- [#47 custom local whisper STT](https://github.com/tover0314-w/opentypeless/pull/47)
  - 和 #28 Doubao 不直接重叠。
- [#60 recordings](https://github.com/tover0314-w/opentypeless/pull/60)
  - 可能增加录音文件保存，对未来 usage summary 可以加“音频存储占用”，但本 spec 的 usage MVP 不依赖它。
- [#53 60dB integration](https://github.com/tover0314-w/opentypeless/pull/53)
  - 和这 4 个 issue 无直接关系；之前已经单独修过一版，不要混进 issue PR。

建议每个 issue/sub-issue 用独立 PR：

1. `fix/wayland-clipboard-copy-only` closes #57。
2. `fix/wayland-hotkey-status` addresses #58。
3. `feat/custom-polish-instructions` closes #59。
4. `feat/usage-summary` partially closes #28。
5. `feat/volcengine-doubao-stt` closes #28 remaining Doubao part。

## 10. Suggested Execution Order

### Phase 1: #57 small fix

目标：Wayland clipboard 不再模拟 paste。

验证：

- Rust unit tests。
- `npm run test`。
- `npm run build`。
- 手动/模拟 env 检查 `XDG_SESSION_TYPE=wayland` 分支。

### Phase 2: #58 platform capability UX

目标：把 Wayland global hotkey 限制从“隐藏 log”变成“用户看得懂”。

验证：

- Wayland capability command test。
- Settings/Home render tests。
- 注册失败事件不会 crash。

### Phase 3: #59 AI polish controls

目标：用户可以持久化 custom polish instructions 和简繁中文偏好。

验证：

- Rust prompt tests。
- Config migration tests。
- LlmPane tests。
- Cloud + BYOK prompt path都覆盖。

### Phase 4: #28A usage summary

目标：Home 显示聚合 summary，不受前端 history limit 影响。

验证：

- Rust DB aggregation tests。
- Home empty/non-empty render tests。
- clear history 后 summary reset。

### Phase 5: #28B Doubao STT

目标：基于当前 Volcengine/Doubao 本地改动做干净 PR。

验证：

- provider frame/parser unit tests。
- connection benchmark test。
- manual credential test（需要真实 token，不放 CI）。

## 11. Discussion Points Before Coding

需要你拍板的点：

1. #57：Wayland 下 clipboard mode 是否接受 copy-only？我建议接受，不然 GNOME 权限弹窗基本绕不过去。
2. #58：是否把 Wayland global hotkey 定义为 “best effort + visible fallback”，而不是硬承诺修成系统级全局热键？我建议这么定。
3. #59：字段名是否用 `Custom polish instructions`，避免叫 `System Prompt`？我建议不要开放完整 system prompt 替换。
4. #28：Usage summary 放 Home 还是 History 顶部？我建议先放 Home，History 顶部后续再增强。
5. #28 Doubao：是否基于当前本地 `volcengine-doubao` 改动继续补，而不是重新从零写？我建议先 review 现有实现。

## 12. Definition Of Done

每个 PR 合并前至少满足：

- 只改该 issue 需要的最小范围。
- 新增/修改的 config 字段有默认值、迁移兼容、前端类型。
- 新 warning/error code 有 i18n。
- 有单测覆盖核心逻辑。
- `npm run test` 和 `npm run build` 通过。
- Rust tests 至少跑对应模块；如果本机环境继续因为系统权限导致 dependency 编译失败，需要在 PR 里说明本地 blocker 和已跑的替代验证。
