//! Silences the computer's sound output while the user dictates, and restores it.

use std::path::{Path, PathBuf};

/// Records which output is muted, so a run that ends abruptly can be undone at next launch.
pub fn marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join("output-muted-while-dictating")
}

/// Keeps the default output muted until dropped. It restores only what it changed.
pub struct OutputMute {
    device: u32,
    marker: PathBuf,
}

impl OutputMute {
    /// Mutes the default output. Returns `None` when it was already muted or cannot be muted.
    pub fn engage(marker: &Path) -> Option<Self> {
        let device = platform::default_output()?;
        if platform::is_muted(device)? || !platform::set_muted(device, true) {
            return None;
        }
        if let Err(error) = std::fs::write(marker, device.to_string()) {
            tracing::warn!("Could not record the muted output: {error}");
        }
        Some(Self {
            device,
            marker: marker.to_path_buf(),
        })
    }
}

impl Drop for OutputMute {
    fn drop(&mut self) {
        unmute(self.device);
        let _ = std::fs::remove_file(&self.marker);
    }
}

/// Unmutes an output that an earlier run muted and could not restore.
pub fn recover(marker: &Path) {
    let Ok(text) = std::fs::read_to_string(marker) else {
        return;
    };
    if let Ok(device) = text.trim().parse() {
        unmute(device);
    }
    let _ = std::fs::remove_file(marker);
}

/// Leaves the output alone when someone already unmuted it.
fn unmute(device: u32) {
    if platform::is_muted(device) == Some(true) && !platform::set_muted(device, false) {
        tracing::warn!("Could not unmute the sound output after dictation");
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::c_void;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const fn code(name: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*name)
    }

    const SYSTEM_OBJECT: u32 = 1;
    const DEFAULT_OUTPUT: u32 = code(b"dOut");
    const MUTE: u32 = code(b"mute");
    const SCOPE_GLOBAL: u32 = code(b"glob");
    const SCOPE_OUTPUT: u32 = code(b"outp");
    const ELEMENT_MAIN: u32 = 0;

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
        fn AudioObjectSetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            data: *const c_void,
        ) -> i32;
    }

    fn read(object: u32, address: &Address) -> Option<u32> {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: the buffer is a u32 and `size` states its length.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    const fn mute_address() -> Address {
        Address {
            selector: MUTE,
            scope: SCOPE_OUTPUT,
            element: ELEMENT_MAIN,
        }
    }

    pub fn default_output() -> Option<u32> {
        let address = Address {
            selector: DEFAULT_OUTPUT,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        };
        read(SYSTEM_OBJECT, &address).filter(|device| *device != 0)
    }

    pub fn is_muted(device: u32) -> Option<bool> {
        read(device, &mute_address()).map(|value| value != 0)
    }

    pub fn set_muted(device: u32, muted: bool) -> bool {
        let value = u32::from(muted);
        // SAFETY: the data is a u32 and the size states its length.
        let status = unsafe {
            AudioObjectSetPropertyData(
                device,
                &mute_address(),
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                (&value as *const u32).cast(),
            )
        };
        status == 0
    }
}

/// Windows support comes with the Windows build, where it can be compiled and tested.
#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn default_output() -> Option<u32> {
        None
    }

    pub fn is_muted(_device: u32) -> Option<bool> {
        None
    }

    pub fn set_muted(_device: u32, _muted: bool) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ot-mute-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        marker_path(&dir)
    }

    #[test]
    fn recovery_clears_a_marker_it_cannot_use() {
        let marker = scratch("garbage");
        std::fs::write(&marker, "not a device").unwrap();
        recover(&marker);
        assert!(!marker.exists());
    }

    /// Mutes and restores this computer's real output for a moment.
    #[test]
    #[ignore = "changes the real sound output"]
    fn mutes_the_output_and_restores_it() {
        let device = platform::default_output().expect("an output device");
        if platform::is_muted(device) != Some(false) {
            return;
        }
        let marker = scratch("real");
        let mute = OutputMute::engage(&marker).expect("output muted");
        assert_eq!(platform::is_muted(device), Some(true));
        assert!(marker.exists());
        drop(mute);
        assert_eq!(platform::is_muted(device), Some(false));
        assert!(!marker.exists());
    }

    #[test]
    #[ignore = "changes the real sound output"]
    fn recovery_unmutes_what_a_crashed_run_left_muted() {
        let device = platform::default_output().expect("an output device");
        if platform::is_muted(device) != Some(false) {
            return;
        }
        let marker = scratch("crash");
        std::mem::forget(OutputMute::engage(&marker).expect("output muted"));
        assert_eq!(platform::is_muted(device), Some(true));
        recover(&marker);
        assert_eq!(platform::is_muted(device), Some(false));
        assert!(!marker.exists());
    }
}
