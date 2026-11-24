use crate::meetings::{Meeting, Summary};

/// Past meetings, the newest first.
#[tauri::command]
pub fn meetings_list() -> Vec<Summary> {
    crate::meetings::list()
}

/// The meetings that hold every word looked for, the newest first.
#[tauri::command]
pub async fn meetings_search(query: String) -> Vec<crate::meetings::Found> {
    tokio::task::spawn_blocking(move || crate::meetings::search(&query))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn meeting_get(id: String) -> Result<Meeting, String> {
    crate::meetings::get(&id)
}

/// Saves a meeting, replacing the one with the same id.
#[tauri::command]
pub fn meeting_save(meeting: Meeting) -> Result<(), String> {
    crate::meetings::save(&meeting)
}

/// Puts a meeting in a folder, or in none where `folder` is empty.
#[tauri::command]
pub fn meeting_folder(id: String, folder: String) -> Result<(), String> {
    crate::meetings::set_folder(&id, &folder)
}

#[tauri::command]
pub async fn meeting_delete(id: String) -> Result<(), String> {
    // Waits, off the window's thread, for a sound being packed.
    blocking(move || crate::meetings::delete(&id)).await
}

/// Runs work that may wait for a meeting's sound being packed on a thread of its own, so
/// that the window never waits with it.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

fn shown(path: std::path::PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

/// Where the sound of a meeting is kept, if it still is.
#[tauri::command]
pub fn meeting_audio(id: String) -> Option<String> {
    crate::meetings::audio(&id).map(shown)
}

/// Gives the recording that has just ended to the meeting written from it, at once, and
/// packs it as FLAC afterwards on a thread of its own: the same samples in about half the
/// room. The window is told of the packed file with a `meeting:audio-packed` event; where
/// packing fails the recording stays as it was made.
#[tauri::command]
pub async fn meeting_audio_keep(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<String>, String> {
    if crate::meeting_record::is_recording() {
        return Ok(None);
    }
    blocking(move || {
        let Some(recording) = crate::meeting_record::kept_recording() else {
            return Ok(crate::meetings::audio(&id).map(shown));
        };
        let kept = crate::meetings::keep_audio(&id, &recording)?;
        pack_later(app, id);
        Ok(Some(shown(kept)))
    })
    .await
}

fn pack_later(app: tauri::AppHandle, id: String) {
    use tauri::Emitter;
    let _ = std::thread::Builder::new()
        .name("opentypeless-pack-audio".to_string())
        .spawn(move || {
            // A meeting being recorded keeps the machine to itself; the sound is packed
            // the next time the app starts.
            if crate::meeting_record::is_recording() {
                return;
            }
            match crate::meetings::pack_audio(&id) {
                Ok(Some(path)) => {
                    let _ = app.emit(
                        "meeting:audio-packed",
                        serde_json::json!({ "id": id, "path": shown(path) }),
                    );
                }
                Ok(None) => {}
                Err(error) => crate::applog::note(
                    "meeting",
                    &format!("the sound of a meeting could not be packed: {error}"),
                ),
            }
        });
}

/// Packs the sound of every meeting still kept as WAV, one after another, on its own
/// thread, telling the window of each with a `meeting:audio-packed` event.
pub fn pack_kept_audio(app: tauri::AppHandle) {
    use tauri::Emitter;
    let _ = std::thread::Builder::new()
        .name("opentypeless-pack-audio".to_string())
        .spawn(move || {
            // Sound kept aside for an undo from when the app last ran can no longer be asked
            // for.
            crate::meetings::drop_cuts();
            // Leaves the app's start to itself.
            std::thread::sleep(std::time::Duration::from_secs(20));
            let mut count = 0;
            crate::meetings::pack_all(|id, path| {
                count += 1;
                let _ = app.emit(
                    "meeting:audio-packed",
                    serde_json::json!({ "id": id, "path": shown(path.to_path_buf()) }),
                );
            });
            if count > 0 {
                crate::applog::note("meeting", &format!("packed the sound of {count} meetings"));
            }
        });
}

/// Copies the sound of a meeting to a place the user chose.
#[tauri::command]
pub fn meeting_audio_save(id: String, path: String) -> Result<(), String> {
    let kept = crate::meetings::audio(&id).ok_or("no_recording")?;
    std::fs::copy(kept, path)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Cuts the sound of a meeting off at `end_ms` from its start.
#[tauri::command]
pub async fn meeting_audio_trim(id: String, end_ms: u64) -> Result<(), String> {
    blocking(move || crate::meetings::trim_audio(&id, end_ms)).await
}

/// Puts back the sound a meeting had before its last cut; false where none is kept.
#[tauri::command]
pub async fn meeting_audio_cut_undo(id: String) -> Result<bool, String> {
    blocking(move || crate::meetings::undo_cut(&id)).await
}

/// Lets the sound from before a meeting's last cut go, once the cut can no longer be undone.
#[tauri::command]
pub async fn meeting_audio_cut_keep(id: String) -> Result<(), String> {
    blocking(move || crate::meetings::keep_cut(&id)).await
}

#[tauri::command]
pub async fn meeting_audio_delete(id: String) -> Result<(), String> {
    blocking(move || crate::meetings::delete_audio(&id)).await
}
