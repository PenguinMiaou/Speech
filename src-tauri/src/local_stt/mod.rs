//! On-device speech recognition: the worker process and the models it runs.

pub mod catalog;
pub mod install;
pub mod supervisor;

pub use supervisor::{
    default_worker_command, global, set_global, LocalSttError, WorkerCommand, WorkerSupervisor,
};

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Slightly under the pipeline's 120-second finalize timeout, so the worker's own error is what surfaces.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(115);
/// Outer limit on waiting for a result. The supervisor detects a stuck worker well before this:
/// the base allowance, plus the longest recording, plus a margin to report the error.
pub const FINALIZE_TIMEOUT: Duration = Duration::from_secs(115 + 720 + 15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Sets up the models directory and the worker supervisor at app startup.
pub fn init(data_dir: &Path) {
    catalog::set_models_dir(data_dir.join("models"));
    let command = match default_worker_command() {
        Ok(command) => command,
        Err(error) => {
            tracing::warn!("local speech worker path unavailable: {error}");
            return;
        }
    };
    let supervisor = WorkerSupervisor::new(command, REQUEST_TIMEOUT, IDLE_TIMEOUT);
    // Off the Mac the recognizer may run on a graphics processor that turns out not to
    // be up to it; it then does without.
    let supervisor = Arc::new(if cfg!(target_os = "macos") {
        supervisor
    } else {
        supervisor.falling_back()
    });
    set_global(supervisor.clone());
    std::thread::spawn(move || loop {
        std::thread::sleep(IDLE_CHECK_INTERVAL);
        supervisor.stop_if_idle();
    });
}
