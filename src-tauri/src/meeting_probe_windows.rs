//! Which programs take sound from the microphone and which play sound on Windows.
//!
//! The audio sessions Windows keeps for each active device tell this: a session that is
//! active on a capture device is a program taking sound, one active on a render device a
//! program playing it. Where the sessions cannot be read, the record Windows keeps of
//! microphone use for its privacy settings is used instead: each program that has used
//! the microphone has a key with the times it last began and stopped, and a stop time of
//! zero while it still has the microphone. Programs installed as packages have keys of
//! their package names; other programs, keys of their paths under NonPackaged, with # for
//! each \. That record cannot tell whether a program plays sound.

use std::collections::BTreeSet;

use super::Holder;
use windows::core::Interface;
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_OK};
use windows::Win32::Media::Audio::{
    eCapture, eRender, AudioSessionStateActive, EDataFlow, IAudioSessionControl2,
    IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    REG_QWORD,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

const MICROPHONE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// An open key, closed when dropped.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed once.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

fn open(parent: HKEY, path: &str) -> Option<Key> {
    let path = wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` ends in a zero and `key` receives the opened key.
    let status = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ, &mut key) };
    (status == ERROR_SUCCESS).then_some(Key(key))
}

fn subkeys(key: &Key) -> Vec<String> {
    let mut names = Vec::new();
    for index in 0.. {
        let mut name = [0_u16; 512];
        let mut length = name.len() as u32;
        // SAFETY: `name` holds `length` characters; the other outputs are not asked for.
        let status = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                name.as_mut_ptr(),
                &mut length,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&name[..length as usize]));
    }
    names
}

fn qword(key: &Key, value: &str) -> Option<u64> {
    let value = wide(value);
    let mut data = 0_u64;
    let mut size = std::mem::size_of::<u64>() as u32;
    let mut kind = 0_u32;
    // SAFETY: `data` holds `size` bytes and `value` ends in a zero.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            value.as_ptr(),
            std::ptr::null(),
            &mut kind,
            (&mut data as *mut u64).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS && kind == REG_QWORD).then_some(data)
}

/// Whether the program of `key` has the microphone now: it began and has not stopped.
fn in_use(key: &Key) -> bool {
    qword(key, "LastUsedTimeStop") == Some(0)
        && qword(key, "LastUsedTimeStart").is_some_and(|start| start != 0)
}

/// The program file a NonPackaged key names, from its path with # for each \.
fn program_of(key_name: &str) -> String {
    key_name
        .rsplit('#')
        .next()
        .unwrap_or(key_name)
        .to_lowercase()
}

/// The processes with an active audio session on the active devices of `flow`, leaving out
/// the system sounds. A device whose sessions cannot be read is passed over.
fn session_pids(
    devices: &IMMDeviceEnumerator,
    flow: EDataFlow,
) -> windows::core::Result<BTreeSet<u32>> {
    let mut pids = BTreeSet::new();
    // SAFETY: COM is initialised on this thread and every interface is used on it only.
    unsafe {
        let collection = devices.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
        for index in 0..collection.GetCount()? {
            let Ok(device) = collection.Item(index) else {
                continue;
            };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let Ok(sessions) = manager.GetSessionEnumerator() else {
                continue;
            };
            for index in 0..sessions.GetCount().unwrap_or(0) {
                let Some(session) = sessions
                    .GetSession(index)
                    .and_then(|session| session.cast::<IAudioSessionControl2>())
                    .ok()
                else {
                    continue;
                };
                if session.GetState() != Ok(AudioSessionStateActive)
                    || session.IsSystemSoundsSession() == S_OK
                {
                    continue;
                }
                if let Ok(pid) = session.GetProcessId() {
                    if pid != 0 {
                        pids.insert(pid);
                    }
                }
            }
        }
    }
    Ok(pids)
}

/// The processes taking sound and the processes playing it, from the audio sessions; None
/// where they cannot be read.
fn sessions() -> Option<(BTreeSet<u32>, BTreeSet<u32>)> {
    // SAFETY: CoInitializeEx takes no pointers here. A thread already set up for COM in
    // another mode answers RPC_E_CHANGED_MODE and can still use COM; only a call that
    // succeeds is balanced by CoUninitialize, after every interface below is released.
    let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if status.is_err() && status != RPC_E_CHANGED_MODE {
        return None;
    }
    // The error is dropped inside, as it may hold a COM object of its own.
    let found = (|| -> windows::core::Result<_> {
        // SAFETY: COM is initialised on this thread.
        let devices: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        Ok((
            session_pids(&devices, eCapture)?,
            session_pids(&devices, eRender)?,
        ))
    })()
    .ok();
    if status.is_ok() {
        // SAFETY: balances the successful CoInitializeEx above; no interface is alive.
        unsafe { CoUninitialize() };
    }
    found
}

/// The full path of the program file of process `pid`.
fn program_path(pid: u32) -> Option<String> {
    // SAFETY: OpenProcess takes no pointers; a null handle means it failed.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = [0_u16; 1024];
    let mut length = path.len() as u32;
    // SAFETY: `path` holds `length` characters and `process` is open until closed below.
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length)
    };
    // SAFETY: `process` was opened above and is closed once.
    unsafe { CloseHandle(process) };
    (ok != 0).then(|| String::from_utf16_lossy(&path[..length as usize]))
}

/// The processes taking or playing sound, from the audio sessions; from the privacy
/// record of microphone use where the sessions cannot be read. A process whose program
/// file cannot be found is left out, as nothing could be told of it.
pub fn holders() -> Vec<Holder> {
    let Some((capture, render)) = sessions() else {
        return consent_holders();
    };
    capture
        .union(&render)
        .filter_map(|&pid| {
            let app = program_path(pid)?;
            let id = app.rsplit('\\').next().unwrap_or(&app).to_string();
            Some(Holder {
                id,
                pid: Some(pid),
                app,
                input: capture.contains(&pid),
                output: Some(render.contains(&pid)),
            })
        })
        .collect()
}

/// The programs that have the microphone now, from the privacy record. Whether they play
/// sound is not known.
fn consent_holders() -> Vec<Holder> {
    let Some(microphone) = open(HKEY_CURRENT_USER, MICROPHONE) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for name in subkeys(&microphone) {
        if name.eq_ignore_ascii_case("NonPackaged") {
            continue;
        }
        if open(microphone.0, &name).is_some_and(|key| in_use(&key)) {
            names.push(name);
        }
    }
    if let Some(programs) = open(microphone.0, "NonPackaged") {
        for name in subkeys(&programs) {
            if open(programs.0, &name).is_some_and(|key| in_use(&key)) {
                names.push(program_of(&name));
            }
        }
    }
    names
        .into_iter()
        .map(|id| Holder {
            app: id.clone(),
            id,
            pid: None,
            input: true,
            output: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_program_is_named_by_its_file() {
        assert_eq!(
            super::program_of("C:#Program Files#Zoom#bin#Zoom.exe"),
            "zoom.exe"
        );
        assert_eq!(super::program_of("Zoom.exe"), "zoom.exe");
    }
}
