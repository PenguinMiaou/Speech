# 本地引擎与一键安装 实施计划（交付步骤 1）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在设置页点一下就下载、校验、启用 Qwen3-ASR，之后听写的识别在本机完成，润色仍走现有云端配置。

**Architecture:** 引擎放在独立可执行文件 `ot-stt-worker`（链接 `transcribe-cpp`），主程序不链接 C++。两边通过共用的 `ot-stt-protocol` crate 走标准输入输出通信。主程序新增 `local_stt` 模块（模型目录、下载安装、工作进程管理）和文件型 provider `local-qwen3`。

**Tech Stack:** Rust、Tauri 2、React、`transcribe-cpp` 0.4.1（Metal）、`sha2`、`fs2`。

**Spec:** `docs/superpowers/specs/2026-11-09-local-first-dictation-design.md`

## Global Constraints

- 模型目录固定 revision，校验值和大小见 Task 4，校验不过绝不启用。
- 本地模型没装好时，其他服务商照常工作，不自动回退到云端。
- 不打印、不提交任何转写文字和录音。`tools/eval/data/` 必须先加入 `.gitignore` 才能放数据。
- Groq 评测只发 70 条测试录音的识别文字，不发录音。
- `com.opentypeless.app` 不变。
- 引擎、模型管理的代码不写 macOS 专属逻辑；`transcribe-cpp` 只在 macOS arm64 开 `metal` feature。
- 每个任务结束前 `cargo test`（改到的 crate）和 `npm test` 通过。提交由用户授权后再做，计划中的 commit 步骤在未授权时跳过。

## Review Focus

1. 极短录音（误触热键，不足 0.2 秒）：不报错，返回空结果。Task 3 测试。
2. 选中的模型文件被删：听写提示“模型未安装”，不崩溃，卡片回到未安装。Task 3、5。
3. 下载中断网或退出应用：`.part` 保留，再点继续下载。Task 4 测试。
4. 工作进程卡死或崩溃：当次报错，下一次自动重启。Task 2 测试。
5. 连续两次听写：严格串行，结果不串。Task 2 测试。

---

### Task 1: 协议 crate 和工作进程

**Files:**
- Create: `stt-protocol/{Cargo.toml,src/lib.rs}`, `stt-worker/{Cargo.toml,src/main.rs,src/eval.rs}`
- Modify: `.gitignore`（加 `stt-worker/target`、`stt-protocol/target`、`src-tauri/binaries/`、`tools/eval/data/`）

**Interfaces:**
- Produces（`ot_stt_protocol`）：
  ```rust
  pub enum Request { Load { model_path: String }, Transcribe { vocabulary: Vec<String>, prompt: Option<String>, pcm_bytes: u32 }, Shutdown }
  pub enum Response { Ready { backend: String, load_ms: u64 }, Transcript { text: String, language: Option<String>, elapsed_ms: u64 }, Error { code: String, message: String } }
  pub fn write_frame<W: Write, T: Serialize>(w: &mut W, value: &T) -> io::Result<()>
  pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<Option<T>> // EOF 返回 None
  pub const MAX_PCM_BYTES: u32 = 46_080_000; // 12 分钟 16 kHz f32
  ```
- Produces（命令行）：`ot-stt-worker serve`；`ot-stt-worker eval --model <gguf> --dir <bench 目录> --label <名> [--vocab-file <json 数组>] [--context]`。

帧格式：4 字节小端长度加 JSON。`Transcribe` 帧之后紧跟 `pcm_bytes` 字节的小端 f32。JSON 帧上限 1MB，超限返回 `InvalidData`。

- [ ] **Step 1:** 在 `stt-protocol` 写测试 `frame_round_trip`、`eof_returns_none`、`oversized_frame_rejected`，运行确认失败。
- [ ] **Step 2:** 实现协议，`cargo test --manifest-path stt-protocol/Cargo.toml` 通过。
- [ ] **Step 3:** 实现 `serve`：循环读帧；`Load` 用 `Model::load(path)` 建 `session`，`backend` 取自模型；`Transcribe` 读 PCM 后调 `session.run(&pcm, &RunOptions { vocabulary, prompt, ..Default::default() })`；未 `Load` 就转写返回 `Error{code:"not_loaded"}`；不足 0.2 秒的 PCM 直接返回空文本的 `Transcript`；stdin EOF 或 `Shutdown` 时退出。原生日志关掉或走 stderr，stdout 只走协议。
- [ ] **Step 4:** 实现 `eval`：读 `<dir>/manifest.json` 的 `items`（字段 `idx`、`wav`），`--context` 时读 `<dir>/contexts.json` 作 `prompt`，写 `<dir>/results/<label>.jsonl`，每行 `{"idx","text","lang","err","secs"}`，不向 stdout 打印文字。
- [ ] **Step 5:** 验证等同实验：
  ```bash
  cargo run --release --manifest-path stt-worker/Cargo.toml -- eval --model <gguf> --dir <bench> --label worker-plain
  FIX_LABELS=1 uv run --no-project --python 3.12 --with rapidfuzz --with opencc-python-reimplemented python -I <bench>/score.py <bench>
  ```
  Expected：`worker-plain` 约 `all=0.828`、`term_recall=68/96`，`err=0`。
- [ ] **Step 6:** Commit `feat: add local speech worker and protocol`

### Task 2: 工作进程管理

**Files:** Create `src-tauri/src/local_stt/{mod.rs,supervisor.rs}`；Modify `src-tauri/Cargo.toml`（加 `ot-stt-protocol = { path = "../stt-protocol" }`）。

**Interfaces:**
- Produces：
  ```rust
  pub struct WorkerCommand { pub program: PathBuf, pub args: Vec<String> }
  pub struct WorkerSupervisor; // 内部 Mutex 保证串行
  impl WorkerSupervisor {
      pub fn new(command: WorkerCommand, request_timeout: Duration, idle_timeout: Duration) -> Self;
      pub fn ensure_loaded(&self, model_path: &Path) -> Result<(), LocalSttError>;
      pub fn transcribe(&self, model_path: &Path, pcm: &[f32], vocabulary: Vec<String>, prompt: Option<String>) -> Result<String, LocalSttError>;
      pub fn stop_if_idle(&self);
  }
  pub enum LocalSttError { ModelMissing, WorkerCrashed(String), Timeout, Engine { code: String, message: String }, Io(String) }
  pub fn global() -> Option<Arc<WorkerSupervisor>>; pub fn set_global(s: Arc<WorkerSupervisor>);
  ```

同步实现，调用方走 `spawn_blocking`。工作进程路径：`std::env::current_exe()?.with_file_name("ot-stt-worker")`。超时后杀进程并返回 `Timeout`。换模型路径时重新 `Load`。

- [ ] **Step 1:** 用 `sh -c` 假进程写测试：`crash_returns_error_then_next_call_respawns`、`hang_times_out_and_kills`、`concurrent_transcribes_are_serial_and_not_mixed`、`missing_model_returns_model_missing`、`idle_stop_kills_worker`。需要应答协议的假进程用 `stt-protocol` 写一个 test helper 二进制或 Python one-liner 均可。运行确认失败。
- [ ] **Step 2:** 实现，测试通过。
- [ ] **Step 3:** Commit `feat: supervise the local speech worker`

### Task 3: `local-qwen3` provider 接入

**Files:**
- Create: `src-tauri/src/stt/local_qwen3.rs`、`src-tauri/src/local_stt/catalog.rs`（目录内容见 Task 4 的表）
- Modify: `stt/mod.rs`（注册）、`stt/config.rs:117`（免 key，常量 `LOCAL_QWEN3_PROVIDER`）、`stt/capabilities.rs:173`（照 `apple-speech` 加一项）、`pipeline.rs:308`（归 Local）、`pipeline.rs` 构造 `SttConfig` 处（`resource_id: Some(config.stt_local_model.clone())`）、`commands/stt.rs:303,485,814`（诊断和测试分支）、`lib.rs` 的 `setup`（`set_global`，加 30 分钟空闲定时器）、`config` 结构（新增 `stt_local_model: String`，默认 `qwen3-asr-1.7b-q8`）、`Cargo.toml`。

**Interfaces:**
- Consumes：Task 2 的 `global()`、`transcribe`、`ensure_loaded`。
- Produces：`pub const LOCAL_QWEN3_PROVIDER: &str = "local-qwen3";`；`catalog::set_models_dir(dir: PathBuf)`（在 `setup` 调用）、`catalog::installed_path(id: &str) -> Option<PathBuf>`、`catalog::entries() -> &'static [ModelEntry]`。

`connect`：记录 `resource_id`，`spawn_blocking` 后台 `ensure_loaded` 预热，不等待、不阻塞录音。`send_audio`：累积，超 `MAX_PCM_BYTES` 对应长度报错。音频格式照 `whisper_compat.rs` 建 WAV 的方式确认（预期 16 位小端 16 kHz 单声道），转 f32 除以 32768。`recv_transcript`：返回 `Ok(None)`。`disconnect`：`spawn_blocking` 调 `transcribe`，空结果返回 `Ok(None)`。超时用 `STT_FINALIZE_TIMEOUT_SECS`。错误映射：`ModelMissing` 映射为 `AppError::Config("local_model_not_installed")`。

- [ ] **Step 1:** 写测试 `pcm_i16_converted_to_f32`、`create_provider_returns_local_qwen3`、`does_not_require_api_key`、`disconnect_with_empty_buffer_returns_none`、`missing_model_reports_not_installed`，运行确认失败。
- [ ] **Step 2:** 实现并接线，`cargo test --manifest-path src-tauri/Cargo.toml` 通过。
- [ ] **Step 3:** Commit `feat: add local-qwen3 speech provider`

### Task 4: 模型目录和下载安装

**Files:** Create `src-tauri/src/local_stt/install.rs`、`src-tauri/src/commands/local_stt.rs`；Modify `commands/mod.rs`、`lib.rs`（注册命令）、`Cargo.toml`（`sha2`、`fs2`，dev `wiremock`，tokio 加需要的 feature）。

目录（写死）：

| id | 文件 | repo / rev | 字节 | SHA-256 |
|---|---|---|---|---|
| `qwen3-asr-1.7b-q8` | `Qwen3-ASR-1.7B-Q8_0.gguf` | `handy-computer/Qwen3-ASR-1.7B-gguf` / `92282af1610a2db19d66f2bef1e260f5deca782d` | 2185030624 | `9a0d81792dfea2d5f278b8a63deb3ea6e02139ce42c2301f32ea19c4f77526b7` |
| `qwen3-asr-0.6b-q8` | `Qwen3-ASR-0.6B-Q8_0.gguf` | `handy-computer/Qwen3-ASR-0.6B-gguf` / `e4e16599b900eb0cb36e524514756bb92eb092b7` | 850423456 | `f081b2d5e23bd669d92cc331d722a8a0681943b8e6f34b48996fd5c319b5acd8` |

下载地址 `https://huggingface.co/<repo>/resolve/<rev>/<文件>`，存到 `<app_data_dir>/models/`。

**Interfaces:**
- Produces：
  ```rust
  #[tauri::command] local_stt_list_models() -> Vec<LocalModelStatus> // id, size_bytes, recommended, installed
  #[tauri::command] local_stt_install_model(model_id: String) -> Result<(), String>
  #[tauri::command] local_stt_cancel_install(model_id: String)
  #[tauri::command] local_stt_delete_model(model_id: String) -> Result<(), String>
  // 事件 "local-stt:progress": { model_id, phase: "downloading"|"verifying"|"testing"|"ready"|"failed"|"cancelled", downloaded, total, bytes_per_sec, error_code }
  // error_code: "disk_full" | "network" | "checksum" | "engine"
  ```

流程按 Spec 第 2 节：查空间（需要大小的 1.1 倍）、`Range` 续传（服务器回 200 则从头写）、流式写 `.part`、全文件 SHA-256、失败删 `.part`、`rename`、`ensure_loaded` 后转写一秒静音。删除正在使用的模型前先停工作进程。

- [ ] **Step 1:** 用 `wiremock` 写测试：`downloads_and_verifies`、`resumes_with_range_header`、`server_ignoring_range_restarts`、`checksum_mismatch_deletes_part`、`network_drop_keeps_part`、`cancel_keeps_part_and_reports_cancelled`、`insufficient_disk_fails_before_download`、`catalog_entries_are_pinned`（URL 含 rev，SHA 长度 64）。运行确认失败。
- [ ] **Step 2:** 实现，测试通过。
- [ ] **Step 3:** Commit `feat: one-click local model install`

### Task 5: 设置页模型卡片

**Files:** Create `src/components/Settings/LocalModelCard.tsx` 和测试；Modify `src/lib/constants.ts`（`STT_PROVIDERS` 加本地项并排第一）、`src/stores/appStore.ts`（类型、`stt_local_model` 默认值）、`src/lib/tauri.ts:133`、`src/components/Settings/SttPane.tsx`（`isLocal` 时不显示 key 和测试按钮，顶部放卡片）、`src/i18n/locales/*.json`（`localeParity.test.ts` 要求 10 种语言键一致，非中英文用英文文案）。

卡片状态：未安装（主按钮“下载并启用”，显示大小）、下载中（进度、速度、取消）、校验中、已启用（标记和删除）、失败（原因和重试）。默认展示 1.7B；0.6B 放“更小更快”的次要入口。`ready` 事件后调 store 的配置更新，写入 `stt_provider: 'local-qwen3'` 和 `stt_local_model`。安装中切页不丢状态，进入时用 `local_stt_list_models` 和事件恢复。

- [ ] **Step 1:** 写测试：`shows_install_button_when_missing`、`shows_progress_on_event`、`activates_provider_on_ready`、`shows_retry_on_failed`、`shows_not_installed_when_selected_model_is_gone`，运行确认失败。
- [ ] **Step 2:** 实现，`npm test && npm run lint && npx tsc --noEmit` 通过。
- [ ] **Step 3:** Commit `feat: local model card in speech settings`

### Task 6: 打包、关更新器、修计时

**Files:** Create `scripts/build-stt-worker.mjs`；Modify `src-tauri/tauri.conf.json`、`src-tauri/capabilities/default.json:31`、`src-tauri/src/lib.rs:884`、`src-tauri/Cargo.toml`、`package.json`、挂载 `UpdatePrompt` 的组件，删除 `src/components/UpdatePrompt.tsx` 及其测试、`pipeline.rs`（计时）。

- 脚本：`cargo build --release --manifest-path stt-worker/Cargo.toml --target <triple>`，复制到 `src-tauri/binaries/ot-stt-worker-<triple>`。`tauri.conf.json` 加 `bundle.externalBin: ["binaries/ot-stt-worker"]`，`beforeBuildCommand`、`beforeDevCommand` 先跑脚本。
- 更新器：删插件注册、权限、`plugins.updater`、`createUpdaterArtifacts`、npm 和 cargo 依赖。先 `grep -rn updater src src-tauri/src` 确认没有残留引用。
- 计时：`pipeline.rs` 约 2040 行把 `total_elapsed` 移到保存之后；`pipeline:timing` 和日志拆出 `output_ms`、`save_ms`，输出内部再记 `focus_ms`、`insert_ms`。

- [ ] **Step 1:** 改动后 `cargo test`、`npm test` 通过。
- [ ] **Step 2:** 打包验证：
  ```bash
  APPLE_SIGNING_IDENTITY="<your signing identity>" npm run tauri build -- --bundles app --target aarch64-apple-darwin
  codesign -dv <app>/Contents/MacOS/ot-stt-worker
  <app>/Contents/MacOS/ot-stt-worker eval --model <gguf> --dir <bench> --label packaged
  ```
  Expected：`.app` 内有工作进程，签名有效，`packaged` 的 `backend` 为 Metal，分数同 Task 1。签名被权限系统拦截时停下来问用户。
- [ ] **Step 3:** Commit `chore: bundle worker, drop updater, finer pipeline timing`

### Task 7: 评测迁入仓库和整体验收

**Files:** Create `tools/eval/{README.md,prep_manifest.py,prep_context.py,score.py}`、`src-tauri/examples/polish_eval.rs`。数据复制到 `tools/eval/data/`（已忽略）。

`polish_eval.rs` 走真实代码：读 `results/<label>.jsonl`，用 `llm/openai.rs:35` 的 provider 和 `PolishRequest` 润色，模型和参数与用户现有配置相同（`qwen/qwen3.8-27b`，`groq`），key 从环境变量读，结果写 `results/<label>-polished.jsonl`，不打印文字。如 `llm` 模块未公开，将需要的项 `pub` 导出。

- [ ] **Step 1:** 确认 `git check-ignore tools/eval/data/x` 生效，再迁入。
- [ ] **Step 2:** 跑 Groq 评测（只发 70 条识别文字，用户已同意）：
  ```bash
  set -a; source ~/.claude/.credentials/groq.env; set +a
  cargo run --release --manifest-path src-tauri/Cargo.toml --example polish_eval -- --dir tools/eval/data --label tcpp-qwen-1.7b-q8-vocab-ctx
  FIX_LABELS=1 uv run --no-project --python 3.12 --with rapidfuzz --with opencc-python-reimplemented python -I tools/eval/score.py tools/eval/data
  ```
  Expected：`-polished` 一行有 `n=70`、`err=0`。把分数写入设计文档作为润色基线。
- [ ] **Step 3:** 留出集跑一次 `worker-holdout`，只看汇总，写入设计文档。
- [ ] **Step 4:** 看 `/Applications/OpenTypeless.app` 当前版本，备份后替换前向用户确认，由用户从设置页点一键安装，听写几条。Expected：验收表的“一键安装”“引擎接入”“识别延迟”通过，日志有输出阶段的细分计时。
- [ ] **Step 5:** Commit `chore: move evaluation into the repo`
