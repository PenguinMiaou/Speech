//! The offer to record a meeting or a call found going on: a small window in the corner
//! where the system shows its own notices, asking whether to begin. Nothing is recorded
//! until the user says so.

use crate::meeting_probe::MeetingApp;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

const LABEL: &str = "offer";
/// The offer window's size in logical pixels, and its distance from the screen's edges.
pub const WIDTH: f64 = 420.0;
pub const HEIGHT: f64 = 120.0;
const MARGIN: f64 = 28.0;

/// The meeting last offered, for the offer window to show when it opens.
static OFFERED: Mutex<Option<MeetingApp>> = Mutex::new(None);

/// Where the offer goes on a work area at `x`, `y` of `width` by `height`, all in physical
/// pixels: the top right on macOS, by the menu bar's notices; the bottom right elsewhere,
/// above the taskbar.
pub fn corner(x: i32, y: i32, width: u32, height: u32, scale: f64, top: bool) -> (i32, i32) {
    let (own_width, own_height, margin) = (
        (WIDTH * scale).round() as i32,
        (HEIGHT * scale).round() as i32,
        (MARGIN * scale).round() as i32,
    );
    let left = x + width as i32 - own_width - margin;
    let up = if top {
        y + margin
    } else {
        y + height as i32 - own_height - margin
    };
    (left, up)
}

/// Shows the offer for `found` without taking the focus from what the user is doing.
pub fn show(app: &tauri::AppHandle, found: &MeetingApp) {
    *OFFERED.lock().unwrap_or_else(|e| e.into_inner()) = Some(found.clone());
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let _ = window.set_size(tauri::LogicalSize::new(WIDTH, HEIGHT));
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let area = monitor.work_area();
        let (x, y) = corner(
            area.position.x,
            area.position.y,
            area.size.width,
            area.size.height,
            monitor.scale_factor(),
            cfg!(target_os = "macos"),
        );
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = app.emit_to(LABEL, "meeting:detected", found);
}

fn hide(app: &tauri::AppHandle) {
    OFFERED.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// The meeting the offer window is showing, if any.
#[tauri::command]
pub fn meeting_offer_current() -> Option<MeetingApp> {
    OFFERED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The user said yes: the offer goes, the main window comes forward and begins recording.
#[tauri::command]
pub fn meeting_offer_take(app: tauri::AppHandle) {
    hide(&app);
    crate::restore_main_window(&app);
    let _ = app.emit_to("main", "meeting:record-asked", ());
}

/// The user let the offer go.
#[tauri::command]
pub fn meeting_offer_dismiss(app: tauri::AppHandle) {
    hide(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offer_sits_in_the_corner_of_the_work_area_clear_of_its_edges() {
        let (width, height, margin) = (WIDTH as i32, HEIGHT as i32, MARGIN as i32);
        // A 1920 by 1040 work area above a taskbar, at twice the scale.
        assert_eq!(
            corner(0, 0, 3840, 2080, 2.0, false),
            (3840 - 2 * (width + margin), 2080 - 2 * (height + margin))
        );
        // On macOS, under the menu bar at the top.
        assert_eq!(
            corner(0, 50, 2880, 1750, 2.0, true),
            (2880 - 2 * (width + margin), 50 + 2 * margin)
        );
        // A screen to the left of the main one.
        assert_eq!(corner(-1920, 0, 1920, 1040, 1.0, false).0, -width - margin);
    }
}
