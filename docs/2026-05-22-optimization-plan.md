# OpenTypeless 三阶段优化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix installation experience, improve pipeline reliability with retry/fallback, and refactor code architecture for maintainability.

**Architecture:** Three independent phases, each committed and tested before the next begins. Phase 1 adds platform detection and docs. Phase 2 adds error types, retry logic, output fallback, and i18n error messages inside existing providers without changing trait signatures. Phase 3 extracts large files into focused modules.

**Tech Stack:** Rust (Tauri 2, reqwest, tokio, enigo, arboard), TypeScript/React (Zustand, i18next, Framer Motion)

**Design spec:** `docs/2026-05-21-optimization-design.md`

---

## File Structure Overview

### Phase 1 — Files Modified
| Action | Path | Purpose |
|--------|------|---------|
| Modify | `src-tauri/src/lib.rs` | Add NVIDIA/Wayland env detection before WebView init |
| Modify | `README.md` | Add platform-specific installation instructions |

### Phase 2 — Files Created
| Action | Path | Purpose |
|--------|------|---------|
| Create | `src-tauri/src/error.rs` | AppError enum + UserError struct + retry helper |

### Phase 2 — Files Modified
| Action | Path | Purpose |
|--------|------|---------|
| Modify | `src-tauri/src/lib.rs` | Register shared `reqwest::Client` as Tauri state |
| Modify | `src-tauri/src/pipeline.rs` | Use shared client, integrate retry/fallback/timeout, emit structured errors |
| Modify | `src-tauri/src/stt/whisper_compat.rs` | Add retry around HTTP POST in `disconnect()` |
| Modify | `src-tauri/src/stt/deepgram.rs` | Add retry around WebSocket connect |
| Modify | `src-tauri/src/stt/assemblyai.rs` | Add retry around WebSocket connect |
| Modify | `src-tauri/src/stt/cloud.rs` | Add retry around HTTP POST in `disconnect()` |
| Modify | `src-tauri/src/llm/openai.rs` | Add retry around SSE connection |
| Modify | `src-tauri/src/llm/cloud.rs` | Add retry around SSE connection |
| Modify | `src-tauri/src/output/keyboard.rs` | Linux Wayland/xdotool detection |
| Modify | `src-tauri/src/output/mod.rs` | Expose fallback chain helper |
| Modify | `src/hooks/useTauriEvents.ts` | Handle structured error events with i18n |
| Modify | `src/components/Capsule/CapsuleError.tsx` | Use i18n error codes |
| Modify | `src/components/Toast.tsx` | No change needed — already supports `toast.info()` |
| Modify | `src/i18n/locales/en.json` | Add `errors` namespace |
| Modify | `src/i18n/locales/zh.json` | Add `errors` namespace |

### Phase 3 — Files Created
| Action | Path | Purpose |
|--------|------|---------|
| Create | `src-tauri/src/stt/config.rs` | Shared STT provider config constants |
| Create | `src-tauri/src/commands/mod.rs` | Command module re-exports |
| Create | `src-tauri/src/commands/stt.rs` | test_stt_connection, bench_stt_connection |
| Create | `src-tauri/src/commands/llm.rs` | test_llm_connection, bench_llm_connection, fetch_llm_models |
| Create | `src-tauri/src/commands/config.rs` | get_config, update_config, set_auto_start, set_session_token |
| Create | `src-tauri/src/commands/history.rs` | get_history, clear_history |
| Create | `src-tauri/src/commands/dictionary.rs` | get_dictionary, add_dictionary_entry, remove_dictionary_entry |
| Create | `src-tauri/src/commands/misc.rs` | check_accessibility_permission, request_accessibility_permission, pause_hotkey, resume_hotkey, update_hotkey |
| Create | `src-tauri/src/tray.rs` | build_tray_menu, refresh_tray |
| Create | `src-tauri/src/hotkey.rs` | parse_hotkey, default_shortcut, build_shortcut_handler |

### Phase 3 — Files Modified
| Action | Path | Purpose |
|--------|------|---------|
| Modify | `src-tauri/src/lib.rs` | Slim down to run() entry + state types only |
| Modify | `src-tauri/src/pipeline.rs` | Extract stop() phases into named methods |
| Modify | `src-tauri/src/stt/mod.rs` | Use shared config from stt/config.rs |
| Modify | `src-tauri/src/llm/mod.rs` | Update provider factories for AppError |
| Modify | `src-tauri/src/stt/whisper_compat.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/stt/deepgram.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/stt/assemblyai.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/stt/cloud.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/llm/openai.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/llm/cloud.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/output/keyboard.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/output/clipboard.rs` | Return AppError instead of anyhow |
| Modify | `src-tauri/src/output/mod.rs` | Update TextOutput trait + create_output |

---

## Phase 1: Installation Experience

### Task 1: Linux NVIDIA/Wayland EGL crash detection (#36)

**Files:**
- Modify: `src-tauri/src/lib.rs:1028-1038` (run() function, before Builder chain)

- [ ] **Step 1: Add NVIDIA/Wayland detection function**

Add the following function in `src-tauri/src/lib.rs` before the `run()` function (around line 1026):

```rust
/// On Linux with NVIDIA proprietary drivers + Wayland, WebKit's DMA-BUF renderer
/// crashes in libnvidia-eglcore during GL context teardown. Set env vars to disable
/// it before any WebView is created. See GitHub issue #36.
fn apply_linux_workarounds() {
    #[cfg(target_os = "linux")]
    {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let is_nvidia = std::path::Path::new("/proc/driver/nvidia").exists()
            || std::env::var("__GLX_VENDOR_LIBRARY_NAME")
                .map(|v| v.eq_ignore_ascii_case("nvidia"))
                .unwrap_or(false);

        if is_nvidia && session == "wayland" {
            tracing::info!("Detected NVIDIA + Wayland, disabling WebKit DMA-BUF renderer");
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
}
```

- [ ] **Step 2: Call it at the top of run()**

In `src-tauri/src/lib.rs`, add the call as the very first line of `run()` (line 1029, before `tracing_subscriber`):

```rust
pub fn run() {
    apply_linux_workarounds();

    tracing_subscriber::fmt()
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "fix(linux): auto-detect NVIDIA + Wayland and disable DMA-BUF renderer (#36)"
```

---

### Task 2: README installation instructions (#34, #35, #37)

**Files:**
- Modify: `README.md` (Download/Installation section, around lines 87-119)

- [ ] **Step 1: Add platform-specific installation warnings**

In `README.md`, after the Download section (after line 99) and before the **Getting Started** section, insert a new section:

```markdown
## Installation Notes

> **OpenTypeless is an unsigned open-source application.** You may see security warnings during installation — these are expected.

### Windows

Windows SmartScreen may show "Windows protected your PC":

1. Click **More info**
2. Click **Run anyway**

If you downloaded a `.msi` that shows a publisher validation error:
1. Right-click the `.msi` file → **Properties**
2. Check **Unblock** at the bottom → **Apply**
3. Run the installer again

### macOS

macOS Gatekeeper may report the app is "damaged" because it lacks a Developer certificate:

```bash
xattr -cr /Applications/OpenTypeless.app
```

Then open the app normally.

### Linux

**Ubuntu/Debian** — install the `.deb` package:
```bash
sudo apt install ./OpenTypeless_x.x.x_amd64.deb
```

**AppImage** — make it executable and run:
```bash
chmod +x OpenTypeless_x.x.x_amd64.AppImage
./OpenTypeless_x.x.x_amd64.AppImage
```

**NVIDIA + Wayland users:** If the app crashes on startup, it should now auto-detect and work around the issue. If problems persist, run:
```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./OpenTypeless
```
```

- [ ] **Step 2: Verify README renders correctly**

Read the file to check markdown formatting is valid.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add platform-specific installation instructions for unsigned app (#34 #35 #37)"
```

---

## Phase 2: Pipeline Reliability

### Task 3: Create unified error types

**Files:**
- Create: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod error;`)

- [ ] **Step 1: Create error.rs**

```rust
use serde::Serialize;
use std::time::Duration;

/// Structured error sent to the frontend via Tauri events.
/// The frontend uses `code` to look up an i18n-translated message.
#[derive(Debug, Clone, Serialize)]
pub struct UserError {
    pub code: String,
    pub details: Option<String>,
    pub retry_count: u32,
}

/// Internal error type used throughout the Rust backend.
/// Provides `is_retryable()` for retry logic and `to_user_error()` for frontend display.
#[derive(Debug)]
pub enum AppError {
    Network(String),
    Timeout(Duration),
    Api { status: u16, body: String },
    Auth(String),
    Output(String),
    Config(String),
}

impl AppError {
    pub fn is_retryable(&self) -> bool {
        match self {
            AppError::Network(_) => true,
            AppError::Timeout(_) => true,
            AppError::Api { status, .. } => *status >= 500,
            AppError::Auth(_) => false,
            AppError::Output(_) => false,
            AppError::Config(_) => false,
        }
    }

    pub fn to_user_error(&self) -> UserError {
        let (code, details) = match self {
            AppError::Network(msg) => ("stt_timeout".to_string(), Some(msg.clone())),
            AppError::Timeout(_) => ("stt_timeout".to_string(), None),
            AppError::Api { status, body } => {
                if *status == 401 || *status == 403 {
                    ("stt_invalid_key".to_string(), None)
                } else {
                    ("stt_failed".to_string(), Some(format!("HTTP {}", status)))
                }
            }
            AppError::Auth(msg) => ("stt_invalid_key".to_string(), Some(msg.clone())),
            AppError::Output(msg) => {
                ("output_fallback_clipboard".to_string(), Some(msg.clone()))
            }
            AppError::Config(msg) => ("stt_failed".to_string(), Some(msg.clone())),
        };
        UserError {
            code,
            details,
            retry_count: 0,
        }
    }

    pub fn with_retry_count(self, count: u32) -> UserError {
        let mut ue = self.to_user_error();
        ue.retry_count = count;
        ue
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Network(msg) => write!(f, "Network error: {}", msg),
            AppError::Timeout(d) => write!(f, "Timeout after {:.1}s", d.as_secs_f64()),
            AppError::Api { status, body } => write!(f, "API error {}: {}", status, body),
            AppError::Auth(msg) => write!(f, "Auth error: {}", msg),
            AppError::Output(msg) => write!(f, "Output error: {}", msg),
            AppError::Config(msg) => write!(f, "Config error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            AppError::Timeout(Duration::from_secs(30))
        } else if let Some(status) = e.status() {
            AppError::Api {
                status: status.as_u16(),
                body: e.to_string(),
            }
        } else {
            AppError::Network(e.to_string())
        }
    }
}

/// Retry an async operation with exponential backoff.
/// - `max_retries`: number of retries (0 = no retry)
/// - `f`: closure returning a Future that produces Result<T, AppError>
/// Emits a `pipeline:retry` event on each retry attempt.
pub async fn with_retry<F, Fut, T>(
    app_handle: &tauri::AppHandle,
    max_retries: u32,
    f: F,
) -> Result<T, AppError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, AppError>>,
{
    let mut last_error: Option<AppError> = None;
    for attempt in 0..=max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempt < max_retries => {
                let delay_ms = 1000 * 2u64.pow(attempt);
                tracing::warn!(
                    "Retryable error (attempt {}/{}): {}, retrying in {}ms",
                    attempt + 1,
                    max_retries,
                    e,
                    delay_ms
                );
                let _ = app_handle.emit(
                    "pipeline:retry",
                    serde_json::json!({
                        "attempt": attempt + 1,
                        "max": max_retries,
                        "error": e.to_string(),
                    }),
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_error.unwrap())
}
```

- [ ] **Step 2: Register the module in lib.rs**

Add `pub mod error;` to the module declarations at the top of `src-tauri/src/lib.rs` (after the existing `pub mod` lines, around line 10):

```rust
pub mod app_detector;
pub mod audio;
pub mod error;
pub mod llm;
pub mod output;
pub mod pipeline;
pub mod storage;
pub mod stt;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/error.rs src-tauri/src/lib.rs
git commit -m "feat(error): add unified AppError type, UserError struct, and retry helper"
```

---

### Task 4: Shared HTTP Client via Tauri state

**Files:**
- Modify: `src-tauri/src/lib.rs:1083-1093` (setup block, manage() calls)
- Modify: `src-tauri/src/pipeline.rs:165-183` (PipelineHandle::new)

- [ ] **Step 1: Add shared client creation in run()**

In `src-tauri/src/lib.rs`, inside the `setup` closure, after line 1100 (`app.manage(SessionTokenStore(...))`) and before the auto-start block (line 1102), add:

```rust
            // Shared HTTP client with connection pooling for all providers
            let shared_client = reqwest::Client::builder()
                .pool_max_idle_per_host(2)
                .pool_idle_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client");
            app.manage(shared_client);
```

- [ ] **Step 2: Update PipelineHandle::new to accept shared client**

Change `src-tauri/src/pipeline.rs` line 183 (`pub fn new`) to accept the shared client:

```rust
    pub fn new(app_handle: tauri::AppHandle, shared_client: reqwest::Client) -> Self {
        Self {
            app_handle,
            state: Arc::new(AtomicU8::new(PipelineState::Idle.as_u8())),
            audio_handle: Arc::new(Mutex::new(None)),
            audio_volume: Arc::new(Mutex::new(0.0)),
            accumulated_text: Arc::new(Mutex::new(String::new())),
            stt_done: Arc::new(Notify::new()),
            abort_flag: Arc::new(AtomicBool::new(false)),
            preloaded_config: Arc::new(Mutex::new(None)),
            preloaded_app_ctx: Arc::new(Mutex::new(None)),
            preloaded_dictionary: Arc::new(Mutex::new(None)),
            preloaded_selected_text: Arc::new(Mutex::new(None)),
            recording_start: Arc::new(Mutex::new(None)),
            shared_client,
            pipeline_lock: tokio::sync::Mutex::new(()),
        }
    }
```

- [ ] **Step 3: Update the call site in run()**

In `src-tauri/src/lib.rs`, change line 1083 from:

```rust
            let pipeline_handle = pipeline::PipelineHandle::new(app_handle.clone());
```

to:

```rust
            let pipeline_handle = pipeline::PipelineHandle::new(
                app_handle.clone(),
                shared_client.clone(),
            );
```

Note: the `shared_client` must be created **before** `pipeline_handle`, so move the client creation block above the pipeline creation line. The order in setup() should be:

1. Create data dir and database (lines 1072-1082)
2. Initialize stores (lines 1078-1082)
3. **Create shared_client** (new)
4. Create pipeline_handle with shared_client (modified)
5. Load initial config (lines 1086-1088)
6. app.manage() calls (lines 1090-1101)

- [ ] **Step 4: Update test/benchmark commands to use shared client**

In `src-tauri/src/lib.rs`, update `test_stt_connection`, `test_llm_connection`, `bench_stt_connection`, `bench_llm_connection` to accept the shared client from Tauri state instead of creating `reqwest::Client::new()`.

For each of these four commands, add `client: tauri::State<'_, reqwest::Client>` to the parameters and replace `reqwest::Client::new()` with `client.inner().clone()`.

Example for `test_stt_connection` (line 164):

```rust
#[tauri::command]
async fn test_stt_connection(
    api_key: String,
    provider: String,
    token_store: tauri::State<'_, SessionTokenStore>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<bool, String> {
```

And inside, replace `let client = reqwest::Client::new();` with `let client = client.inner().clone();`.

Apply the same pattern to `test_llm_connection`, `bench_stt_connection`, `bench_llm_connection`.

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/pipeline.rs
git commit -m "refactor: use shared reqwest::Client via Tauri state for connection pooling"
```

---

### Task 5: Add STT retry logic

**Files:**
- Modify: `src-tauri/src/stt/whisper_compat.rs` (disconnect method, lines 106-198)
- Modify: `src-tauri/src/stt/cloud.rs` (disconnect method, lines 63-133)

The WebSocket providers (deepgram, assemblyai) handle retries differently — they reconnect the WebSocket. We'll add retry there too.

- [ ] **Step 1: Add retry to whisper_compat disconnect()**

In `src-tauri/src/stt/whisper_compat.rs`, wrap the HTTP POST in `disconnect()` with a retry loop. The current code sends the request once. Change it to retry up to 3 times on server errors.

Add imports at the top of the file:

```rust
use std::time::Duration;
```

In the `disconnect()` method, wrap the HTTP request section (the `let resp = client.post(...)` block around lines 140-170) in a retry loop:

```rust
        let mut last_error: Option<anyhow::Error> = None;
        for attempt in 0..3u32 {
            match client
                .post(&self.provider_config.endpoint)
                .header("Authorization", format!("Bearer {}", api_key))
                .multipart(form.clone())
                .timeout(Duration::from_secs(30))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        // parse response as before
                        let body: serde_json::Value = resp.json().await?;
                        let text = body["text"]
                            .as_str()
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if text.is_empty() {
                            anyhow::bail!("{} returned empty transcription", self.provider_config.provider_name);
                        }
                        tracing::info!("{} transcription complete: {} chars", self.provider_config.provider_name, text.len());
                        return Ok(Some(text));
                    } else if status.as_u16() >= 500 && attempt < 2 {
                        let body = resp.text().await.unwrap_or_default();
                        tracing::warn!("{} server error {} (attempt {}/3), retrying", self.provider_config.provider_name, status, attempt + 1);
                        last_error = Some(anyhow::anyhow!("HTTP {}: {}", status, body));
                        tokio::time::sleep(Duration::from_millis(1000 * 2u64.pow(attempt))).await;
                        // Rebuild form for retry (multipart was consumed)
                        form = reqwest::multipart::Form::new()
                            .part("file", reqwest::multipart::Part::bytes(wav_data.clone())
                                .file_name("audio.wav")
                                .mime_str("audio/wav")?);
                        continue;
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        anyhow::bail!("{} API error {}: {}", self.provider_config.provider_name, status, body);
                    }
                }
                Err(e) if e.is_timeout() && attempt < 2 => {
                    tracing::warn!("{} timeout (attempt {}/3), retrying", self.provider_config.provider_name, attempt + 1);
                    last_error = Some(e.into());
                    tokio::time::sleep(Duration::from_millis(1000 * 2u64.pow(attempt))).await;
                    form = reqwest::multipart::Form::new()
                        .part("file", reqwest::multipart::Part::bytes(wav_data.clone())
                            .file_name("audio.wav")
                            .mime_str("audio/wav")?);
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retries exhausted")))
```

Note: `wav_data` must be cloned before the loop so it's available for retries. Move `let wav_data = Self::build_wav(...)` before the loop and store it in a variable that the loop can reference.

- [ ] **Step 2: Add retry to cloud STT disconnect()**

Apply the same retry pattern in `src-tauri/src/stt/cloud.rs` `disconnect()` method. The structure is similar — HTTP POST with multipart.

- [ ] **Step 3: Add retry to deepgram connect()**

In `src-tauri/src/stt/deepgram.rs`, wrap the WebSocket connection in `connect()` (lines 47-66) with a retry loop:

```rust
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        if config.api_key.is_empty() {
            anyhow::bail!("Deepgram API key is empty");
        }
        self.stt_config = Some(config.clone());
        let url = Self::build_url(config);

        let mut last_error: Option<anyhow::Error> = None;
        for attempt in 0..3u32 {
            match connect_async(&url).await {
                Ok((ws, _)) => {
                    self.ws = Some(ws);
                    tracing::info!("Deepgram connected");
                    return Ok(());
                }
                Err(e) if attempt < 2 => {
                    tracing::warn!("Deepgram connect failed (attempt {}/3): {}, retrying", attempt + 1, e);
                    last_error = Some(e.into());
                    tokio::time::sleep(std::time::Duration::from_millis(1000 * 2u64.pow(attempt))).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(last_error.unwrap())
    }
```

- [ ] **Step 4: Add retry to assemblyai connect()**

Apply the same retry pattern to `src-tauri/src/stt/assemblyai.rs` `connect()` method.

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/stt/
git commit -m "feat(stt): add exponential backoff retry to all STT providers"
```

---

### Task 6: Add LLM retry logic

**Files:**
- Modify: `src-tauri/src/llm/openai.rs` (polish method, lines 32-187)
- Modify: `src-tauri/src/llm/cloud.rs` (polish method, lines 34-150)

- [ ] **Step 1: Add retry to OpenAI LLM polish()**

In `src-tauri/src/llm/openai.rs`, wrap the initial connection request in `polish()` with retry. The key insight from the design: only retry on connection errors and timeouts, NOT once SSE streaming has started.

Add a retry loop around the initial `client.post(...).send()` call (around lines 60-80 in the current code). Store the response outside the loop, and only retry if the send() fails before getting a response.

```rust
        // Retry the initial connection (not once streaming starts)
        let mut response = None;
        let mut last_error: Option<anyhow::Error> = None;
        for attempt in 0..3u32 {
            match client
                .post(&config.base_url)
                .header("Authorization", format!("Bearer {}", config.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        response = Some(resp);
                        break;
                    } else if status.as_u16() >= 500 && attempt < 2 {
                        let body_text = resp.text().await.unwrap_or_default();
                        tracing::warn!("LLM server error {} (attempt {}/3), retrying", status, attempt + 1);
                        last_error = Some(anyhow::anyhow!("HTTP {}: {}", status, body_text));
                        tokio::time::sleep(std::time::Duration::from_millis(1000 * 2u64.pow(attempt))).await;
                        continue;
                    } else {
                        let body_text = resp.text().await.unwrap_or_default();
                        anyhow::bail!("LLM API error {}: {}", status, body_text);
                    }
                }
                Err(e) if e.is_timeout() && attempt < 2 => {
                    tracing::warn!("LLM connection timeout (attempt {}/3), retrying", attempt + 1);
                    last_error = Some(e.into());
                    tokio::time::sleep(std::time::Duration::from_millis(1000 * 2u64.pow(attempt))).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        let response = response.ok_or_else(|| last_error.unwrap())?;
```

After this block, continue with the existing SSE stream parsing code.

- [ ] **Step 2: Add retry to Cloud LLM polish()**

Apply the same retry pattern to `src-tauri/src/llm/cloud.rs` `polish()` method.

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/llm/
git commit -m "feat(llm): add connection retry with exponential backoff"
```

---

### Task 7: Output fallback chain

**Files:**
- Modify: `src-tauri/src/output/mod.rs` (add fallback helper)
- Modify: `src-tauri/src/output/keyboard.rs` (add error type info)
- Modify: `src-tauri/src/pipeline.rs` (output_text method, lines 895-921)

- [ ] **Step 1: Add fallback output function to output/mod.rs**

Add a new function `output_with_fallback` in `src-tauri/src/output/mod.rs`:

```rust
use crate::error::UserError;
use tauri::Emitter;

/// Try keyboard output first. On failure, fall back to clipboard.
/// On clipboard failure, return the error with code indicating both failed.
/// Returns a UserError code for the frontend to display.
pub async fn output_with_fallback(
    text: &str,
    mode: OutputMode,
    app_handle: &tauri::AppHandle,
) -> Result<Option<UserError>, String> {
    if mode == OutputMode::Clipboard {
        let output = create_output(OutputMode::Clipboard);
        return output.type_text(text).await.map_err(|e| e.to_string()).map(|_| None);
    }

    // Try keyboard first
    let keyboard = create_output(OutputMode::Keyboard);
    match keyboard.type_text(text).await {
        Ok(()) => Ok(None),
        Err(kb_err) => {
            tracing::warn!("Keyboard output failed: {}, falling back to clipboard", kb_err);
            // Fall back to clipboard
            let clipboard = create_output(OutputMode::Clipboard);
            match clipboard.type_text(text).await {
                Ok(()) => {
                    let ue = UserError {
                        code: "output_fallback_clipboard".to_string(),
                        details: Some(kb_err.to_string()),
                        retry_count: 0,
                    };
                    Ok(Some(ue))
                }
                Err(cb_err) => {
                    Err(format!("Both keyboard ({}) and clipboard ({}) output failed", kb_err, cb_err))
                }
            }
        }
    }
}
```

- [ ] **Step 2: Update pipeline.rs output_text to use fallback**

Replace the `output_text` method in `src-tauri/src/pipeline.rs` (lines 895-921) with:

```rust
    async fn output_text(
        &self,
        text: &str,
        app_name: &str,
        config: &storage::AppConfig,
    ) -> Result<()> {
        self.set_state(PipelineState::Outputting);

        let mode = if config.output_mode == "keyboard" {
            OutputMode::Keyboard
        } else {
            OutputMode::Clipboard
        };

        if mode == OutputMode::Keyboard && !is_accessibility_trusted() {
            anyhow::bail!("ACCESSIBILITY_REQUIRED");
        }

        match output::output_with_fallback(text, mode, &self.app_handle).await {
            Ok(Some(user_error)) => {
                // Fell back to clipboard — notify frontend
                tracing::info!("Output fell back to clipboard");
                let _ = self.app_handle.emit("pipeline:warning", &user_error);
            }
            Ok(None) => {}
            Err(e) => anyhow::bail!("{}", e),
        }

        let _ = self.app_handle.emit("pipeline:target_app", app_name);
        Ok(())
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/output/ src-tauri/src/pipeline.rs
git commit -m "feat(output): add keyboard → clipboard fallback chain with user notification"
```

---

### Task 8: Linux keyboard output detection (#32)

**Files:**
- Modify: `src-tauri/src/output/keyboard.rs` (add detection)

- [ ] **Step 1: Add Linux environment detection**

Add a function at the top of `src-tauri/src/output/keyboard.rs` (after imports, before the struct):

```rust
/// Check if keyboard simulation is reliable on this platform.
/// Returns Ok(()) if fine, or Err with a user-facing reason.
pub fn check_keyboard_available() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        if session == "wayland" {
            return Err("wayland_unsupported".to_string());
        }
        // Check xdotool availability on X11
        if session == "x11" || session.is_empty() {
            if std::process::Command::new("which")
                .arg("xdotool")
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true)
            {
                return Err("xdotool_missing".to_string());
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Use it in pipeline.rs**

In `src-tauri/src/pipeline.rs`, update the `output_text` method to check Linux keyboard availability before attempting keyboard output. Add this check right after the accessibility check:

```rust
        if mode == OutputMode::Keyboard {
            if let Err(reason) = output::keyboard::check_keyboard_available() {
                if reason == "wayland_unsupported" {
                    tracing::warn!("Keyboard output not supported on Wayland, falling back to clipboard");
                    let ue = crate::error::UserError {
                        code: "output_wayland_unsupported".to_string(),
                        details: None,
                        retry_count: 0,
                    };
                    let _ = self.app_handle.emit("pipeline:warning", &ue);
                    // Fall through to clipboard output
                    return self.output_text(text, app_name, &storage::AppConfig {
                        output_mode: "clipboard".to_string(),
                        ..config.clone()
                    }).await;
                }
                // xdotool_missing — still try, enigo might have its own backend
                tracing::warn!("xdotool not found, keyboard output may fail: {}", reason);
            }
        }
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/output/keyboard.rs src-tauri/src/pipeline.rs
git commit -m "feat(linux): detect Wayland/xdotool and auto-fallback to clipboard (#32)"
```

---

### Task 9: Frontend i18n error messages

**Files:**
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh.json`
- Modify: `src/hooks/useTauriEvents.ts`
- Modify: `src/components/Capsule/CapsuleError.tsx`

- [ ] **Step 1: Add errors namespace to en.json**

Add a new `"errors"` key at the end of `src/i18n/locales/en.json` (before the closing `}`):

```json
  "errors": {
    "stt_invalid_key": "Invalid API Key, please check settings",
    "stt_timeout": "Connection timeout, retrying...",
    "stt_failed": "Speech recognition failed, please check your network",
    "llm_failed": "Text polish failed, raw transcription saved",
    "output_fallback_clipboard": "Copied to clipboard, paste manually",
    "output_wayland_unsupported": "Keyboard output unsupported on Wayland, switched to clipboard mode",
    "output_xdotool_missing": "xdotool not found, install it for keyboard output: sudo apt install xdotool"
  }
```

- [ ] **Step 2: Add errors namespace to zh.json**

Add the same structure to `src/i18n/locales/zh.json`:

```json
  "errors": {
    "stt_invalid_key": "API Key 无效，请检查设置",
    "stt_timeout": "网络连接超时，正在重试...",
    "stt_failed": "语音识别失败，请检查网络连接",
    "llm_failed": "文本润色失败，原始转录已保存",
    "output_fallback_clipboard": "已复制到剪贴板，请手动粘贴",
    "output_wayland_unsupported": "Wayland 不支持键盘输出，已切换为剪贴板模式",
    "output_xdotool_missing": "未找到 xdotool，请安装以使用键盘输出：sudo apt install xdotool"
  }
```

- [ ] **Step 3: Update useTauriEvents.ts to handle structured errors**

In `src/hooks/useTauriEvents.ts`, modify the `pipeline:error` listener (around line 60) to handle both string errors and structured UserError objects:

Find the existing `pipeline:error` listener and replace it:

```typescript
      addListener<string | { code: string; details?: string; retry_count: number }>(
        'pipeline:error',
        (payload) => {
          const message =
            typeof payload === 'string'
              ? payload
              : t(`errors.${payload.code}`, { details: payload.details ?? '' });
          setPipelineError(message);
        }
      );
```

Add a new listener for `pipeline:warning`:

```typescript
      addListener<{ code: string; details?: string }>('pipeline:warning', (payload) => {
        const message = t(`errors.${payload.code}`, { details: payload.details ?? '' });
        toast(message, 'info');
      });
```

Also add `import { useTranslation } from 'react-i18next';` at the top and `const { t } = useTranslation();` inside the hook.

Also add `toast` import if not present:

```typescript
import { toast } from '../components/Toast';
```

- [ ] **Step 4: Update CapsuleError to use i18n for ACCESSIBILITY_REQUIRED**

In `src/components/Capsule/CapsuleError.tsx`, the existing code already handles `ACCESSIBILITY_REQUIRED` with `t('capsule.accessibilityRequired')`. No change needed for this specific case.

However, update the error display to handle i18n-translated messages (which are already strings at this point since useTauriEvents does the translation):

No changes needed — CapsuleError already displays `pipelineError` as a string, and we've moved the i18n translation into useTauriEvents.

- [ ] **Step 5: Verify frontend compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 6: Commit**

```bash
git add src/i18n/locales/ src/hooks/useTauriEvents.ts src/components/Capsule/CapsuleError.tsx
git commit -m "feat(i18n): add structured error codes with en/zh translations for pipeline errors"
```

---

## Phase 3: Code Architecture

### Task 10: Extract STT provider config constants

**Files:**
- Create: `src-tauri/src/stt/config.rs`
- Modify: `src-tauri/src/stt/mod.rs` (add `pub mod config;`)
- Modify: `src-tauri/src/lib.rs` (use config in test_stt/bench_stt commands)

- [ ] **Step 1: Create stt/config.rs with shared provider config**

```rust
use std::collections::HashMap;

/// Configuration for a Whisper-compatible STT provider.
pub struct SttProviderConfig {
    pub endpoint: &'static str,
    pub model: &'static str,
    pub extra_fields: Option<HashMap<&'static str, &'static str>>,
}

/// Returns the endpoint, model name, and any extra JSON fields for a given
/// Whisper-compatible STT provider. Used by `create_provider`, `test_stt_connection`,
/// and `bench_stt_connection` to avoid duplicating this mapping.
pub fn get_whisper_config(provider: &str) -> Option<SttProviderConfig> {
    match provider {
        "glm-asr" => Some(SttProviderConfig {
            endpoint: "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions",
            model: "",
            extra_fields: None,
        }),
        "openai-whisper" => Some(SttProviderConfig {
            endpoint: "https://api.openai.com/v1/audio/transcriptions",
            model: "whisper-1",
            extra_fields: None,
        }),
        "groq-whisper" => Some(SttProviderConfig {
            endpoint: "https://api.groq.com/openai/v1/audio/transcriptions",
            model: "whisper-large-v3",
            extra_fields: None,
        }),
        "siliconflow" => Some(SttProviderConfig {
            endpoint: "https://api.siliconflow.cn/v1/audio/transcriptions",
            model: "FunAudioLLM/SenseVoiceSmall",
            extra_fields: Some(HashMap::from([("response_format", "json")])),
        }),
        _ => None,
    }
}
```

- [ ] **Step 2: Register the module**

Add `pub mod config;` to `src-tauri/src/stt/mod.rs`.

- [ ] **Step 3: Update stt/mod.rs create_provider to use config**

In `src-tauri/src/stt/mod.rs` `create_provider()`, replace the inline match arms for glm-asr, openai-whisper, groq-whisper, siliconflow with:

```rust
    if let Some(cfg) = config::get_whisper_config(provider_name) {
        return Box::new(WhisperCompatProvider::new(WhisperCompatConfig {
            provider_name: provider_name.to_string(),
            endpoint: cfg.endpoint.to_string(),
            model: cfg.model.to_string(),
            extra_fields: cfg.extra_fields.map(|m| m.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        }));
    }
```

- [ ] **Step 4: Update lib.rs test_stt_connection and bench_stt_connection**

In `src-tauri/src/lib.rs`, replace the duplicated match arms in `test_stt_connection` (lines 229-277) and `bench_stt_connection` (lines 479-536) with calls to `stt::config::get_whisper_config()`.

Example for test_stt_connection:
```rust
        "deepgram" => { /* ... existing deepgram code ... */ }
        "assemblyai" => { /* ... existing assemblyai code ... */ }
        "cloud" => { /* ... existing cloud code ... */ }
        _ => {
            let cfg = stt::config::get_whisper_config(&provider)
                .ok_or_else(|| format!("Unknown STT provider: {}", provider))?;
            let form = reqwest::multipart::Form::new()
                .part("file", reqwest::multipart::Part::bytes(audio_bytes.clone())
                    .file_name("audio.wav")
                    .mime_str("audio/wav").map_err(|e| e.to_string())?);
            let mut req = client.post(cfg.endpoint)
                .header("Authorization", format!("Bearer {}", api_key))
                .multipart(form);
            if !cfg.model.is_empty() {
                req = req.query(&[("model", cfg.model)]);
            }
            // ... send and check response ...
        }
```

Apply the same pattern to `bench_stt_connection`.

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles without errors

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/stt/config.rs src-tauri/src/stt/mod.rs src-tauri/src/lib.rs
git commit -m "refactor(stt): extract provider config constants to eliminate triple duplication"
```

---

### Task 11: Extract commands from lib.rs

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/stt.rs`
- Create: `src-tauri/src/commands/llm.rs`
- Create: `src-tauri/src/commands/config.rs`
- Create: `src-tauri/src/commands/history.rs`
- Create: `src-tauri/src/commands/dictionary.rs`
- Create: `src-tauri/src/commands/misc.rs`
- Modify: `src-tauri/src/lib.rs`

This is a large structural refactor. The principle: move each `#[tauri::command]` function from `lib.rs` into its corresponding commands file. The `run()` function stays in `lib.rs` but references the commands via the module path.

- [ ] **Step 1: Create commands/mod.rs**

```rust
pub mod config;
pub mod dictionary;
pub mod history;
pub mod llm;
pub mod misc;
pub mod stt;
```

- [ ] **Step 2: Create commands/stt.rs**

Move these functions from `lib.rs` into `src-tauri/src/commands/stt.rs`:
- `test_stt_connection`
- `bench_stt_connection`

Add necessary imports at the top:

```rust
use crate::{error, pipeline, storage, stt, SessionTokenStore};
use tauri::Emitter;
```

Make functions `pub` so they can be referenced in the `generate_handler!` macro.

- [ ] **Step 3: Create commands/llm.rs**

Move these functions from `lib.rs` into `src-tauri/src/commands/llm.rs`:
- `test_llm_connection`
- `bench_llm_connection`
- `fetch_llm_models`

- [ ] **Step 4: Create commands/config.rs**

Move these functions from `lib.rs`:
- `get_config`
- `update_config`
- `set_auto_start`
- `set_session_token`

These need access to `storage::ConfigManager`, `HotkeyModeCache`, `CloseToTrayCache`, `SessionTokenStore`.

- [ ] **Step 5: Create commands/history.rs**

Move these functions:
- `get_history`
- `clear_history`

- [ ] **Step 6: Create commands/dictionary.rs**

Move these functions:
- `get_dictionary`
- `add_dictionary_entry`
- `remove_dictionary_entry`

- [ ] **Step 7: Create commands/misc.rs**

Move these functions:
- `check_accessibility_permission`
- `request_accessibility_permission`
- `pause_hotkey`
- `resume_hotkey`
- `update_hotkey`

- [ ] **Step 8: Update lib.rs — remove moved functions, add module, update generate_handler**

At the top of `lib.rs`, add:
```rust
pub mod commands;
```

Remove all the moved `#[tauri::command]` function bodies from `lib.rs`. Keep only:
- Structs: `HotkeyModeCache`, `CloseToTrayCache`, `SessionTokenStore`, `TrayHandle`, `WindowState`
- Helper functions that stay: `api_base_url`, `build_tray_menu`, `refresh_tray`, `default_shortcut`, `parse_hotkey`, `build_shortcut_handler`, `run`
- Simple wrappers that stay: `start_recording`, `stop_recording`, `abort_recording`

Update the `invoke_handler` in `run()` (line 1313) to use module paths:

```rust
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            abort_recording,
            commands::misc::check_accessibility_permission,
            commands::misc::request_accessibility_permission,
            commands::config::get_config,
            commands::config::update_config,
            commands::stt::test_stt_connection,
            commands::llm::test_llm_connection,
            commands::llm::bench_llm_connection,
            commands::stt::bench_stt_connection,
            commands::llm::fetch_llm_models,
            commands::history::get_history,
            commands::history::clear_history,
            commands::dictionary::get_dictionary,
            commands::dictionary::add_dictionary_entry,
            commands::dictionary::remove_dictionary_entry,
            commands::misc::update_hotkey,
            commands::misc::pause_hotkey,
            commands::misc::resume_hotkey,
            commands::config::set_auto_start,
            commands::config::set_session_token,
        ])
```

- [ ] **Step 9: Verify it compiles and existing tests pass**

Run: `cd src-tauri && cargo test && cargo check`
Expected: all tests pass, no errors

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands/ src-tauri/src/lib.rs
git commit -m "refactor: extract Tauri commands into commands/ module"
```

---

### Task 12: Extract tray.rs and hotkey.rs from lib.rs

**Files:**
- Create: `src-tauri/src/tray.rs`
- Create: `src-tauri/src/hotkey.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create tray.rs**

Move from `lib.rs` into `src-tauri/src/tray.rs`:
- `TrayHandle` struct
- `build_tray_menu` function
- `refresh_tray` function

Add imports:
```rust
use crate::pipeline;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::Emitter;
```

Note: `refresh_tray` is called from multiple places in `lib.rs` `run()`. Make it `pub` and reference as `crate::tray::refresh_tray` or re-export from `lib.rs`.

- [ ] **Step 2: Create hotkey.rs**

Move from `lib.rs` into `src-tauri/src/hotkey.rs`:
- `default_shortcut` function
- `parse_hotkey` function + its tests
- `build_shortcut_handler` function

Add imports:
```rust
use crate::pipeline;
use crate::tray;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
```

- [ ] **Step 3: Update lib.rs**

Add modules:
```rust
pub mod hotkey;
pub mod tray;
```

Remove the moved functions and structs. Keep only:
- `api_base_url`
- `HotkeyModeCache`, `CloseToTrayCache`, `SessionTokenStore`, `WindowState`
- `start_recording`, `stop_recording`, `abort_recording`
- `run()`

In `run()`, update references:
- `build_tray_menu` → `tray::build_tray_menu`
- `refresh_tray` → `tray::refresh_tray`
- `parse_hotkey` → `hotkey::parse_hotkey`
- `default_shortcut` → `hotkey::default_shortcut`
- `build_shortcut_handler` → `hotkey::build_shortcut_handler`

- [ ] **Step 4: Verify it compiles and tests pass**

Run: `cd src-tauri && cargo test && cargo check`
Expected: all tests pass (including hotkey tests now in hotkey.rs)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/hotkey.rs src-tauri/src/lib.rs
git commit -m "refactor: extract tray and hotkey modules from lib.rs"
```

---

### Task 13: Split pipeline.rs stop() into methods

**Files:**
- Modify: `src-tauri/src/pipeline.rs`

- [ ] **Step 1: Extract transcribe() method**

Create a private method that handles the STT wait phase (lines 709-746 of the current `stop()`). This covers:
- Waiting for stt_done notification (with timeout)
- Checking abort flag
- Getting raw_text from accumulated_text
- Emitting "no speech detected" error

```rust
    /// Wait for STT to finalize and return the transcribed text.
    /// Returns None if no speech was detected or pipeline was aborted.
    async fn wait_for_stt(&self) -> Result<Option<String>> {
        let stt_done = self.stt_done.clone();
        tokio::select! {
            _ = stt_done.notified() => {
                tracing::debug!("STT task completed");
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(STT_FINALIZE_TIMEOUT_SECS)) => {
                tracing::warn!("STT timed out after {}s", STT_FINALIZE_TIMEOUT_SECS);
            }
        }

        if self.abort_flag.load(Ordering::SeqCst) {
            tracing::info!("Pipeline aborted after STT wait");
            return Ok(None);
        }

        let raw_text = self
            .accumulated_text
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .trim()
            .to_string();

        if raw_text.is_empty() {
            let _ = self.app_handle.emit(
                "pipeline:error",
                "No speech detected. Please try again.",
            );
            self.set_state(PipelineState::Idle);
            return Ok(None);
        }

        Ok(Some(raw_text))
    }
```

- [ ] **Step 2: Extract polish() method**

Create a private method for the LLM polishing phase (lines 751-839):

```rust
    /// Run LLM polish on the transcribed text. Returns (final_text, llm_elapsed).
    /// If polish is disabled or fails, returns the raw text.
    async fn polish_text(
        &self,
        raw_text: &str,
        config: &storage::AppConfig,
        app_ctx: &app_detector::AppContext,
        dictionary_words: Vec<String>,
        selected_text: Option<String>,
    ) -> (String, std::time::Duration) {
        // ... LLM polish logic from current stop() ...
    }
```

- [ ] **Step 3: Extract save_history() method**

Create a private method for saving to history (lines 870-889):

```rust
    async fn save_history(
        &self,
        raw_text: &str,
        final_text: &str,
        app_ctx: &app_detector::AppContext,
        duration_ms: Option<i64>,
    ) {
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let entry = storage::HistoryEntry {
            id: 0,
            created_at: now,
            app_name: app_ctx.app_name.clone(),
            app_type: format!("{:?}", app_ctx.app_type),
            raw_text: raw_text.to_string(),
            polished_text: final_text.to_string(),
            language: None,
            duration_ms,
        };
        if let Err(e) = self
            .app_handle
            .state::<storage::HistoryStore>()
            .add(entry)
            .await
        {
            tracing::error!("Failed to save history: {}", e);
        }
    }
```

- [ ] **Step 4: Rewrite stop() to use the extracted methods**

The new `stop()` becomes a high-level orchestrator:

```rust
    pub async fn stop(&self) -> Result<()> {
        // Phase 1: Acquire lock, transition state
        let guard = self.pipeline_lock.lock().await;
        if self.state.compare_exchange(
            PipelineState::Recording.as_u8(),
            PipelineState::Transcribing.as_u8(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        ).is_err() {
            return Ok(());
        }
        self.set_state(PipelineState::Transcribing);

        let stop_start = std::time::Instant::now();

        // Capture selected text, stop audio, preload resources
        // (existing code from lines 607-672)
        // ... selected text capture, audio stop, preload config/app_ctx/dictionary ...
        drop(guard);

        // Phase 2: Wait for STT
        let raw_text = match self.wait_for_stt().await? {
            Some(text) => text,
            None => return Ok(()),
        };
        let stt_elapsed = stop_start.elapsed();

        // Phase 3: Polish with LLM
        let (final_text, llm_elapsed) = self.polish_text(
            &raw_text, &config, &app_ctx, dictionary_words, selected_text,
        ).await;

        // Phase 4: Output
        if let Err(e) = self.output_text(&final_text, &app_ctx.app_name, &config).await {
            tracing::error!("Output failed: {}", e);
            let _ = self.app_handle.emit("pipeline:error", format!("Output failed: {e}"));
        }

        // Phase 5: Save and finalize
        let duration_ms = self.recording_start.lock().unwrap_or_else(|e| e.into_inner()).take()
            .map(|start| start.elapsed().as_millis() as i64);
        self.save_history(&raw_text, &final_text, &app_ctx, duration_ms).await;

        // Emit timing
        let total = stop_start.elapsed();
        let _ = self.app_handle.emit("pipeline:timing", serde_json::json!({
            "stt_ms": stt_elapsed.as_millis() as u64,
            "llm_ms": llm_elapsed.as_millis() as u64,
            "total_ms": total.as_millis() as u64,
            "recording_ms": duration_ms,
        }));

        self.set_state(PipelineState::Idle);
        Ok(())
    }
```

- [ ] **Step 5: Verify it compiles and tests pass**

Run: `cd src-tauri && cargo test && cargo check`
Expected: all tests pass, no errors

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/pipeline.rs
git commit -m "refactor(pipeline): extract stop() phases into wait_for_stt, polish_text, save_history"
```

---

### Task 14: Migrate trait return types to AppError (Phase 3.4)

**Files:**
- Modify: `src-tauri/src/stt/mod.rs` (SttProvider trait)
- Modify: `src-tauri/src/llm/mod.rs` (LlmProvider trait)
- Modify: `src-tauri/src/output/mod.rs` (TextOutput trait)
- Modify: `src-tauri/src/stt/whisper_compat.rs`
- Modify: `src-tauri/src/stt/deepgram.rs`
- Modify: `src-tauri/src/stt/assemblyai.rs`
- Modify: `src-tauri/src/stt/cloud.rs`
- Modify: `src-tauri/src/llm/openai.rs`
- Modify: `src-tauri/src/llm/cloud.rs`
- Modify: `src-tauri/src/output/keyboard.rs`
- Modify: `src-tauri/src/output/clipboard.rs`

- [ ] **Step 1: Update SttProvider trait**

In `src-tauri/src/stt/mod.rs`, change the trait methods from returning `anyhow::Result` to `Result<T, crate::error::AppError>`:

```rust
use crate::error::AppError;

#[async_trait]
pub trait SttProvider: Send + Sync {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError>;
    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError>;
    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError>;
    async fn disconnect(&mut self) -> Result<Option<String>, AppError>;
    fn name(&self) -> &str;
}
```

- [ ] **Step 2: Update each STT provider implementation**

For each file (`whisper_compat.rs`, `deepgram.rs`, `assemblyai.rs`, `cloud.rs`):
- Replace `anyhow::Result` with `Result<_, crate::error::AppError>`
- Replace `anyhow::bail!(...)` with `return Err(crate::error::AppError::Auth(...))` or appropriate variant
- Replace `?` operator on reqwest errors — they'll auto-convert via the `From<reqwest::Error>` impl
- Replace `?` on other errors with `.map_err(|e| AppError::Network(e.to_string()))` or similar

- [ ] **Step 3: Update LlmProvider trait**

In `src-tauri/src/llm/mod.rs`:

```rust
use crate::error::AppError;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn polish(
        &self,
        config: &LlmConfig,
        req: &PolishRequest,
        on_chunk: Option<&ChunkCallback>,
    ) -> Result<PolishResponse, AppError>;
    fn name(&self) -> &str;
}
```

- [ ] **Step 4: Update each LLM provider implementation**

For `openai.rs` and `cloud.rs`:
- Replace `anyhow::Result` with `Result<_, crate::error::AppError>`
- Replace `anyhow::bail!(...)` with appropriate AppError variants

- [ ] **Step 5: Update TextOutput trait**

In `src-tauri/src/output/mod.rs`:

```rust
use crate::error::AppError;

#[async_trait]
pub trait TextOutput: Send + Sync {
    async fn type_text(&self, text: &str) -> Result<(), AppError>;
    fn mode(&self) -> OutputMode;
}
```

- [ ] **Step 6: Update keyboard.rs and clipboard.rs**

Replace `anyhow::Result` with `Result<_, crate::error::AppError>` in both files.

- [ ] **Step 7: Update pipeline.rs to handle AppError**

In `pipeline.rs`, the `stop()` method calls providers that now return `AppError`. Update error handling:
- Provider calls that previously returned `anyhow::Error` now return `AppError`
- Use `e.to_user_error()` for frontend-facing errors
- The `?` operator works directly since `pipeline::stop` returns `anyhow::Result<()>`

Add a `From<AppError>` impl for `anyhow::Error` if needed, or use `.map_err()` at the call sites.

- [ ] **Step 8: Remove anyhow dependency from provider files**

The provider files no longer need `use anyhow::Result` — they use `Result<_, AppError>` instead. Remove the `anyhow` imports from the updated files.

- [ ] **Step 9: Verify everything compiles and tests pass**

Run: `cd src-tauri && cargo test && cargo check`
Expected: all tests pass

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/
git commit -m "refactor: migrate SttProvider, LlmProvider, TextOutput traits to return AppError"
```

---

### Task 15: Add tests for new functionality

**Files:**
- Create: `src-tauri/src/error.rs` tests (inline in error.rs)
- Create: `src-tauri/src/stt/config.rs` tests (inline)

- [ ] **Step 1: Add error.rs tests**

Add a test module at the bottom of `src-tauri/src/error.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_error_is_retryable() {
        let err = AppError::Network("connection reset".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_timeout_is_retryable() {
        let err = AppError::Timeout(Duration::from_secs(30));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_500_is_retryable() {
        let err = AppError::Api { status: 500, body: "internal error".to_string() };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_401_is_not_retryable() {
        let err = AppError::Api { status: 401, body: "unauthorized".to_string() };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_auth_not_retryable() {
        let err = AppError::Auth("bad key".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_output_not_retryable() {
        let err = AppError::Output("enigo failed".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_401_maps_to_invalid_key_code() {
        let err = AppError::Api { status: 401, body: "".to_string() };
        let ue = err.to_user_error();
        assert_eq!(ue.code, "stt_invalid_key");
    }

    #[test]
    fn test_500_maps_to_stt_failed_code() {
        let err = AppError::Api { status: 500, body: "".to_string() };
        let ue = err.to_user_error();
        assert_eq!(ue.code, "stt_failed");
    }

    #[test]
    fn test_with_retry_count() {
        let err = AppError::Timeout(Duration::from_secs(10));
        let ue = err.with_retry_count(2);
        assert_eq!(ue.retry_count, 2);
    }
}
```

- [ ] **Step 2: Add stt/config.rs tests**

Add a test module at the bottom of `src-tauri/src/stt/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm_asr_config() {
        let cfg = get_whisper_config("glm-asr").unwrap();
        assert!(cfg.endpoint.contains("bigmodel.cn"));
        assert!(cfg.model.is_empty());
    }

    #[test]
    fn test_openai_whisper_config() {
        let cfg = get_whisper_config("openai-whisper").unwrap();
        assert_eq!(cfg.model, "whisper-1");
    }

    #[test]
    fn test_groq_whisper_config() {
        let cfg = get_whisper_config("groq-whisper").unwrap();
        assert_eq!(cfg.model, "whisper-large-v3");
    }

    #[test]
    fn test_siliconflow_has_extra_fields() {
        let cfg = get_whisper_config("siliconflow").unwrap();
        assert!(cfg.extra_fields.is_some());
        let fields = cfg.extra_fields.unwrap();
        assert_eq!(fields.get("response_format"), Some(&"json"));
    }

    #[test]
    fn test_unknown_provider_returns_none() {
        assert!(get_whisper_config("unknown").is_none());
    }

    #[test]
    fn test_deepgram_not_in_whisper_config() {
        assert!(get_whisper_config("deepgram").is_none());
    }
}
```

- [ ] **Step 3: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: all tests pass (including new tests in error, config, and existing hotkey tests)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/error.rs src-tauri/src/stt/config.rs
git commit -m "test: add unit tests for AppError retryability, UserError codes, and STT config"
```

---

## Self-Review Checklist

- [x] **Spec coverage:** Every section of the design spec maps to a task
  - Phase 1: NVIDIA detection (#36) → Task 1, README (#34/35/37) → Task 2
  - Phase 2.1 retry → Tasks 5-6, 2.2 shared client → Task 4, 2.3 fallback → Task 7, 2.4 timeout → inline in Tasks 5-6, 2.5 Linux detection → Task 8, 2.6 i18n → Task 9
  - Phase 3.1 lib.rs split → Tasks 11-12, 3.2 stop() split → Task 13, 3.3 config constants → Task 10, 3.4 AppError migration → Task 14, 3.5 tests → Task 15
- [x] **Placeholder scan:** No TBD/TODO/fill-in-later in the plan
- [x] **Type consistency:** `AppError`, `UserError`, `SttProviderConfig` definitions match across all tasks
- [x] **No contradictions:** Phase 2 uses retry inside providers (access to specific error types). Phase 3 migrates traits to AppError. These don't conflict.
