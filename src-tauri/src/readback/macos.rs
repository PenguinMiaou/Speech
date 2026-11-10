//! Reads a text field of another application through the macOS accessibility interface.

use core_foundation::array::CFArray;
use core_foundation::base::{CFRange, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::time::Duration;

use super::WHOLE_FIELD_LIMIT;

const MESSAGING_TIMEOUT_SECONDS: f32 = 0.3;
/// Time a browser-based application gets to build its accessibility tree once asked.
const WEB_TREE_DELAY: Duration = Duration::from_millis(300);
const MAX_DEPTH: usize = 12;
const MAX_ELEMENTS: usize = 600;
const AX_VALUE_CF_RANGE: u32 = 4;
/// Browser-based applications expose their text only after this is switched on.
const WEB_ACCESS: &str = "AXManualAccessibility";
const TEXT_ROLES: [&str; 3] = ["AXTextArea", "AXTextField", "AXComboBox"];

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> u8;
}

struct Element(CFType);

// SAFETY: accessibility elements are plain references that may be used from any thread;
// each `Element` is owned by one task at a time.
unsafe impl Send for Element {}

impl Element {
    fn get(&self, name: &str) -> Option<CFType> {
        let name = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: `value` receives an owned reference on success.
        let status = unsafe {
            AXUIElementCopyAttributeValue(
                self.0.as_CFTypeRef(),
                name.as_concrete_TypeRef(),
                &mut value,
            )
        };
        (status == 0 && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn string(&self, name: &str) -> Option<String> {
        self.get(name)?
            .downcast::<CFString>()
            .map(|s| s.to_string())
    }

    fn boolean(&self, name: &str) -> Option<bool> {
        self.get(name)?.downcast::<CFBoolean>().map(bool::from)
    }

    fn number(&self, name: &str) -> Option<usize> {
        let value = self.get(name)?.downcast::<CFNumber>()?.to_i64()?;
        usize::try_from(value).ok()
    }

    fn element(&self, name: &str) -> Option<Element> {
        let value = self.get(name)?;
        (value.type_of() == unsafe { AXUIElementGetTypeID() }).then_some(Element(value))
    }

    fn children(&self) -> Vec<Element> {
        let Some(array) = self.get("AXChildren").and_then(|v| v.downcast::<CFArray>()) else {
            return Vec::new();
        };
        array
            .iter()
            .map(|item| Element(unsafe { CFType::wrap_under_get_rule(*item) }))
            .collect()
    }

    fn set_boolean(&self, name: &str, value: bool) -> bool {
        let name = CFString::new(name);
        let value = CFBoolean::from(value);
        let status = unsafe {
            AXUIElementSetAttributeValue(
                self.0.as_CFTypeRef(),
                name.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        status == 0
    }

    fn string_for_range(&self, location: usize, length: usize) -> Option<String> {
        let range = CFRange::init(location as isize, length as isize);
        // SAFETY: `range` outlives the call that copies it into the new value.
        let parameter =
            unsafe { AXValueCreate(AX_VALUE_CF_RANGE, (&range as *const CFRange).cast()) };
        if parameter.is_null() {
            return None;
        }
        let parameter = unsafe { CFType::wrap_under_create_rule(parameter) };
        let name = CFString::new("AXStringForRange");
        let mut value: CFTypeRef = std::ptr::null();
        let status = unsafe {
            AXUIElementCopyParameterizedAttributeValue(
                self.0.as_CFTypeRef(),
                name.as_concrete_TypeRef(),
                parameter.as_CFTypeRef(),
                &mut value,
            )
        };
        if status != 0 || value.is_null() {
            return None;
        }
        unsafe { CFType::wrap_under_create_rule(value) }
            .downcast::<CFString>()
            .map(|s| s.to_string())
    }

    fn is_text(&self) -> bool {
        self.string("AXRole")
            .is_some_and(|role| TEXT_ROLES.contains(&role.as_str()))
    }
}

/// The focused text element below `element`, for applications that do not report it themselves.
fn focused_text_below(element: Element, depth: usize, budget: &mut usize) -> Option<Element> {
    if depth > MAX_DEPTH || *budget == 0 {
        return None;
    }
    *budget -= 1;
    if element.is_text() && element.boolean("AXFocused") == Some(true) {
        return Some(element);
    }
    element
        .children()
        .into_iter()
        .find_map(|child| focused_text_below(child, depth + 1, budget))
}

fn focused_text(app: &Element) -> Option<(&'static str, Element)> {
    if let Some(focused) = app.element("AXFocusedUIElement").filter(Element::is_text) {
        return Some(("focus", focused));
    }
    let window = app.element("AXFocusedWindow")?;
    let mut budget = MAX_ELEMENTS;
    focused_text_below(window, 0, &mut budget).map(|element| ("window", element))
}

/// See [`super::before_cursor`]. Applications that expose text only after being asked
/// (browser-based ones) are left alone here: asking takes too long at the start of a recording.
pub fn before_cursor(pid: u32, max_units: usize) -> Option<String> {
    if unsafe { AXIsProcessTrusted() } == 0 {
        return None;
    }
    let app = unsafe { AXUIElementCreateApplication(i32::try_from(pid).ok()?) };
    if app.is_null() {
        return None;
    }
    let app = Element(unsafe { CFType::wrap_under_create_rule(app) });
    unsafe { AXUIElementSetMessagingTimeout(app.0.as_CFTypeRef(), MESSAGING_TIMEOUT_SECONDS) };
    let (_, element) = focused_text(&app)?;
    if element.string("AXSubrole").as_deref() == Some("AXSecureTextField") {
        return None;
    }
    let selection = element.get("AXSelectedTextRange")?;
    let mut range = CFRange::init(0, 0);
    // SAFETY: `range` is a CFRange, the kind asked for, and outlives the call.
    let read = unsafe {
        AXValueGetValue(
            selection.as_CFTypeRef(),
            AX_VALUE_CF_RANGE,
            (&mut range as *mut CFRange).cast(),
        )
    };
    let cursor = usize::try_from(range.location).ok().filter(|_| read != 0)?;
    if cursor == 0 {
        return None;
    }
    let length = cursor.min(max_units);
    element.string_for_range(cursor - length, length)
}

pub struct Field {
    app: Element,
    element: Element,
    route: &'static str,
    web_access_enabled: bool,
    role: String,
    units: usize,
}

/// Finds the focused text field of application `pid` and reads it.
pub fn open(pid: u32, tail_units: usize) -> Result<(Field, String), String> {
    if unsafe { AXIsProcessTrusted() } == 0 {
        return Err("no-accessibility-permission".into());
    }
    let pid = i32::try_from(pid).map_err(|_| "bad-process-id".to_string())?;
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return Err("no-application".into());
    }
    let app = Element(unsafe { CFType::wrap_under_create_rule(app) });
    unsafe { AXUIElementSetMessagingTimeout(app.0.as_CFTypeRef(), MESSAGING_TIMEOUT_SECONDS) };

    let mut web_access_enabled = false;
    let mut found = focused_text(&app);
    if found.is_none()
        && app.boolean(WEB_ACCESS) == Some(false)
        && app.set_boolean(WEB_ACCESS, true)
    {
        web_access_enabled = true;
        std::thread::sleep(WEB_TREE_DELAY);
        found = focused_text(&app).map(|(route, element)| {
            (
                if route == "focus" {
                    "web-focus"
                } else {
                    "web-window"
                },
                element,
            )
        });
    }
    let restore = |app: &Element| {
        if web_access_enabled {
            app.set_boolean(WEB_ACCESS, false);
        }
    };
    let Some((route, element)) = found else {
        let focused_role = app
            .element("AXFocusedUIElement")
            .and_then(|focused| focused.string("AXRole"))
            .unwrap_or_else(|| "none".into());
        restore(&app);
        return Err(format!(
            "no-text-field focused_role={focused_role} web_access_tried={web_access_enabled}"
        ));
    };
    if element.string("AXSubrole").as_deref() == Some("AXSecureTextField") {
        restore(&app);
        return Err("secure-field".into());
    }
    let mut field = Field {
        role: element.string("AXRole").unwrap_or_default(),
        app,
        element,
        route,
        web_access_enabled,
        units: 0,
    };
    match field.read_counting(tail_units) {
        Some((text, units)) => {
            field.units = units;
            Ok((field, text))
        }
        None => {
            let role = field.role.clone();
            field.close();
            Err(format!("text-unreadable role={role} route={route}"))
        }
    }
}

impl Field {
    pub fn route(&self) -> &'static str {
        self.route
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    /// Length of the field in UTF-16 units at the first read.
    pub fn units(&self) -> usize {
        self.units
    }

    pub fn is_focused(&self) -> bool {
        self.app.boolean("AXFrontmost") != Some(false)
            && self.element.boolean("AXFocused") != Some(false)
    }

    /// The whole field, or its last `tail_units` when it is very long.
    pub fn read(&self, tail_units: usize) -> Option<String> {
        self.read_counting(tail_units).map(|(text, _)| text)
    }

    fn read_counting(&self, tail_units: usize) -> Option<(String, usize)> {
        let units = self.element.number("AXNumberOfCharacters");
        if let Some(units) = units.filter(|units| *units > WHOLE_FIELD_LIMIT) {
            let length = tail_units.min(units);
            return self
                .element
                .string_for_range(units - length, length)
                .map(|text| (text, units));
        }
        let text = self
            .element
            .string("AXValue")
            .or_else(|| self.element.string_for_range(0, units?))?;
        let units = units.unwrap_or_else(|| text.encode_utf16().count());
        Some((text, units))
    }

    /// Leaves the application as it was found.
    pub fn close(self) {
        if self.web_access_enabled {
            self.app.set_boolean(WEB_ACCESS, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints only how much text precedes the cursor in the application `READBACK_PID`.
    #[test]
    #[ignore = "reads a text field of a running application"]
    fn reads_the_text_before_the_cursor_of_a_running_application() {
        let pid: u32 = std::env::var("READBACK_PID")
            .expect("READBACK_PID")
            .parse()
            .expect("a process id");
        let started = std::time::Instant::now();
        let before = before_cursor(pid, 400);
        println!(
            "before_cursor_chars={:?} ms={}",
            before.map(|text| text.chars().count()),
            started.elapsed().as_millis()
        );
    }

    /// Reads the focused field of the application whose process id is in `READBACK_PID`.
    /// Prints sizes only.
    #[test]
    #[ignore = "reads a text field of a running application"]
    fn reads_the_focused_field_of_a_running_application() {
        let pid: u32 = std::env::var("READBACK_PID")
            .expect("READBACK_PID")
            .parse()
            .expect("a process id");
        let started = std::time::Instant::now();
        let (field, text) = open(pid, 4_000).expect("a readable field");
        println!(
            "route={} role={} units={} read_chars={} focused={} ms={}",
            field.route(),
            field.role(),
            field.units(),
            text.chars().count(),
            field.is_focused(),
            started.elapsed().as_millis()
        );
        assert!(field.read(4_000).is_some());
        field.close();
    }
}
