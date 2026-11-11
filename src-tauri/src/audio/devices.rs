//! Which microphone a recording uses: the user's pick when it is connected, the
//! system default otherwise, and another microphone when the chosen one hears nothing.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Below this level a microphone hears nothing: it is muted, unplugged or the wrong input.
pub const SILENT_LEVEL: f32 = 0.003;
/// A microphone at this level is hearing a voice.
pub const VOICE_LEVEL: f32 = 0.01;
/// How long the chosen microphone may stay silent before the others are checked.
pub const SILENCE_BEFORE_PROBE: Duration = Duration::from_millis(1_500);
/// How long each other microphone is listened to.
const PROBE_DURATION: Duration = Duration::from_millis(500);

/// Loopback and meeting-software inputs, which never carry the speaker's voice directly.
const VIRTUAL_MARKERS: [&str; 7] = [
    "virtual",
    "blackhole",
    "loopback",
    "soundflower",
    "zoomaudio",
    "teams audio",
    "aggregate",
];

pub fn is_virtual(name: &str) -> bool {
    let name = name.to_lowercase();
    VIRTUAL_MARKERS.iter().any(|marker| name.contains(marker))
}

/// Index of the chosen device, or `None` to use the system default.
pub fn position_of(ids: &[String], chosen: &str) -> Option<usize> {
    let chosen = chosen.trim();
    if chosen.is_empty() {
        return None;
    }
    ids.iter().position(|id| id == chosen)
}

/// The microphone to switch to: the loudest real one that heard a voice while the
/// current one heard nothing for the whole recording.
pub fn louder_device(recording_level: f32, heard: &[(InputDevice, f32)]) -> Option<InputDevice> {
    if recording_level >= SILENT_LEVEL {
        return None;
    }
    heard
        .iter()
        .filter(|(device, level)| !is_virtual(&device.name) && *level >= VOICE_LEVEL)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(device, _)| device.clone())
}

fn name_of(device: &cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_default()
}

pub(super) fn id_of(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.to_string())
}

pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().as_ref().and_then(id_of);
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = id_of(&device)?;
            let name = name_of(&device);
            (!name.trim().is_empty()).then(|| InputDevice {
                is_default: default_id.as_deref() == Some(id.as_str()),
                id,
                name,
            })
        })
        .collect()
}

/// The chosen microphone when it is connected, otherwise the system default.
pub fn input_device(host: &cpal::Host, chosen: &str) -> Option<cpal::Device> {
    if !chosen.trim().is_empty() {
        if let Ok(devices) = host.input_devices() {
            let devices: Vec<cpal::Device> = devices.collect();
            let ids: Vec<String> = devices
                .iter()
                .map(|device| id_of(device).unwrap_or_default())
                .collect();
            if let Some(index) = position_of(&ids, chosen) {
                return devices.into_iter().nth(index);
            }
        }
        tracing::warn!("The chosen microphone is not connected, using the system default");
    }
    host.default_input_device()
}

/// Listens briefly to every other real microphone and returns the loudest level each heard.
pub fn listen_to_others(host: &cpal::Host, current: &cpal::Device) -> Vec<(InputDevice, f32)> {
    let current_id = id_of(current);
    let default_id = host.default_input_device().as_ref().and_then(id_of);
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = id_of(&device)?;
            let name = name_of(&device);
            if Some(&id) == current_id.as_ref() || name.trim().is_empty() || is_virtual(&name) {
                return None;
            }
            let level = listen(&device)?;
            let is_default = default_id.as_deref() == Some(id.as_str());
            Some((
                InputDevice {
                    id,
                    name,
                    is_default,
                },
                level,
            ))
        })
        .collect()
}

fn listen(device: &cpal::Device) -> Option<f32> {
    let config = device.default_input_config().ok()?;
    if config.sample_format() != cpal::SampleFormat::F32 {
        return None;
    }
    let loudest = Arc::new(Mutex::new(0.0_f32));
    let sink = loudest.clone();
    let stream = device
        .build_input_stream(
            config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let level = super::capture::normalized_rms(data);
                let mut loudest = sink.lock().unwrap_or_else(|error| error.into_inner());
                *loudest = loudest.max(level);
            },
            |_| {},
            None,
        )
        .ok()?;
    stream.play().ok()?;
    std::thread::sleep(PROBE_DURATION);
    drop(stream);
    let level = *loudest.lock().unwrap_or_else(|error| error.into_inner());
    Some(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> InputDevice {
        InputDevice {
            id: format!("id-{name}"),
            name: name.to_string(),
            is_default: false,
        }
    }

    #[test]
    #[ignore = "reads this computer's audio devices"]
    fn lists_this_computers_microphones_with_one_default() {
        let devices = list_input_devices();
        assert!(!devices.is_empty());
        assert_eq!(devices.iter().filter(|device| device.is_default).count(), 1);
        let host = cpal::default_host();
        let chosen = input_device(&host, &devices[0].id).expect("chosen device");
        assert_eq!(id_of(&chosen).as_deref(), Some(devices[0].id.as_str()));
        let fallback = input_device(&host, "not-connected").expect("default device");
        let default_id = devices
            .iter()
            .find(|device| device.is_default)
            .map(|d| d.id.clone());
        assert_eq!(id_of(&fallback), default_id);
        println!("{} input devices", devices.len());
    }

    #[test]
    fn recognizes_loopback_and_meeting_inputs() {
        for name in [
            "BlackHole 2ch",
            "ZoomAudioDevice",
            "Microsoft Teams Audio",
            "CABLE Output (VB-Audio Virtual Cable)",
        ] {
            assert!(is_virtual(name), "{name}");
        }
        for name in [
            "MacBook Air Microphone",
            "AirPods Pro",
            "Microphone Array (Realtek(R) Audio)",
        ] {
            assert!(!is_virtual(name), "{name}");
        }
    }

    #[test]
    fn an_empty_choice_means_the_system_default() {
        let ids = vec!["a".to_string(), "b".to_string()];
        assert_eq!(position_of(&ids, ""), None);
        assert_eq!(position_of(&ids, "  "), None);
        assert_eq!(position_of(&ids, "b"), Some(1));
        assert_eq!(position_of(&ids, "gone"), None);
    }

    #[test]
    fn switches_to_the_loudest_microphone_that_heard_a_voice() {
        let heard = [
            (device("webcam"), 0.02),
            (device("headset"), 0.08),
            (device("BlackHole 2ch"), 0.5),
        ];
        assert_eq!(
            louder_device(0.0, &heard).map(|d| d.name),
            Some("headset".to_string())
        );
    }

    #[test]
    fn keeps_a_microphone_that_heard_the_speaker() {
        let heard = [(device("headset"), 0.08)];
        assert_eq!(louder_device(SILENT_LEVEL, &heard), None);
    }

    #[test]
    fn room_noise_on_another_microphone_is_not_a_voice() {
        let heard = [(device("webcam"), VOICE_LEVEL * 0.9)];
        assert_eq!(louder_device(0.0, &heard), None);
    }
}
