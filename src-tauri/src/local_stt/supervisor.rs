//! Owns the `ot-stt-worker` child process. Requests are serial, and a crashed or stuck worker is replaced on the next call.

use ot_stt_protocol::{read_frame, write_frame, Request, Response, MAX_PCM_BYTES};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalSttError {
    ModelMissing,
    WorkerCrashed(String),
    Timeout,
    Engine { code: String, message: String },
    Io(String),
}

impl std::fmt::Display for LocalSttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelMissing => write!(f, "local_model_not_installed"),
            Self::WorkerCrashed(detail) => write!(f, "local speech worker stopped: {detail}"),
            Self::Timeout => write!(f, "local speech worker timed out"),
            Self::Engine { code, message } => {
                write!(f, "local speech engine error {code}: {message}")
            }
            Self::Io(detail) => write!(f, "local speech worker I/O error: {detail}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<std::io::Result<Response>>,
    /// Audio that followed an `Isolated` reply.
    audio: Receiver<Vec<f32>>,
    model: Option<PathBuf>,
    last_used: Instant,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct WorkerSupervisor {
    command: WorkerCommand,
    request_timeout: Duration,
    idle_timeout: Duration,
    state: Mutex<Option<Running>>,
}

fn spawn_worker(command: &WorkerCommand) -> Result<Running, LocalSttError> {
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        process.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = process
        .spawn()
        .map_err(|e| LocalSttError::Io(e.to_string()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| LocalSttError::Io("worker has no stdin".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LocalSttError::Io("worker has no stdout".into()))?;

    let (sender, responses) = mpsc::channel();
    let (audio_sender, audio) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_frame::<_, Response>(&mut reader) {
                Ok(Some(response)) => {
                    if let Response::Isolated { pcm_bytes } = &response {
                        let mut raw = vec![0u8; (*pcm_bytes).min(MAX_PCM_BYTES) as usize];
                        if let Err(error) = std::io::Read::read_exact(&mut reader, &mut raw) {
                            let _ = sender.send(Err(error));
                            return;
                        }
                        let samples = raw
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                        if audio_sender.send(samples).is_err() {
                            return;
                        }
                    }
                    if sender.send(Ok(response)).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
    });

    Ok(Running {
        child,
        stdin: Some(stdin),
        responses,
        audio,
        model: None,
        last_used: Instant::now(),
    })
}

/// Sends one request and waits for its reply. Writing happens on its own thread because a stuck
/// worker stops draining the pipe, and a blocked write must not outlive the timeout.
const SAMPLE_RATE_HZ: f64 = 16_000.0;

/// How the worker ended, in the form [`failure_code`] reads: Windows reports a crash as an
/// exit code such as 0xc000001d, other systems as a signal.
fn exit_note(status: std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) if !(0..=255).contains(&code) => format!("exit code: {:#010x}", code as u32),
        _ => status.to_string(),
    }
}

/// The error for a worker that stopped answering, with its exit status when it has ended.
fn stopped(running: &mut Running, what: &str) -> LocalSttError {
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match running.child.try_wait() {
            Ok(Some(status)) => {
                return LocalSttError::WorkerCrashed(format!("{what} ({})", exit_note(status)))
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => return LocalSttError::WorkerCrashed(what.to_string()),
        }
    }
}

/// Names why a model could not be run, for the message the user sees.
pub fn failure_code(error: &LocalSttError) -> &'static str {
    match error {
        // An instruction the processor does not have: STATUS_ILLEGAL_INSTRUCTION or SIGILL.
        LocalSttError::WorkerCrashed(detail)
            if detail.contains("0xc000001d") || detail.contains("signal: 4") =>
        {
            "engine_cpu"
        }
        LocalSttError::Timeout => "engine_slow",
        // The worker program could not be started at all.
        LocalSttError::Io(_) => "engine_blocked",
        _ => "engine",
    }
}

fn exchange(
    running: &mut Running,
    payload: Vec<u8>,
    timeout: Duration,
) -> Result<Response, LocalSttError> {
    let deadline = Instant::now() + timeout;
    let mut stdin = running
        .stdin
        .take()
        .ok_or_else(|| LocalSttError::WorkerCrashed("stdin closed".into()))?;
    let (written, write_result) = mpsc::channel();
    std::thread::spawn(move || {
        let result = stdin.write_all(&payload).and_then(|_| stdin.flush());
        let _ = written.send((stdin, result));
    });
    match write_result.recv_timeout(timeout) {
        Ok((stdin, Ok(()))) => running.stdin = Some(stdin),
        Ok((_, Err(error))) => return Err(LocalSttError::WorkerCrashed(error.to_string())),
        Err(RecvTimeoutError::Timeout) => return Err(LocalSttError::Timeout),
        Err(RecvTimeoutError::Disconnected) => {
            return Err(LocalSttError::WorkerCrashed("writer stopped".into()))
        }
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    let response = match running.responses.recv_timeout(remaining) {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => return Err(stopped(running, &error.to_string())),
        Err(RecvTimeoutError::Timeout) => return Err(LocalSttError::Timeout),
        Err(RecvTimeoutError::Disconnected) => return Err(stopped(running, "worker exited")),
    };
    running.last_used = Instant::now();
    Ok(response)
}

fn encode(request: &Request) -> Result<Vec<u8>, LocalSttError> {
    let mut buffer = Vec::new();
    write_frame(&mut buffer, request).map_err(|e| LocalSttError::Io(e.to_string()))?;
    Ok(buffer)
}

impl WorkerSupervisor {
    pub fn new(command: WorkerCommand, request_timeout: Duration, idle_timeout: Duration) -> Self {
        Self {
            command,
            request_timeout,
            idle_timeout,
            state: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Running>> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn load_locked(
        &self,
        state: &mut Option<Running>,
        model_path: &Path,
    ) -> Result<(), LocalSttError> {
        if !model_path.is_file() {
            return Err(LocalSttError::ModelMissing);
        }
        if state.as_ref().and_then(|running| running.model.as_deref()) == Some(model_path) {
            return Ok(());
        }
        if state.is_none() {
            *state = Some(spawn_worker(&self.command)?);
        }
        let running = state.as_mut().expect("worker was just started");
        let payload = encode(&Request::Load {
            model_path: model_path.to_string_lossy().into_owned(),
        })?;
        let outcome = match exchange(running, payload, self.request_timeout) {
            Ok(Response::Ready { .. }) => {
                running.model = Some(model_path.to_path_buf());
                Ok(())
            }
            Ok(Response::Error { code, message }) => Err(LocalSttError::Engine { code, message }),
            Ok(other) => Err(LocalSttError::WorkerCrashed(format!(
                "unexpected reply {other:?}"
            ))),
            Err(error) => Err(error),
        };
        if outcome.is_err() {
            *state = None;
        }
        outcome
    }

    /// One isolation request and its reply. A failure stops the worker, so the next
    /// recording starts clean.
    fn isolation_exchange(
        &self,
        state: &mut Option<Running>,
        payload: Vec<u8>,
    ) -> Result<Response, LocalSttError> {
        if state.is_none() {
            *state = Some(spawn_worker(&self.command)?);
        }
        let running = state.as_mut().expect("worker was just started");
        match exchange(running, payload, self.request_timeout) {
            Ok(Response::Error { code, message }) => Err(LocalSttError::Engine { code, message }),
            Ok(response) => Ok(response),
            Err(error) => {
                *state = None;
                Err(error)
            }
        }
    }

    /// Starts keeping only the voice in `voice_sample` for the recording now beginning.
    pub fn isolate_begin(
        &self,
        enroll_model: &Path,
        chunk_model: &Path,
        voice_sample: &Path,
    ) -> Result<(), LocalSttError> {
        let text = |path: &Path| path.to_string_lossy().into_owned();
        let payload = encode(&Request::IsolateBegin {
            enroll_model: text(enroll_model),
            chunk_model: text(chunk_model),
            voice_sample: text(voice_sample),
        })?;
        self.isolation_exchange(&mut self.lock(), payload)
            .map(|_| ())
    }

    /// Tells who spoke when in a whole recording. The audio goes to the worker as a file
    /// in `scratch_dir`, removed again before this returns.
    pub fn diarize(
        &self,
        segmentation_model: &Path,
        embedding_model: &Path,
        pcm: &[f32],
        scratch_dir: &Path,
    ) -> Result<
        (
            Vec<ot_stt_protocol::SpeakerTurn>,
            Vec<ot_stt_protocol::SpeakerVoice>,
        ),
        LocalSttError,
    > {
        self.speakers(segmentation_model, embedding_model, pcm, scratch_dir, None)
    }

    /// The same for the next window of a recording still running: speakers keep their
    /// numbers from window to window, and `first` starts a new recording.
    pub fn speakers_window(
        &self,
        segmentation_model: &Path,
        embedding_model: &Path,
        pcm: &[f32],
        scratch_dir: &Path,
        first: bool,
    ) -> Result<
        (
            Vec<ot_stt_protocol::SpeakerTurn>,
            Vec<ot_stt_protocol::SpeakerVoice>,
        ),
        LocalSttError,
    > {
        self.speakers(
            segmentation_model,
            embedding_model,
            pcm,
            scratch_dir,
            Some(first),
        )
    }

    fn speakers(
        &self,
        segmentation_model: &Path,
        embedding_model: &Path,
        pcm: &[f32],
        scratch_dir: &Path,
        window: Option<bool>,
    ) -> Result<
        (
            Vec<ot_stt_protocol::SpeakerTurn>,
            Vec<ot_stt_protocol::SpeakerVoice>,
        ),
        LocalSttError,
    > {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let pcm_path = scratch_dir.join(format!(
            "speakers-{}-{}.pcm",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        // Written through a small buffer, so a long recording is not held twice.
        let written = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::io::BufWriter::new(std::fs::File::create(&pcm_path)?);
            for sample in pcm {
                file.write_all(&sample.to_le_bytes())?;
            }
            file.flush()
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&pcm_path);
            return Err(LocalSttError::Io(error.to_string()));
        }
        let text = |path: &Path| path.to_string_lossy().into_owned();
        let request = match window {
            Some(first) => Request::SpeakersWindow {
                segmentation_model: text(segmentation_model),
                embedding_model: text(embedding_model),
                pcm_path: text(&pcm_path),
                first,
            },
            None => Request::Diarize {
                segmentation_model: text(segmentation_model),
                embedding_model: text(embedding_model),
                pcm_path: text(&pcm_path),
            },
        };
        let payload = encode(&request);
        let audio_time = Duration::from_secs_f64(pcm.len() as f64 / SAMPLE_RATE_HZ);
        let answer = payload.and_then(|payload| {
            let mut state = self.lock();
            if state.is_none() {
                *state = Some(spawn_worker(&self.command)?);
            }
            let running = state.as_mut().expect("worker was just started");
            match exchange(running, payload, self.request_timeout + audio_time) {
                Ok(Response::Speakers { turns, voices }) => Ok((turns, voices)),
                Ok(Response::Error { code, message }) => {
                    Err(LocalSttError::Engine { code, message })
                }
                Ok(other) => {
                    *state = None;
                    Err(LocalSttError::WorkerCrashed(format!(
                        "unexpected reply {other:?}"
                    )))
                }
                Err(error) => {
                    *state = None;
                    Err(error)
                }
            }
        });
        let _ = std::fs::remove_file(&pcm_path);
        answer
    }

    /// Sends the next piece of the recording.
    pub fn isolate_feed(&self, pcm: &[f32]) -> Result<(), LocalSttError> {
        let mut payload = encode(&Request::IsolateFeed {
            pcm_bytes: (pcm.len() * 4) as u32,
        })?;
        for sample in pcm {
            payload.extend_from_slice(&sample.to_le_bytes());
        }
        self.isolation_exchange(&mut self.lock(), payload)
            .map(|_| ())
    }

    /// Ends the recording and returns its isolated audio.
    pub fn isolate_end(&self) -> Result<Vec<f32>, LocalSttError> {
        let mut state = self.lock();
        let payload = encode(&Request::IsolateEnd)?;
        match self.isolation_exchange(&mut state, payload)? {
            Response::Isolated { .. } => state
                .as_ref()
                .and_then(|running| running.audio.try_recv().ok())
                .ok_or_else(|| LocalSttError::WorkerCrashed("isolated audio missing".into())),
            other => {
                *state = None;
                Err(LocalSttError::WorkerCrashed(format!(
                    "unexpected reply {other:?}"
                )))
            }
        }
    }

    pub fn ensure_loaded(&self, model_path: &Path) -> Result<(), LocalSttError> {
        self.load_locked(&mut self.lock(), model_path)
    }

    pub fn transcribe(
        &self,
        model_path: &Path,
        pcm: &[f32],
        vocabulary: Vec<String>,
        prompt: Option<String>,
    ) -> Result<String, LocalSttError> {
        self.transcribe_in(model_path, pcm, vocabulary, prompt, None)
    }

    /// Recognizes speech known to be in `language` (an ISO code); `None` leaves the
    /// language to the model.
    pub fn transcribe_in(
        &self,
        model_path: &Path,
        pcm: &[f32],
        vocabulary: Vec<String>,
        prompt: Option<String>,
        language: Option<String>,
    ) -> Result<String, LocalSttError> {
        self.transcribe_heard(model_path, pcm, vocabulary, prompt, language)
            .map(|(text, _)| text)
    }

    /// The same, with the language the model heard when it was left to tell.
    pub fn transcribe_heard(
        &self,
        model_path: &Path,
        pcm: &[f32],
        vocabulary: Vec<String>,
        prompt: Option<String>,
        language: Option<String>,
    ) -> Result<(String, Option<String>), LocalSttError> {
        let pcm_bytes = pcm.len().saturating_mul(4);
        if pcm_bytes > MAX_PCM_BYTES as usize {
            return Err(LocalSttError::Engine {
                code: "audio_too_long".into(),
                message: "audio exceeds 12 minutes".into(),
            });
        }
        let mut state = self.lock();
        self.load_locked(&mut state, model_path)?;

        let mut payload = encode(&Request::Transcribe {
            vocabulary,
            prompt,
            language,
            pcm_bytes: pcm_bytes as u32,
        })?;
        payload.reserve(pcm_bytes);
        for sample in pcm {
            payload.extend_from_slice(&sample.to_le_bytes());
        }

        // Recognition takes time in proportion to the audio, so only the base allowance is fixed.
        let audio_time = Duration::from_secs_f64(pcm.len() as f64 / SAMPLE_RATE_HZ);
        let running = state.as_mut().expect("worker was just loaded");
        match exchange(running, payload, self.request_timeout + audio_time) {
            Ok(Response::Transcript { text, language, .. }) => Ok((text, language)),
            Ok(Response::Error { code, message }) => Err(LocalSttError::Engine { code, message }),
            Ok(other) => {
                *state = None;
                Err(LocalSttError::WorkerCrashed(format!(
                    "unexpected reply {other:?}"
                )))
            }
            Err(error) => {
                *state = None;
                Err(error)
            }
        }
    }

    /// Frees the model's memory once the worker has been unused for the idle timeout. Skips quietly while busy.
    pub fn stop_if_idle(&self) {
        if let Ok(mut state) = self.state.try_lock() {
            if state
                .as_ref()
                .is_some_and(|running| running.last_used.elapsed() >= self.idle_timeout)
            {
                *state = None;
            }
        }
    }

    pub fn stop(&self) {
        *self.lock() = None;
    }

    pub fn is_running(&self) -> bool {
        self.lock().is_some()
    }
}

static GLOBAL: OnceLock<Arc<WorkerSupervisor>> = OnceLock::new();

pub fn global() -> Option<Arc<WorkerSupervisor>> {
    GLOBAL.get().cloned()
}

pub fn set_global(supervisor: Arc<WorkerSupervisor>) {
    let _ = GLOBAL.set(supervisor);
}

/// The worker is bundled next to the main executable.
pub fn default_worker_command() -> std::io::Result<WorkerCommand> {
    let name = if cfg!(windows) {
        "ot-stt-worker.exe"
    } else {
        "ot-stt-worker"
    };
    Ok(WorkerCommand {
        program: std::env::current_exe()?.with_file_name(name),
        args: vec!["serve".to_string()],
    })
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use std::time::Instant;

    const FAKE_WORKER: &str = r#"
import json, os, struct, sys, time
mode = sys.argv[1]
marker = sys.argv[2] if len(sys.argv) > 2 else ''
stdin, stdout = sys.stdin.buffer, sys.stdout.buffer
def reply(value):
    body = json.dumps(value).encode()
    stdout.write(struct.pack('<I', len(body)) + body)
    stdout.flush()
while True:
    header = stdin.read(4)
    if len(header) < 4:
        sys.exit(0)
    request = json.loads(stdin.read(struct.unpack('<I', header)[0]))
    if request == 'Shutdown':
        sys.exit(0)
    if 'Load' in request:
        if mode == 'bad-load':
            reply({'Error': {'code': 'load_failed', 'message': 'bad file'}})
        else:
            reply({'Ready': {'backend': 'fake', 'load_ms': 0}})
        continue
    if 'Diarize' in request:
        samples = os.path.getsize(request['Diarize']['pcm_path']) // 4
        reply({'Speakers': {'turns': [{'start_ms': 0, 'end_ms': samples, 'speaker': 1}]}})
        continue
    if request == 'IsolateEnd':
        body = b''.join(struct.pack('<f', sample / 2) for sample in fed)
        reply({'Isolated': {'pcm_bytes': len(body)}})
        stdout.write(body)
        stdout.flush()
        fed = []
        continue
    if 'IsolateBegin' in request:
        fed = []
        reply('Isolating')
        continue
    if 'IsolateFeed' in request:
        raw = stdin.read(request['IsolateFeed']['pcm_bytes'])
        fed += [value[0] for value in struct.iter_unpack('<f', raw)]
        reply('Isolating')
        continue
    if mode == 'never-read':
        time.sleep(600)
    audio = stdin.read(request['Transcribe']['pcm_bytes'])
    if mode == 'crash-once' and not os.path.exists(marker):
        open(marker, 'w').close()
        sys.exit(3)
    if mode == 'hang':
        time.sleep(600)
    if mode == 'slow':
        time.sleep(1.5)
    time.sleep(0.02)
    reply({'Transcript': {'text': 'n=%d' % (len(audio) // 4), 'language': None, 'elapsed_ms': 0}})
"#;

    fn unique_path(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "ot-stt-test-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ))
    }

    #[test]
    fn a_recording_fed_piece_by_piece_comes_back_isolated_and_the_worker_still_transcribes() {
        let supervisor = supervisor("ok", "", 5_000, 60_000);
        let file = model_file();
        supervisor.isolate_begin(&file, &file, &file).unwrap();
        supervisor.isolate_feed(&[0.5, 0.25]).unwrap();
        supervisor.isolate_feed(&[1.0]).unwrap();
        assert_eq!(supervisor.isolate_end().unwrap(), vec![0.25, 0.125, 0.5]);

        let text = supervisor
            .transcribe(&file, &[0.0; 8], vec![], None)
            .unwrap();
        assert_eq!(text, "n=8");
    }

    pub(crate) fn model_file() -> PathBuf {
        let path = unique_path("model.gguf");
        std::fs::write(&path, b"fake").unwrap();
        path
    }

    pub(crate) fn supervisor(
        mode: &str,
        marker: &str,
        timeout_ms: u64,
        idle_ms: u64,
    ) -> WorkerSupervisor {
        WorkerSupervisor::new(
            WorkerCommand {
                program: PathBuf::from("python3"),
                args: vec![
                    "-I".to_string(),
                    "-c".to_string(),
                    FAKE_WORKER.to_string(),
                    mode.to_string(),
                    marker.to_string(),
                ],
            },
            Duration::from_millis(timeout_ms),
            Duration::from_millis(idle_ms),
        )
    }

    #[test]
    fn a_recording_comes_back_as_speaker_turns_and_its_copy_on_disk_is_removed() {
        let dir = unique_path("diarize");
        std::fs::create_dir_all(&dir).unwrap();
        let supervisor = supervisor("ok", "", 5_000, 60_000);
        let (turns, _) = supervisor
            .diarize(
                Path::new("seg.onnx"),
                Path::new("emb.onnx"),
                &[0.0; 7],
                &dir,
            )
            .unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!((turns[0].end_ms, turns[0].speaker), (7, 1));
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".pcm"))
            .collect();
        assert!(left.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_is_named_by_its_cause() {
        let crashed = |detail: &str| LocalSttError::WorkerCrashed(detail.to_string());
        assert_eq!(
            failure_code(&crashed("worker exited (exit code: 0xc000001d)")),
            "engine_cpu"
        );
        assert_eq!(
            failure_code(&crashed("worker exited (signal: 4 (SIGILL))")),
            "engine_cpu"
        );
        assert_eq!(
            failure_code(&crashed("worker exited (exit code: 0xc0000005)")),
            "engine"
        );
        assert_eq!(failure_code(&LocalSttError::Timeout), "engine_slow");
        assert_eq!(
            failure_code(&LocalSttError::Io("program not found".to_string())),
            "engine_blocked"
        );
        assert_eq!(
            failure_code(&LocalSttError::Engine {
                code: "load_failed".into(),
                message: "bad file".into()
            }),
            "engine"
        );
    }

    #[test]
    fn missing_model_returns_model_missing() {
        let worker = supervisor("ok", "", 5_000, 60_000);
        let error = worker
            .transcribe(&unique_path("absent.gguf"), &[0.0; 8], vec![], None)
            .unwrap_err();
        assert_eq!(error, LocalSttError::ModelMissing);
        assert!(!worker.is_running());
    }

    #[test]
    fn long_audio_gets_more_time() {
        let worker = supervisor("slow", "", 1_000, 60_000);
        let two_seconds = vec![0.0f32; 32_000];
        assert_eq!(
            worker
                .transcribe(&model_file(), &two_seconds, vec![], None)
                .unwrap(),
            "n=32000"
        );
    }

    #[test]
    fn transcribe_returns_worker_text() {
        let worker = supervisor("ok", "", 5_000, 60_000);
        assert_eq!(
            worker
                .transcribe(&model_file(), &[0.0; 8], vec![], None)
                .unwrap(),
            "n=8"
        );
        assert!(worker.is_running());
    }

    #[test]
    fn load_error_is_reported_as_engine_error() {
        let worker = supervisor("bad-load", "", 5_000, 60_000);
        let error = worker.ensure_loaded(&model_file()).unwrap_err();
        assert!(matches!(error, LocalSttError::Engine { code, .. } if code == "load_failed"));
    }

    #[test]
    fn crash_returns_error_then_next_call_respawns() {
        let marker = unique_path("crashed");
        let worker = supervisor("crash-once", marker.to_str().unwrap(), 5_000, 60_000);
        let model = model_file();
        let first = worker
            .transcribe(&model, &[0.0; 8], vec![], None)
            .unwrap_err();
        assert!(
            matches!(&first, LocalSttError::WorkerCrashed(detail) if detail.contains("exit") && detail.contains('3')),
            "{first:?}"
        );
        assert!(!worker.is_running());
        assert_eq!(
            worker.transcribe(&model, &[0.0; 8], vec![], None).unwrap(),
            "n=8"
        );
    }

    #[test]
    fn hang_times_out_and_kills() {
        let worker = supervisor("hang", "", 400, 60_000);
        let started = Instant::now();
        let error = worker
            .transcribe(&model_file(), &[0.0; 8], vec![], None)
            .unwrap_err();
        assert_eq!(error, LocalSttError::Timeout);
        assert!(!worker.is_running());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn worker_that_stops_reading_audio_still_times_out() {
        let worker = supervisor("never-read", "", 400, 60_000);
        // 160 KB fills the pipe, and 2.5 seconds of audio keeps the allowance short.
        let audio = vec![0.0f32; 40_000];
        let started = Instant::now();
        let error = worker
            .transcribe(&model_file(), &audio, vec![], None)
            .unwrap_err();
        assert_eq!(error, LocalSttError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(8));
    }

    #[test]
    fn concurrent_transcribes_are_serial_and_not_mixed() {
        let worker = Arc::new(supervisor("ok", "", 10_000, 60_000));
        let model = model_file();
        let handles: Vec<_> = (1..=8usize)
            .map(|n| {
                let (worker, model) = (worker.clone(), model.clone());
                std::thread::spawn(move || {
                    let text = worker
                        .transcribe(&model, &vec![0.0; n * 100], vec![], None)
                        .unwrap();
                    assert_eq!(text, format!("n={}", n * 100));
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn idle_stop_kills_only_after_the_idle_timeout() {
        let model = model_file();
        let busy = supervisor("ok", "", 5_000, 60_000);
        busy.ensure_loaded(&model).unwrap();
        busy.stop_if_idle();
        assert!(busy.is_running());

        let idle = supervisor("ok", "", 5_000, 0);
        idle.ensure_loaded(&model).unwrap();
        idle.stop_if_idle();
        assert!(!idle.is_running());
        assert_eq!(
            idle.transcribe(&model, &[0.0; 8], vec![], None).unwrap(),
            "n=8"
        );
    }

    #[test]
    fn missing_worker_binary_is_an_io_error() {
        let worker = WorkerSupervisor::new(
            WorkerCommand {
                program: unique_path("no-such-worker"),
                args: vec![],
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        assert!(matches!(
            worker.ensure_loaded(&model_file()),
            Err(LocalSttError::Io(_))
        ));
    }
}
