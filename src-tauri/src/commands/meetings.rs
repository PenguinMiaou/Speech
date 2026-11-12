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
pub fn meeting_delete(id: String) -> Result<(), String> {
    crate::meetings::delete(&id)
}

fn shown(path: std::path::PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

/// Where the sound of a meeting is kept, if it still is.
#[tauri::command]
pub fn meeting_audio(id: String) -> Option<String> {
    crate::meetings::audio(&id).map(shown)
}

/// Gives the recording that has just ended to the meeting written from it.
#[tauri::command]
pub fn meeting_audio_keep(id: String) -> Result<Option<String>, String> {
    if crate::meeting_record::is_recording() {
        return Ok(None);
    }
    match crate::meeting_record::kept_recording() {
        Some(recording) => {
            crate::meetings::keep_audio(&id, &recording).map(|kept| Some(shown(kept)))
        }
        None => Ok(crate::meetings::audio(&id).map(shown)),
    }
}

/// Copies the sound of a meeting to a place the user chose.
#[tauri::command]
pub fn meeting_audio_save(id: String, path: String) -> Result<(), String> {
    let kept = crate::meetings::audio(&id).ok_or("no_recording")?;
    std::fs::copy(kept, path)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn meeting_audio_delete(id: String) -> Result<(), String> {
    crate::meetings::delete_audio(&id)
}
