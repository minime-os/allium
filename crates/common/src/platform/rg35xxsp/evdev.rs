use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, anyhow};
use evdev::{Device, EventStream, EventType, KeyCode};

use crate::constants::MAXIMUM_FRAME_TIME;
use crate::platform::{Key, KeyEvent};

pub struct Rg35xxSpKeys {
    events: EventStream,
}

impl Rg35xxSpKeys {
    pub fn new() -> Result<Self> {
        let device = open_input_device()?;
        Ok(Self {
            events: device.into_event_stream()?,
        })
    }

    pub async fn poll(&mut self) -> KeyEvent {
        loop {
            let timeout =
                tokio::time::timeout(Duration::from_millis(500), self.events.next_event());
            let Ok(result) = timeout.await else {
                continue;
            };
            if let Some(event) = parse_key_event(result.unwrap()) {
                return event;
            }
        }
    }
}

fn open_input_device() -> Result<Device> {
    for path in event_paths()? {
        let device = Device::open(&path)?;
        if is_gamepad_device(&device) {
            return Ok(device);
        }
    }
    Err(anyhow!("No RG35XXSP gamepad input device found"))
}

fn event_paths() -> Result<Vec<PathBuf>> {
    let mut paths = std::fs::read_dir("/dev/input")?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| is_event_path(path))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn is_event_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("event"))
}

fn is_gamepad_device(device: &Device) -> bool {
    let name = device.name().unwrap_or_default().to_ascii_lowercase();
    if name.contains("gpio") || name.contains("joypad") || name.contains("gamepad") {
        return true;
    }
    device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::BTN_SOUTH))
}

fn parse_key_event(event: evdev::InputEvent) -> Option<KeyEvent> {
    if event.event_type() != EventType::KEY {
        return None;
    }
    if event.timestamp().elapsed().ok()? > MAXIMUM_FRAME_TIME {
        return None;
    }
    let key = map_key(event.code());
    match event.value() {
        0 => Some(KeyEvent::Released(key)),
        1 => Some(KeyEvent::Pressed(key)),
        2 => Some(KeyEvent::Autorepeat(key)),
        _ => None,
    }
}

fn map_key(code: u16) -> Key {
    match KeyCode(code) {
        KeyCode::KEY_UP | KeyCode::BTN_DPAD_UP => Key::Up,
        KeyCode::KEY_DOWN | KeyCode::BTN_DPAD_DOWN => Key::Down,
        KeyCode::KEY_LEFT | KeyCode::BTN_DPAD_LEFT => Key::Left,
        KeyCode::KEY_RIGHT | KeyCode::BTN_DPAD_RIGHT => Key::Right,
        KeyCode::KEY_SPACE | KeyCode::BTN_EAST => Key::A,
        KeyCode::KEY_LEFTCTRL | KeyCode::BTN_SOUTH => Key::B,
        KeyCode::KEY_LEFTSHIFT | KeyCode::BTN_NORTH => Key::X,
        KeyCode::KEY_LEFTALT | KeyCode::BTN_WEST => Key::Y,
        KeyCode::KEY_ENTER | KeyCode::BTN_START => Key::Start,
        KeyCode::KEY_RIGHTCTRL | KeyCode::BTN_SELECT => Key::Select,
        KeyCode::KEY_E | KeyCode::BTN_TL => Key::L,
        KeyCode::KEY_T | KeyCode::BTN_TR => Key::R,
        KeyCode::KEY_ESC | KeyCode::BTN_MODE => Key::Menu,
        KeyCode::KEY_TAB | KeyCode::BTN_TL2 => Key::L2,
        KeyCode::KEY_BACKSPACE | KeyCode::BTN_TR2 => Key::R2,
        KeyCode::KEY_POWER => Key::Power,
        KeyCode::KEY_VOLUMEDOWN => Key::VolDown,
        KeyCode::KEY_VOLUMEUP => Key::VolUp,
        _ => Key::Unknown,
    }
}
