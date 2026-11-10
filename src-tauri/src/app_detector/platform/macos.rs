use std::ffi::c_void;
use std::process::Command;

use url::Url;

use super::ContextSignalSource;
use crate::app_detector::types::{
    BrowserAccessStatus, BrowserTarget, ContextSignals, TargetAppGuard,
};

pub struct MacOsContextSource;

#[derive(Clone, Copy)]
struct SupportedBrowser {
    script_name: &'static str,
    bundle_id: &'static str,
    target: BrowserTarget,
}

pub(crate) fn restore_target_application(target: &TargetAppGuard) -> bool {
    let Some(process_id) = target.process_id else {
        return false;
    };
    let script = format!(
        "tell application \"System Events\" to set frontmost of first application process whose unix id is {process_id} to true"
    );
    Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .status()
        .is_ok_and(|status| status.success())
}

/// Asks the system which process is in front. Takes microseconds, where running a script
/// to collect every signal takes a few hundred milliseconds.
fn frontmost_process_id() -> Option<u32> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: both messages take no arguments and return an object or nil, checked below.
        let pid: i32 = unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
            if app.is_null() {
                return None;
            }
            msg_send![app, processIdentifier]
        };
        u32::try_from(pid).ok().filter(|pid| *pid > 0)
    })
}

/// The category the app in front declares in its own bundle, such as
/// `public.app-category.developer-tools`. Read from the app on this Mac.
fn frontmost_app_category() -> Option<String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: every message returns an object or nil, and each is checked before use.
        // The C string is copied before the pool releases its owner.
        unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let url: *mut AnyObject = msg_send![app, bundleURL];
            if url.is_null() {
                return None;
            }
            let bundle: *mut AnyObject = msg_send![class!(NSBundle), bundleWithURL: url];
            if bundle.is_null() {
                return None;
            }
            let key: *mut AnyObject = msg_send![
                class!(NSString),
                stringWithUTF8String: c"LSApplicationCategoryType".as_ptr()
            ];
            let value: *mut AnyObject = msg_send![bundle, objectForInfoDictionaryKey: key];
            if value.is_null() {
                return None;
            }
            let is_string: bool = msg_send![value, isKindOfClass: class!(NSString)];
            if !is_string {
                return None;
            }
            let text: *const std::os::raw::c_char = msg_send![value, UTF8String];
            if text.is_null() {
                return None;
            }
            let text = std::ffi::CStr::from_ptr(text)
                .to_string_lossy()
                .into_owned();
            Some(text).filter(|text| !text.is_empty())
        }
    })
}

impl ContextSignalSource for MacOsContextSource {
    fn frontmost_process_id(&self) -> Option<u32> {
        frontmost_process_id()
    }

    fn collect(&self) -> Option<ContextSignals> {
        let output = Command::new("/usr/bin/osascript")
            .args(["-e", FRONT_APP_SCRIPT])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let value = String::from_utf8(output.stdout).ok()?;
        let mut fields = value.trim_end().split('\u{1f}');
        let app_name = fields.next()?.trim().to_string();
        let process_id = fields.next().and_then(|value| value.trim().parse().ok());
        let native_identity = fields
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let window_title = fields
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let browser = supported_browser(native_identity.as_deref(), &app_name);
        let (browser_host, browser_access_status) = match browser {
            Some(browser) => {
                let permission_status = browser_access_status_for_automation_permission(
                    automation_permission_status(browser.bundle_id, false),
                );
                if permission_status != BrowserAccessStatus::Available {
                    (None, permission_status)
                } else {
                    match read_browser_host(browser.script_name) {
                        Some(host) => (Some(host), BrowserAccessStatus::Available),
                        None => (None, BrowserAccessStatus::Unknown),
                    }
                }
            }
            None => (None, BrowserAccessStatus::NotApplicable),
        };

        Some(ContextSignals {
            process_id,
            native_identity,
            process_alias: Some(app_name),
            window_title,
            app_category: frontmost_app_category(),
            browser_host,
            is_supported_browser: browser.is_some(),
            browser_access_status,
            browser_target: browser.map(|browser| browser.target),
        })
    }
}

const FRONT_APP_SCRIPT: &str = r#"
tell application "System Events"
    set frontProcess to first application process whose frontmost is true
    set appName to name of frontProcess
    set appPid to unix id of frontProcess
    try
        set bundleId to bundle identifier of frontProcess
    on error
        set bundleId to ""
    end try
    try
        set windowName to name of front window of frontProcess
    on error
        set windowName to ""
    end try
    return appName & ASCII character 31 & (appPid as text) & ASCII character 31 & bundleId & ASCII character 31 & windowName
end tell
"#;

fn supported_browser(identity: Option<&str>, app_name: &str) -> Option<SupportedBrowser> {
    let identity = identity.unwrap_or_default();
    match identity {
        "com.apple.Safari" => Some(SupportedBrowser {
            script_name: "Safari",
            bundle_id: "com.apple.Safari",
            target: BrowserTarget::Safari,
        }),
        "com.google.Chrome" => Some(SupportedBrowser {
            script_name: "Google Chrome",
            bundle_id: "com.google.Chrome",
            target: BrowserTarget::Chrome,
        }),
        "com.microsoft.edgemac" => Some(SupportedBrowser {
            script_name: "Microsoft Edge",
            bundle_id: "com.microsoft.edgemac",
            target: BrowserTarget::Edge,
        }),
        "com.brave.Browser" => Some(SupportedBrowser {
            script_name: "Brave Browser",
            bundle_id: "com.brave.Browser",
            target: BrowserTarget::Brave,
        }),
        "company.thebrowser.Browser" => Some(SupportedBrowser {
            script_name: "Arc",
            bundle_id: "company.thebrowser.Browser",
            target: BrowserTarget::Arc,
        }),
        _ => match app_name {
            "Safari" => supported_browser(Some("com.apple.Safari"), ""),
            "Google Chrome" => supported_browser(Some("com.google.Chrome"), ""),
            "Microsoft Edge" => supported_browser(Some("com.microsoft.edgemac"), ""),
            "Brave Browser" => supported_browser(Some("com.brave.Browser"), ""),
            "Arc" => supported_browser(Some("company.thebrowser.Browser"), ""),
            _ => None,
        },
    }
}

#[repr(C)]
struct AeDesc {
    descriptor_type: u32,
    data_handle: *mut c_void,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AECreateDesc(
        descriptor_type: u32,
        data: *const c_void,
        data_size: isize,
        result: *mut AeDesc,
    ) -> i16;
    fn AEDisposeDesc(descriptor: *mut AeDesc) -> i16;
    fn AEDeterminePermissionToAutomateTarget(
        target: *const AeDesc,
        event_class: u32,
        event_id: u32,
        ask_user_if_needed: u8,
    ) -> i32;
}

fn automation_permission_status(bundle_id: &str, ask_user_if_needed: bool) -> i32 {
    const TYPE_APPLICATION_BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"core");
    const GET_DATA_EVENT_ID: u32 = u32::from_be_bytes(*b"getd");

    let mut target = AeDesc {
        descriptor_type: 0,
        data_handle: std::ptr::null_mut(),
    };
    let create_status = unsafe {
        AECreateDesc(
            TYPE_APPLICATION_BUNDLE_ID,
            bundle_id.as_ptr().cast(),
            bundle_id.len() as isize,
            &mut target,
        )
    };
    if create_status != 0 {
        return i32::from(create_status);
    }

    let permission_status = unsafe {
        AEDeterminePermissionToAutomateTarget(
            &target,
            CORE_EVENT_CLASS,
            GET_DATA_EVENT_ID,
            u8::from(ask_user_if_needed),
        )
    };
    unsafe {
        let _ = AEDisposeDesc(&mut target);
    }
    permission_status
}

pub(crate) fn request_browser_access(target: BrowserTarget) -> BrowserAccessStatus {
    let bundle_id = match target {
        BrowserTarget::Safari => "com.apple.Safari",
        BrowserTarget::Chrome => "com.google.Chrome",
        BrowserTarget::Edge => "com.microsoft.edgemac",
        BrowserTarget::Brave => "com.brave.Browser",
        BrowserTarget::Arc => "company.thebrowser.Browser",
    };
    browser_access_status_for_automation_permission(automation_permission_status(bundle_id, true))
}

fn browser_access_status_for_automation_permission(status: i32) -> BrowserAccessStatus {
    match status {
        0 => BrowserAccessStatus::Available,
        -1743 | -1744 => BrowserAccessStatus::NeedsPermission,
        _ => BrowserAccessStatus::Unknown,
    }
}

fn read_browser_host(browser_name: &str) -> Option<String> {
    let script = if browser_name == "Safari" {
        r#"tell application "Safari" to get URL of current tab of front window"#.to_string()
    } else {
        format!("tell application \"{browser_name}\" to get URL of active tab of front window")
    };
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let transient_url = String::from_utf8(output.stdout).ok()?;
    Url::parse(transient_url.trim())
        .ok()?
        .host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "asks the system which application is in front"]
    fn the_quick_check_agrees_with_the_full_collection() {
        let started = std::time::Instant::now();
        let quick = frontmost_process_id();
        let quick_took = started.elapsed();
        let started = std::time::Instant::now();
        let full = MacOsContextSource
            .collect()
            .and_then(|signals| signals.process_id);
        println!("quick {quick_took:?}, full {:?}", started.elapsed());
        assert!(quick.is_some());
        assert_eq!(quick, full);
    }

    #[test]
    fn automation_permission_status_only_requests_action_for_permission_errors() {
        assert_eq!(
            browser_access_status_for_automation_permission(0),
            BrowserAccessStatus::Available
        );
        assert_eq!(
            browser_access_status_for_automation_permission(-1743),
            BrowserAccessStatus::NeedsPermission
        );
        assert_eq!(
            browser_access_status_for_automation_permission(-1744),
            BrowserAccessStatus::NeedsPermission
        );
        assert_eq!(
            browser_access_status_for_automation_permission(-600),
            BrowserAccessStatus::Unknown
        );
        assert_eq!(
            browser_access_status_for_automation_permission(-1),
            BrowserAccessStatus::Unknown
        );
    }
}

#[cfg(test)]
mod category_tests {
    /// Needs a desktop session with an app in front. Run with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_the_category_of_the_app_in_front() {
        println!("category: {:?}", super::frontmost_app_category());
    }
}
