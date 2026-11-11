//! Whether the computer's sound plays into headphones. With headphones the microphone
//! hears none of what the computer plays, so nothing of it has to be taken out again.

const fn code(name: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*name)
}

const BUILT_IN: u32 = code(b"bltn");
const BLUETOOTH: u32 = code(b"blue");
const BLUETOOTH_LE: u32 = code(b"blea");
const USB: u32 = code(b"usb ");
/// What a built-in output plays through when headphones are plugged in.
const SOURCE_HEADPHONES: u32 = code(b"hdpn");
const TERMINAL_HEADPHONES: u32 = code(b"hdph");
const TERMINAL_HEADSET_MICROPHONE: u32 = code(b"hmic");
/// Headphones and a headset as a USB device names them.
const USB_HEADPHONES: u32 = 0x0302;
const USB_HEADSET: u32 = 0x0402;

/// What the system says about the device the sound plays through.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Output {
    pub transport: u32,
    pub source: Option<u32>,
    pub terminals: Vec<u32>,
    pub name: String,
}

const LOUDSPEAKERS: [&str; 5] = ["speaker", "soundbar", "homepod", "音箱", "音响"];
const WORN: [&str; 5] = ["headphone", "headset", "airpod", "earbud", "耳机"];

/// Whether `output` is worn on the ears. An output that cannot be told is taken for
/// loudspeakers, which costs a little work and loses nothing.
pub fn is_headphones(output: &Output) -> bool {
    let named = |words: &[&str]| {
        let name = output.name.to_lowercase();
        words.iter().any(|word| name.contains(word))
    };
    let said = output.terminals.iter().any(|terminal| {
        [
            TERMINAL_HEADPHONES,
            TERMINAL_HEADSET_MICROPHONE,
            USB_HEADPHONES,
            USB_HEADSET,
        ]
        .contains(terminal)
    });
    if said || output.source == Some(SOURCE_HEADPHONES) {
        return true;
    }
    match output.transport {
        BLUETOOTH | BLUETOOTH_LE => !named(&LOUDSPEAKERS),
        USB | BUILT_IN => named(&WORN),
        _ => false,
    }
}

/// Whether the computer's sound plays into headphones right now.
pub fn headphones() -> bool {
    platform::output().is_some_and(|output| is_headphones(&output))
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{code, Output};
    use cpal::traits::{DeviceTrait, HostTrait};
    use std::ffi::c_void;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const SYSTEM_OBJECT: u32 = 1;
    const SCOPE_GLOBAL: u32 = code(b"glob");
    const SCOPE_OUTPUT: u32 = code(b"outp");

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
    }

    /// Up to `most` numbers of a property of `object`.
    fn read(object: u32, selector: &[u8; 4], scope: u32, most: usize) -> Option<Vec<u32>> {
        let address = Address {
            selector: code(selector),
            scope,
            element: 0,
        };
        let mut values = vec![0_u32; most];
        let mut size = (most * std::mem::size_of::<u32>()) as u32;
        // SAFETY: the buffer holds `most` numbers and `size` states its length in bytes;
        // the system writes no more than that and sets `size` to what it wrote.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                values.as_mut_ptr().cast(),
            )
        };
        if status != 0 {
            return None;
        }
        values.truncate(size as usize / std::mem::size_of::<u32>());
        Some(values)
    }

    pub fn output() -> Option<Output> {
        let device = *read(SYSTEM_OBJECT, b"dOut", SCOPE_GLOBAL, 1)?.first()?;
        let transport = read(device, b"tran", SCOPE_GLOBAL, 1)
            .and_then(|values| values.first().copied())
            .unwrap_or(0);
        let source =
            read(device, b"ssrc", SCOPE_OUTPUT, 1).and_then(|values| values.first().copied());
        let terminals = read(device, b"stm#", SCOPE_OUTPUT, 16)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|stream| read(stream, b"term", SCOPE_GLOBAL, 1)?.first().copied())
            .collect();
        let name = cpal::default_host()
            .default_output_device()
            .and_then(|device| device.description().ok())
            .map(|description| description.name().to_string())
            .unwrap_or_default();
        Some(Output {
            transport,
            source,
            terminals,
            name,
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn output() -> Option<super::Output> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(transport: u32, name: &str) -> Output {
        Output {
            transport,
            name: name.to_string(),
            ..Output::default()
        }
    }

    #[test]
    fn the_built_in_loudspeakers_are_not_headphones() {
        let mut speakers = output(BUILT_IN, "Built-in Output");
        speakers.source = Some(code(b"ispk"));
        speakers.terminals = vec![code(b"spkr")];
        assert!(!is_headphones(&speakers));
    }

    #[test]
    fn headphones_in_the_jack_are_headphones() {
        let mut plugged = output(BUILT_IN, "Built-in Output");
        plugged.source = Some(SOURCE_HEADPHONES);
        assert!(is_headphones(&plugged));
        let mut said = output(BUILT_IN, "Output");
        said.terminals = vec![TERMINAL_HEADPHONES];
        assert!(is_headphones(&said));
        assert!(is_headphones(&output(BUILT_IN, "External Headphones")));
    }

    #[test]
    fn a_wireless_output_is_worn_unless_its_name_says_loudspeaker() {
        assert!(is_headphones(&output(BLUETOOTH, "Buds")));
        assert!(is_headphones(&output(BLUETOOTH_LE, "Buds")));
        assert!(!is_headphones(&output(BLUETOOTH, "Kitchen Speaker")));
        assert!(!is_headphones(&output(BLUETOOTH, "客厅音箱")));
    }

    #[test]
    fn a_usb_output_is_worn_only_when_it_says_so() {
        assert!(!is_headphones(&output(USB, "Audio Interface")));
        assert!(is_headphones(&output(USB, "Gaming Headset")));
        let mut said = output(USB, "Audio Device");
        said.terminals = vec![USB_HEADSET];
        assert!(is_headphones(&said));
    }

    #[test]
    fn an_output_nothing_is_known_of_is_taken_for_loudspeakers() {
        assert!(!is_headphones(&Output::default()));
        assert!(!is_headphones(&output(code(b"hdmi"), "Display")));
    }

    /// Prints what the system says of the output in use now, without its name.
    #[test]
    #[ignore = "reads this machine's sound output"]
    fn reads_the_output_in_use() {
        let output = platform::output().expect("no output");
        let text = |value: u32| String::from_utf8_lossy(&value.to_be_bytes()).to_string();
        println!(
            "transport {:?} source {:?} terminals {:?} headphones {}",
            text(output.transport),
            output.source.map(text),
            output
                .terminals
                .iter()
                .map(|t| text(*t))
                .collect::<Vec<_>>(),
            is_headphones(&output)
        );
    }
}
