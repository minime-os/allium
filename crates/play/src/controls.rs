//! Control and shortcut bindings for Play.
//!
//! Handles per-core button filtering via `retro_input_descriptor` and
//! configurable physical-key → libretro-button mappings.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use common::platform::Key;
use log::info;
use serde::{Deserialize, Serialize};

use crate::paths::PlayPaths;
use crate::settings::SaveScope;
use libretro::*;

// ---- Input descriptors from core ----

/// One entry from the core's `RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS`.
#[derive(Debug, Clone)]
pub struct InputDescriptor {
    pub port: u32,
    pub device: u32,
    pub index: u32,
    pub id: u32,
    pub description: String,
}

/// Buttons the current core reports as present (port 0, joypad, index 0).
#[derive(Debug, Default, Clone)]
pub struct InputDescriptors {
    pub buttons: Vec<InputDescriptor>,
}

impl InputDescriptors {
    /// Accepts a null-terminated array of `retro_input_descriptor`.
    pub unsafe fn from_raw(raw: *const libretro::retro_input_descriptor) -> Self {
        let mut buttons = Vec::new();
        if raw.is_null() {
            return Self { buttons };
        }
        let mut ptr = raw;
        unsafe {
            while !(*ptr).description.is_null() {
                let desc = &*ptr;
                if desc.port == 0
                    && desc.device == RETRO_DEVICE_JOYPAD
                    && desc.index == 0
                    && desc.id < RETRO_DEVICE_ID_JOYPAD_MASK
                {
                    let description = std::ffi::CStr::from_ptr(desc.description)
                        .to_string_lossy()
                        .into_owned();
                    buttons.push(InputDescriptor {
                        port: desc.port,
                        device: desc.device,
                        index: desc.index,
                        id: desc.id,
                        description,
                    });
                }
                ptr = ptr.add(1);
            }
        }
        Self { buttons }
    }

    /// Returns the human description for a libretro button id, if the core
    /// provided one. Falls back to generic names.
    pub fn description_for(&self, id: u32) -> String {
        self.buttons
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.description.clone())
            .unwrap_or_else(|| retro_button_name(id).to_string())
    }

    /// True if this button id was reported present by the core.
    pub fn is_present(&self, id: u32) -> bool {
        self.buttons.iter().any(|b| b.id == id)
    }
}

// ---- Control mappings ----

const DEFAULT_CONTROLS: [(u32, Key); 16] = [
    (RETRO_DEVICE_ID_JOYPAD_B, Key::B),
    (RETRO_DEVICE_ID_JOYPAD_Y, Key::Y),
    (RETRO_DEVICE_ID_JOYPAD_SELECT, Key::Select),
    (RETRO_DEVICE_ID_JOYPAD_START, Key::Start),
    (RETRO_DEVICE_ID_JOYPAD_UP, Key::Up),
    (RETRO_DEVICE_ID_JOYPAD_DOWN, Key::Down),
    (RETRO_DEVICE_ID_JOYPAD_LEFT, Key::Left),
    (RETRO_DEVICE_ID_JOYPAD_RIGHT, Key::Right),
    (RETRO_DEVICE_ID_JOYPAD_A, Key::A),
    (RETRO_DEVICE_ID_JOYPAD_X, Key::X),
    (RETRO_DEVICE_ID_JOYPAD_L, Key::L),
    (RETRO_DEVICE_ID_JOYPAD_R, Key::R),
    (RETRO_DEVICE_ID_JOYPAD_L2, Key::L2),
    (RETRO_DEVICE_ID_JOYPAD_R2, Key::R2),
    (RETRO_DEVICE_ID_JOYPAD_L3, Key::Menu), // no L3/R3 on Miyoo
    (RETRO_DEVICE_ID_JOYPAD_R3, Key::Menu),
];

/// Maps libretro joypad IDs to physical keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlBindings {
    /// retro_id (as string name) -> physical key (as string name).
    #[serde(default)]
    pub mapping: HashMap<String, String>,
}

impl Default for ControlBindings {
    fn default() -> Self {
        let mut mapping = HashMap::new();
        for (id, key) in DEFAULT_CONTROLS {
            mapping.insert(retro_button_name(id).into(), key.to_string());
        }
        Self { mapping }
    }
}

impl ControlBindings {
    /// Returns the physical key mapped to this libretro button id.
    /// Falls back to the baked-in default if no override exists.
    pub fn key_for_retro_id(&self, id: u32) -> Option<Key> {
        let name = retro_button_name(id);
        self.mapping
            .get(name)
            .and_then(|s| s.parse::<Key>().ok())
            .or_else(|| {
                DEFAULT_CONTROLS
                    .iter()
                    .find(|(rid, _)| *rid == id)
                    .map(|(_, k)| *k)
            })
    }

    /// Set a binding: libretro button name -> physical key name.
    pub fn set(&mut self, retro_name: &str, key_name: &str) {
        self.mapping.insert(retro_name.into(), key_name.into());
    }

    /// Clear a binding (revert to default on next lookup).
    pub fn clear(&mut self, retro_name: &str) {
        self.mapping.remove(retro_name);
    }
}

fn retro_button_name(id: u32) -> &'static str {
    match id {
        0 => "B",
        1 => "Y",
        2 => "Select",
        3 => "Start",
        4 => "Up",
        5 => "Down",
        6 => "Left",
        7 => "Right",
        8 => "A",
        9 => "X",
        10 => "L",
        11 => "R",
        12 => "L2",
        13 => "R2",
        14 => "L3",
        15 => "R3",
        _ => "Unknown",
    }
}

// ---- Shortcut mappings ----

const MENU_PREFIX: &str = "MENU+";

/// Maps frontend actions (shortcuts) to physical button combos.
/// A combo may be a single key or "MENU+" prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortcutBindings {
    /// action name -> combo string (e.g. "MENU+X" or "Start").
    #[serde(default)]
    pub mapping: HashMap<String, String>,
}

impl Default for ShortcutBindings {
    fn default() -> Self {
        let mut mapping = HashMap::new();
        mapping.insert("toggle_menu".into(), "Menu".into());
        mapping.insert("toggle_fast_forward".into(), "MENU+X".into());
        mapping.insert("save_state".into(), "MENU+Y".into());
        mapping.insert("load_state".into(), "MENU+R".into());
        mapping.insert("reset".into(), "MENU+Start".into());
        Self { mapping }
    }
}

impl ShortcutBindings {
    /// Parse a combo string like "MENU+X" or "Start" into (requires_menu, key).
    pub fn parse_combo(s: &str) -> Option<(bool, Key)> {
        if s.starts_with(MENU_PREFIX) {
            s[MENU_PREFIX.len()..].parse().ok().map(|k| (true, k))
        } else {
            s.parse().ok().map(|k| (false, k))
        }
    }

    pub fn set(&mut self, action: &str, combo: &str) {
        self.mapping.insert(action.into(), combo.into());
    }

    pub fn clear(&mut self, action: &str) {
        self.mapping.remove(action);
    }

    /// Poll `keys` for shortcuts and return triggered action names.
    /// `menu_held` should be `true` if the MENU key is currently pressed.
    pub fn poll(&self, keys: &HashSet<Key>, menu_held: bool) -> Vec<String> {
        let mut actions = Vec::new();
        for (action, combo) in &self.mapping {
            let Some((needs_menu, key)) = Self::parse_combo(combo) else {
                continue;
            };
            if keys.contains(&key) {
                if needs_menu && !menu_held {
                    continue;
                }
                actions.push(action.clone());
            }
        }
        actions
    }
}

// ---- Persistence ----

pub fn load_control_bindings(paths: &PlayPaths) -> ControlBindings {
    let path = paths.config_dir.join("controls.toml");
    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Failed to parse controls.toml: {}, using defaults", e);
                ControlBindings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ControlBindings::default(),
        Err(e) => {
            log::warn!("Failed to read controls.toml: {}, using defaults", e);
            ControlBindings::default()
        }
    }
}

pub fn save_control_bindings(
    paths: &PlayPaths,
    _scope: SaveScope,
    bindings: &ControlBindings,
) -> Result<()> {
    let path = paths.config_dir.join("controls.toml");
    std::fs::create_dir_all(&paths.config_dir).context("Failed to create config dir")?;
    let content = toml::to_string_pretty(bindings).context("Failed to serialise controls")?;
    std::fs::write(&path, content).context("Failed to write controls.toml")?;
    info!("Saved controls to {:?}", path);
    Ok(())
}

pub fn load_shortcut_bindings(_paths: &PlayPaths) -> ShortcutBindings {
    let path = common::constants::ALLIUM_BASE_DIR
        .join("config")
        .join("play")
        .join("shortcuts.toml");
    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Failed to parse shortcuts.toml: {}, using defaults", e);
                ShortcutBindings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ShortcutBindings::default(),
        Err(e) => {
            log::warn!("Failed to read shortcuts.toml: {}, using defaults", e);
            ShortcutBindings::default()
        }
    }
}

pub fn save_shortcut_bindings(_paths: &PlayPaths, bindings: &ShortcutBindings) -> Result<()> {
    let path = common::constants::ALLIUM_BASE_DIR
        .join("config")
        .join("play")
        .join("shortcuts.toml");
    std::fs::create_dir_all(path.parent().unwrap()).context("Failed to create config dir")?;
    let content = toml::to_string_pretty(bindings).context("Failed to serialise shortcuts")?;
    std::fs::write(&path, content).context("Failed to write shortcuts.toml")?;
    info!("Saved shortcuts to {:?}", path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_controls_resolve() {
        let controls = ControlBindings::default();
        assert_eq!(
            controls.key_for_retro_id(RETRO_DEVICE_ID_JOYPAD_A),
            Some(Key::A)
        );
        assert_eq!(
            controls.key_for_retro_id(RETRO_DEVICE_ID_JOYPAD_B),
            Some(Key::B)
        );
        assert_eq!(
            controls.key_for_retro_id(RETRO_DEVICE_ID_JOYPAD_X),
            Some(Key::X)
        );
    }

    #[test]
    fn override_changes_mapping() {
        let mut controls = ControlBindings::default();
        controls.set("A", "L");
        assert_eq!(
            controls.key_for_retro_id(RETRO_DEVICE_ID_JOYPAD_A),
            Some(Key::L)
        );
    }

    #[test]
    fn clear_reverts_to_default() {
        let mut controls = ControlBindings::default();
        controls.set("A", "L");
        controls.clear("A");
        assert_eq!(
            controls.key_for_retro_id(RETRO_DEVICE_ID_JOYPAD_A),
            Some(Key::A)
        );
    }

    #[test]
    fn shortcut_parse_single_key() {
        assert_eq!(ShortcutBindings::parse_combo("X"), Some((false, Key::X)));
    }

    #[test]
    fn shortcut_parse_menu_combo() {
        assert_eq!(
            ShortcutBindings::parse_combo("MENU+Y"),
            Some((true, Key::Y))
        );
    }

    #[test]
    fn shortcut_poll_fires_when_key_pressed() {
        let shortcuts = ShortcutBindings::default();
        let mut keys = HashSet::new();
        keys.insert(Key::X);
        let actions = shortcuts.poll(&keys, true); // MENU held
        assert!(actions.contains(&"toggle_fast_forward".to_string()));
    }

    #[test]
    fn shortcut_poll_skips_if_menu_not_held() {
        let shortcuts = ShortcutBindings::default();
        let mut keys = HashSet::new();
        keys.insert(Key::X);
        let actions = shortcuts.poll(&keys, false); // MENU not held
        assert!(!actions.contains(&"toggle_fast_forward".to_string()));
    }

    #[test]
    fn roundtrip_control_bindings() {
        let mut b = ControlBindings::default();
        b.set("A", "L");
        let serialised = toml::to_string_pretty(&b).unwrap();
        let deserialised: ControlBindings = toml::from_str(&serialised).unwrap();
        assert_eq!(b, deserialised);
    }

    #[test]
    fn roundtrip_shortcut_bindings() {
        let mut b = ShortcutBindings::default();
        b.set("toggle_menu", "L");
        let serialised = toml::to_string_pretty(&b).unwrap();
        let deserialised: ShortcutBindings = toml::from_str(&serialised).unwrap();
        assert_eq!(b, deserialised);
    }
}
