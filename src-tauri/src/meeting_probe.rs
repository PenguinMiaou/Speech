//! Tells when a meeting or a call is going on, so that the app can offer to write it down.
//! Any program that takes sound from the microphone and plays sound at the same time, for a
//! little while, is taken to be in a call, whatever the program is; a known meeting app or a
//! browser counts as soon as it takes sound from the microphone. Which programs do that
//! comes from the system: on macOS the audio server's list of client processes (macOS 14.2
//! and later), on Windows the sound sessions of each microphone and speaker. The app's own
//! recording never counts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
#[path = "meeting_probe_windows.rs"]
mod windows;

/// A meeting app, or a browser that may hold a meeting, found taking sound from the
/// microphone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MeetingApp {
    /// The name people know it by.
    pub name: String,
    /// How the system names the program: a bundle id on macOS, a program file name or a
    /// package name on Windows.
    pub id: String,
    /// A browser holds the microphone for many things besides meetings, so it is less sure.
    pub browser: bool,
    /// Found by taking and playing sound at once rather than by its name: a call in any
    /// program.
    pub call: bool,
    /// How many looks in a row it must be seen before it is told of.
    #[serde(skip)]
    pub looks: u8,
}

/// A process taking sound from the microphone or playing sound, as the system names it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Holder {
    /// A bundle id on macOS, a program file name on Windows.
    pub id: String,
    pub pid: Option<u32>,
    /// The app it belongs to: the outermost .app on macOS, the program file's path on
    /// Windows. The processes of one app share it.
    pub app: String,
    /// Taking sound from the microphone.
    pub input: bool,
    /// Playing sound; None where the system cannot tell.
    pub output: Option<bool>,
}

/// Looks in a row before a call in an unknown program is told of: about ten seconds.
const CALL_LOOKS: u8 = 2;
/// Looks in a row before an unknown program that only takes sound is told of, where the
/// system cannot tell whether it plays sound: about thirty seconds.
const MIC_ONLY_LOOKS: u8 = 6;

/// The meeting apps and browsers, by the start of the names the systems give them: the
/// bundle id on macOS, the program file or package name on Windows. A helper process
/// carries the name of its app with more after it (com.google.Chrome.helper).
const KNOWN: &[(&str, &str, bool)] = &[
    // macOS
    ("com.tencent.meeting", "腾讯会议", false),
    ("us.zoom.xos", "Zoom", false),
    ("com.microsoft.teams2", "Microsoft Teams", false),
    ("com.microsoft.teams", "Microsoft Teams", false),
    ("com.bytedance.macos.feishu", "飞书", false),
    ("com.electron.lark", "Lark", false),
    ("com.alibaba.DingTalkMac", "钉钉", false),
    ("Cisco-Systems.Spark", "Webex", false),
    ("com.cisco.webexmeetingsapp", "Webex", false),
    ("com.tinyspeck.slackmacgap", "Slack", false),
    ("com.google.Chrome", "Google Chrome", true),
    ("com.apple.Safari", "Safari", true),
    ("com.apple.WebKit", "Safari", true),
    ("com.microsoft.edgemac", "Microsoft Edge", true),
    ("org.mozilla.firefox", "Firefox", true),
    ("company.thebrowser.Browser", "Arc", true),
    // Windows
    ("wemeetapp.exe", "腾讯会议", false),
    ("zoom.exe", "Zoom", false),
    ("ms-teams.exe", "Microsoft Teams", false),
    ("teams.exe", "Microsoft Teams", false),
    ("msteams_", "Microsoft Teams", false),
    ("feishu.exe", "飞书", false),
    ("lark.exe", "Lark", false),
    ("dingtalk.exe", "钉钉", false),
    ("ciscocollabhost.exe", "Webex", false),
    ("webex.exe", "Webex", false),
    ("atmgr.exe", "Webex", false),
    ("slack.exe", "Slack", false),
    ("chrome.exe", "Google Chrome", true),
    ("msedge.exe", "Microsoft Edge", true),
    ("firefox.exe", "Firefox", true),
];

/// The app's own programs, which never count as a meeting.
const OWN: &[&str] = &["com.opentypeless.app", "opentypeless", "ot-stt-worker"];

/// The meeting app or browser that `id` names, if it is one.
pub fn known(id: &str) -> Option<MeetingApp> {
    let lower = id.to_lowercase();
    KNOWN
        .iter()
        .find(|(start, _, _)| {
            let start = start.to_lowercase();
            lower == start || lower.starts_with(&format!("{start}.")) || {
                // A Windows package name or a program file goes by its start alone.
                (start.ends_with('_') || start.ends_with(".exe")) && lower.starts_with(&start)
            }
        })
        .map(|(_, name, browser)| MeetingApp {
            name: (*name).to_string(),
            id: id.to_string(),
            browser: *browser,
            call: false,
            looks: 1,
        })
}

/// The name of the app at `app`, a path or a name: its last part without .app or .exe.
fn app_name(app: &str) -> String {
    let last = app
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(app);
    let lower = last.to_lowercase();
    if lower.ends_with(".app") || lower.ends_with(".exe") {
        last[..last.len() - 4].to_string()
    } else {
        last.to_string()
    }
}

/// Whether `holder` is one of the app's own programs.
fn own(holder: &Holder, own_pid: u32) -> bool {
    let lower = holder.id.to_lowercase();
    holder.pid == Some(own_pid)
        || OWN
            .iter()
            .any(|name| lower == *name || lower.starts_with(&format!("{name}.")))
}

/// The meeting among the processes taking or playing sound, taken app by app: a known
/// meeting app first, then a call in any app, then a browser; never the app itself. An app
/// counts only while it takes sound from the microphone.
pub fn meeting_of(holders: &[Holder], own_pid: u32) -> Option<MeetingApp> {
    let mut apps: Vec<(&str, Vec<&Holder>)> = Vec::new();
    for holder in holders.iter().filter(|holder| !own(holder, own_pid)) {
        let app = if holder.app.is_empty() {
            holder.id.as_str()
        } else {
            holder.app.as_str()
        };
        match apps.iter_mut().find(|(each, _)| *each == app) {
            Some((_, members)) => members.push(holder),
            None => apps.push((app, vec![holder])),
        }
    }
    let found: Vec<MeetingApp> = apps
        .into_iter()
        .filter(|(_, members)| members.iter().any(|holder| holder.input))
        .filter_map(|(app, members)| {
            if let Some(known) = members.iter().find_map(|holder| known(&holder.id)) {
                return Some(known);
            }
            let plays = if members.iter().any(|holder| holder.output == Some(true)) {
                Some(true)
            } else if members.iter().all(|holder| holder.output.is_none()) {
                None
            } else {
                Some(false)
            };
            let looks = match plays {
                Some(true) => CALL_LOOKS,
                None => MIC_ONLY_LOOKS,
                Some(false) => return None,
            };
            let name = app_name(app);
            (!name.is_empty()).then(|| MeetingApp {
                name,
                // The app, which stays the same from look to look in an app of many processes.
                id: app.to_string(),
                browser: false,
                call: true,
                looks,
            })
        })
        .collect();
    let rank = |app: &MeetingApp| match (app.call, app.browser) {
        (false, false) => 0,
        (true, _) => 1,
        (false, true) => 2,
    };
    found.into_iter().min_by_key(rank)
}

/// The programs taking sound from the microphone now. Empty where the system cannot tell.
pub fn mic_holders() -> Vec<Holder> {
    #[cfg(target_os = "macos")]
    {
        macos::holders()
    }
    #[cfg(target_os = "windows")]
    {
        windows::holders()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// The processes taking or playing sound, by app name, with "in" for taking sound and
/// "out" for playing it, for the log. Names only, never what is said.
fn heard(holders: &[Holder]) -> String {
    let mut seen: Vec<String> = holders
        .iter()
        .map(|holder| {
            let app = if holder.app.is_empty() {
                &holder.id
            } else {
                &holder.app
            };
            let way = match (holder.input, holder.output) {
                (true, Some(true)) => "in+out",
                (true, Some(false)) => "in",
                (true, None) => "in?",
                (false, _) => "out",
            };
            format!("{} {way}", app_name(app))
        })
        .collect();
    seen.sort();
    seen.dedup();
    if seen.is_empty() {
        "nothing".to_string()
    } else {
        seen.join(", ")
    }
}

/// The meeting going on now, if a meeting app or a browser is taking sound from the
/// microphone.
pub fn meeting_in_progress() -> Option<MeetingApp> {
    meeting_of(&mic_holders(), std::process::id())
}

/// How long the microphone must be free of a meeting before the next meeting is told of.
pub const QUIET_BEFORE_NEXT: Duration = Duration::from_secs(30);
/// How often the watcher looks.
pub const EVERY: Duration = Duration::from_secs(5);

/// Follows what is seen from look to look, and tells of each meeting once: when it has been
/// seen as many looks in a row as it asks, and not again until the microphone has been free
/// of meetings for `QUIET_BEFORE_NEXT`. While the app is recording, a meeting is taken as
/// told of.
#[derive(Debug, Default)]
pub struct Watch {
    last_seen: Option<Instant>,
    told: bool,
    /// The meeting seen in the looks just before, and in how many of them.
    streak: Option<(String, u8)>,
}

impl Watch {
    pub fn step(
        &mut self,
        now: Instant,
        seen: Option<MeetingApp>,
        busy: bool,
    ) -> Option<MeetingApp> {
        if self
            .last_seen
            .is_none_or(|last| now.duration_since(last) >= QUIET_BEFORE_NEXT)
        {
            self.told = false;
        }
        let Some(seen) = seen else {
            self.streak = None;
            return None;
        };
        self.last_seen = Some(now);
        let looks = match &self.streak {
            Some((id, looks)) if *id == seen.id => looks.saturating_add(1),
            _ => 1,
        };
        self.streak = Some((seen.id.clone(), looks));
        if busy {
            self.told = true;
        }
        if self.told || looks < seen.looks {
            return None;
        }
        self.told = true;
        Some(seen)
    }
}

static ENABLED: AtomicBool = AtomicBool::new(true);
static BUSY: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

/// Turns the watching on or off, as the setting says.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether the app is recording a meeting itself, so that the meeting it records is not
/// offered again, nor right after the recording ends while the meeting goes on.
pub fn set_busy(busy: bool) {
    BUSY.store(busy, Ordering::Relaxed);
}

/// Looks for a meeting every few seconds for as long as the app runs, and emits
/// `meeting:detected` with the app's name once for each meeting. Started once.
pub fn start_watch(app: tauri::AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let mut watch = Watch::default();
        let mut last_heard = String::new();
        loop {
            std::thread::sleep(EVERY);
            if !ENABLED.load(Ordering::Relaxed) {
                watch = Watch::default();
                continue;
            }
            let holders = mic_holders();
            let now_heard = heard(&holders);
            if now_heard != last_heard {
                // What takes and plays sound, to tell why a call was or was not found.
                crate::applog::note("meeting", &format!("sound in use: {now_heard}"));
                last_heard = now_heard;
            }
            let seen = meeting_of(&holders, std::process::id());
            if let Some(found) = watch.step(Instant::now(), seen, BUSY.load(Ordering::Relaxed)) {
                crate::applog::note(
                    "meeting",
                    &format!("a meeting seems to be on in {}", found.name),
                );
                crate::meeting_offer::show(&app, &found);
            }
        }
    });
}

#[cfg(target_os = "macos")]
mod macos {
    use super::Holder;
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
    const PROCESS_LIST: u32 = code(b"prs#");
    const PID: u32 = code(b"ppid");
    const BUNDLE_ID: u32 = code(b"pbid");
    const RUNNING_INPUT: u32 = code(b"piri");
    const RUNNING_OUTPUT: u32 = code(b"piro");
    const SCOPE_GLOBAL: u32 = code(b"glob");
    const ELEMENT_MAIN: u32 = 0;

    extern "C" {
        /// From libproc: the path of the program `pid` runs.
        fn proc_pidpath(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    }

    /// The app process `pid` belongs to: the path up to its outermost .app, or the path of
    /// its program when it is in no app.
    fn app_of(pid: u32) -> Option<String> {
        let mut buffer = vec![0_u8; 4096];
        // SAFETY: `buffer` holds `size` bytes; the call writes at most that many.
        let length = unsafe {
            proc_pidpath(
                i32::try_from(pid).ok()?,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
            )
        };
        if length <= 0 {
            return None;
        }
        buffer.truncate(length as usize);
        let path = String::from_utf8(buffer).ok()?;
        Some(match path.find(".app/") {
            Some(at) => path[..at + 4].to_string(),
            None => path,
        })
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    const fn address(selector: u32) -> Address {
        Address {
            selector,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        }
    }

    fn read_u32(object: u32, selector: u32) -> Option<u32> {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: the buffer is a u32 (a pid_t is the same size) and `size` states its length.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address(selector),
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    fn bundle_id(object: u32) -> Option<String> {
        use core_foundation::base::TCFType;
        use core_foundation::string::{CFString, CFStringRef};
        let mut value: CFStringRef = std::ptr::null();
        let mut size = std::mem::size_of::<CFStringRef>() as u32;
        // SAFETY: the buffer holds one CFStringRef and `size` states its length.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address(BUNDLE_ID),
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut CFStringRef).cast(),
            )
        };
        if status != 0 || value.is_null() {
            return None;
        }
        // SAFETY: the property hands over a string the caller must release.
        let text = unsafe { CFString::wrap_under_create_rule(value) }.to_string();
        (!text.is_empty()).then_some(text)
    }

    fn processes() -> Vec<u32> {
        let list = address(PROCESS_LIST);
        let mut size = 0_u32;
        // SAFETY: only the size is asked for.
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &list, 0, std::ptr::null(), &mut size)
        };
        if status != 0 || size == 0 {
            return Vec::new();
        }
        let mut objects = vec![0_u32; size as usize / std::mem::size_of::<u32>()];
        // SAFETY: the buffer holds `size` bytes of object ids.
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &list,
                0,
                std::ptr::null(),
                &mut size,
                objects.as_mut_ptr().cast(),
            )
        };
        if status != 0 {
            return Vec::new();
        }
        objects.truncate(size as usize / std::mem::size_of::<u32>());
        objects
    }

    /// The client processes of the audio server that have sound coming in or going out.
    /// Before macOS 14.2 the list does not exist and nothing is found.
    pub fn holders() -> Vec<Holder> {
        processes()
            .into_iter()
            .filter_map(|object| {
                let input = read_u32(object, RUNNING_INPUT) == Some(1);
                let output = read_u32(object, RUNNING_OUTPUT) == Some(1);
                if !input && !output {
                    return None;
                }
                let pid = read_u32(object, PID);
                let id = bundle_id(object).unwrap_or_default();
                let app = pid.and_then(app_of).unwrap_or_else(|| id.clone());
                Some(Holder {
                    id,
                    pid,
                    app,
                    input,
                    output: Some(output),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder(id: &str, pid: u32) -> Holder {
        Holder {
            id: id.into(),
            pid: Some(pid),
            input: true,
            output: Some(false),
            ..Holder::default()
        }
    }

    fn process(app: &str, pid: u32, input: bool, output: Option<bool>) -> Holder {
        Holder {
            id: String::new(),
            pid: Some(pid),
            app: app.into(),
            input,
            output,
        }
    }

    #[test]
    fn a_call_in_any_app_is_found_by_taking_and_playing_sound_at_once() {
        let own_pid = 77;
        let app = "/Applications/Office Chat.app";
        // Sound taken in one process and played in another of the same app.
        let call = [
            process(app, 5, true, Some(false)),
            process(app, 6, false, Some(true)),
        ];
        let found = meeting_of(&call, own_pid).unwrap();
        assert_eq!((found.name.as_str(), found.call), ("Office Chat", true));
        assert_eq!(found.looks, CALL_LOOKS);
        // Recording without playing, or playing without recording, is no call.
        assert_eq!(
            meeting_of(&[process(app, 5, true, Some(false))], own_pid),
            None
        );
        assert_eq!(
            meeting_of(&[process(app, 6, false, Some(true))], own_pid),
            None
        );
        // Two apps, one recording and one playing, are no call either.
        let apart = [
            process("/Applications/Recorder.app", 5, true, Some(false)),
            process("/Applications/Music.app", 6, false, Some(true)),
        ];
        assert_eq!(meeting_of(&apart, own_pid), None);
        // Where the system cannot tell what plays, a program long on the microphone counts.
        let unsure = meeting_of(&[process(r"C:\Apps\Chat.exe", 5, true, None)], own_pid).unwrap();
        assert_eq!(
            (unsure.name.as_str(), unsure.looks),
            ("Chat", MIC_ONLY_LOOKS)
        );
        // The app's own recording never counts.
        assert_eq!(
            meeting_of(&[process(app, own_pid, true, Some(true))], own_pid),
            None
        );
        // A known meeting app comes first, a call before a browser.
        let mut many = call.to_vec();
        many.push(holder("com.google.Chrome.helper", 8));
        assert!(meeting_of(&many, own_pid).unwrap().call);
        many.push(holder("us.zoom.xos", 9));
        assert_eq!(meeting_of(&many, own_pid).unwrap().name, "Zoom");
    }

    #[test]
    fn a_call_is_told_of_only_once_seen_for_a_few_looks_in_a_row() {
        let call = Some(MeetingApp {
            name: "Chat".into(),
            id: "chat".into(),
            browser: false,
            call: true,
            looks: 2,
        });
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut watch = Watch::default();
        assert_eq!(watch.step(at(0), call.clone(), false), None);
        // A blip is not a call.
        assert_eq!(watch.step(at(5), None, false), None);
        assert_eq!(watch.step(at(10), call.clone(), false), None);
        assert!(watch.step(at(15), call.clone(), false).is_some());
        assert_eq!(watch.step(at(20), call.clone(), false), None);
    }

    #[test]
    fn meeting_apps_and_their_helpers_are_known_by_the_start_of_their_names() {
        assert_eq!(known("us.zoom.xos").unwrap().name, "Zoom");
        assert_eq!(
            known("com.microsoft.teams2").unwrap().name,
            "Microsoft Teams"
        );
        assert_eq!(
            known("com.microsoft.teams2.helper").unwrap().name,
            "Microsoft Teams"
        );
        assert!(known("com.google.Chrome.helper").unwrap().browser);
        assert_eq!(known("Zoom.exe").unwrap().name, "Zoom");
        assert_eq!(
            known("MSTeams_8wekyb3d8bbwe").unwrap().name,
            "Microsoft Teams"
        );
        assert_eq!(known("C:#Program Files#Zoom#bin#Zoom.exe"), None);
        assert_eq!(known("com.apple.Music"), None);
        assert_eq!(known(""), None);
        // A name that merely begins the same way is another app.
        assert_eq!(known("us.zoom.xosother"), None);
    }

    #[test]
    fn a_meeting_app_comes_before_a_browser_and_the_app_itself_never_counts() {
        let own_pid = 77;
        let holders = [
            holder("com.opentypeless.app", 5),
            holder("com.google.Chrome.helper", 6),
            holder("com.tencent.meeting", 7),
        ];
        assert_eq!(meeting_of(&holders, own_pid).unwrap().name, "腾讯会议");
        let browser_only = [holder("com.google.Chrome.helper", 6)];
        assert!(meeting_of(&browser_only, own_pid).unwrap().browser);
        // The app's own process, whatever the system calls it, and its worker.
        let ours = [holder("us.zoom.xos", own_pid), holder("ot-stt-worker", 9)];
        assert_eq!(meeting_of(&ours, own_pid), None);
        assert_eq!(meeting_of(&[holder("com.apple.Music", 3)], own_pid), None);
    }

    #[test]
    fn each_meeting_is_told_of_once_until_the_microphone_is_free_for_a_while() {
        let zoom = known("us.zoom.xos");
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut watch = Watch::default();
        assert_eq!(watch.step(at(0), None, false), None);
        assert!(watch.step(at(5), zoom.clone(), false).is_some());
        assert_eq!(watch.step(at(10), zoom.clone(), false), None);
        // A short gap, as when the microphone is muted for a moment, is the same meeting.
        assert_eq!(watch.step(at(15), None, false), None);
        assert_eq!(watch.step(at(30), zoom.clone(), false), None);
        // Free for longer: the next meeting is told of.
        assert_eq!(watch.step(at(45), None, false), None);
        assert!(watch.step(at(61), zoom.clone(), false).is_some());
    }

    #[test]
    fn a_meeting_the_app_is_recording_is_not_offered_then_or_right_after() {
        let teams = known("ms-teams.exe");
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut watch = Watch::default();
        assert_eq!(watch.step(at(0), teams.clone(), true), None);
        assert_eq!(watch.step(at(5), teams.clone(), true), None);
        // The recording ends while the meeting goes on.
        assert_eq!(watch.step(at(10), teams.clone(), false), None);
    }

    #[test]
    #[ignore = "reads who holds this machine's microphone"]
    fn prints_who_holds_the_microphone() {
        println!("holders: {:?}", mic_holders());
        println!("meeting: {:?}", meeting_in_progress());
    }
}
