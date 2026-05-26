//! Core options storage and libretro option API handling.
//!
//! Supports both legacy `RETRO_ENVIRONMENT_SET_VARIABLES`
//! and modern `RETRO_ENVIRONMENT_SET_CORE_OPTIONS`.

use std::collections::HashMap;

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// Internal representation of one core option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreOption {
    pub key: String,
    pub desc: String,
    pub values: Vec<String>,
    pub default: String,
}

/// Container for all core option definitions and current values.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CoreOptions {
    pub options: HashMap<String, CoreOption>,
    pub current_values: HashMap<String, String>,
    pub dirty: bool,
}

impl CoreOptions {
    /// Parse a legacy `retro_variable` array.
    /// Each entry: `value` = "description; option1|option2|option3"
    pub unsafe fn from_variables(raw: *const libretro::retro_variable) -> Self {
        let mut opts = Self::default();
        if raw.is_null() {
            return Self::default();
        }
        let mut ptr = raw;
        unsafe {
            while !(*ptr).key.is_null() {
                let key = std::ffi::CStr::from_ptr((*ptr).key)
                    .to_string_lossy()
                    .into_owned();
                let value_str = std::ffi::CStr::from_ptr((*ptr).value)
                    .to_string_lossy()
                    .into_owned();

                if let Some((desc, rest)) = value_str.split_once("; ") {
                    let values: Vec<String> = rest.split('|').map(|s| s.to_string()).collect();
                    let default = values.first().cloned().unwrap_or_default();
                    opts.options.insert(
                        key.clone(),
                        CoreOption {
                            key: key.clone(),
                            desc: desc.to_string(),
                            values: values.clone(),
                            default: default.clone(),
                        },
                    );
                    opts.current_values.insert(key, default);
                } else {
                    warn!("Core option '{}' has malformed value string: {}", key, value_str);
                }
                ptr = ptr.add(1);
            }
        }
        info!("Loaded {} legacy core options", opts.options.len());
        opts
    }

    /// Parse a modern `retro_core_option_definition` array.
    pub unsafe fn from_core_options(raw: *const libretro::retro_core_option_definition) -> Self {
        let mut opts = Self::default();
        if raw.is_null() {
            return Self::default();
        }
        let mut ptr = raw;
        unsafe {
            loop {
                let def = &*ptr;
                if def.key.is_null() {
                    break;
                }
                Self::parse_core_option_def(&mut opts, &*ptr);
                ptr = ptr.add(1);
            }
        }
        info!("Loaded {} modern core options", opts.options.len());
        opts
    }

    /// Parse a v2 `retro_core_options_v2` struct.
    pub unsafe fn from_core_options_v2(raw: *const libretro::retro_core_options_v2) -> Self {
        let mut opts = Self::default();
        if raw.is_null() {
            return Self::default();
        }
        unsafe {
            let defs = &*raw;
            if defs.definitions.is_null() {
                return opts;
            }
            let mut ptr = defs.definitions;
            loop {
                let def = &*ptr;
                if def.key.is_null() {
                    break;
                }
                Self::parse_core_option_v2_def(&mut opts, def);
                ptr = ptr.add(1);
            }
        }
        info!("Loaded {} v2 core options", opts.options.len());
        opts
    }

    unsafe fn parse_core_option_def(
        opts: &mut CoreOptions,
        def: &libretro::retro_core_option_definition,
    ) {
        let key = unsafe { std::ffi::CStr::from_ptr(def.key) }
            .to_string_lossy()
            .into_owned();
        let desc = if def.desc.is_null() {
            key.clone()
        } else {
            unsafe { std::ffi::CStr::from_ptr(def.desc) }
                .to_string_lossy()
                .into_owned()
        };
        let default = unsafe { std::ffi::CStr::from_ptr(def.default_value) }
            .to_string_lossy()
            .into_owned();

        let mut values = Vec::new();
        for i in 0..128 {
            let val = &def.values[i];
            if val.value.is_null() {
                break;
            }
            values.push(
                unsafe { std::ffi::CStr::from_ptr(val.value) }
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        if !values.is_empty() && values.contains(&default) {
            opts.options.insert(
                key.clone(),
                CoreOption {
                    key: key.clone(),
                    desc,
                    values: values.clone(),
                    default: default.clone(),
                },
            );
            opts.current_values.insert(key, default);
        } else {
            warn!(
                "Core option '{}' default '{}' not in values {:?}",
                key, default, values
            );
        }
    }

    unsafe fn parse_core_option_v2_def(
        opts: &mut CoreOptions,
        def: &libretro::retro_core_option_v2_definition,
    ) {
        let key = unsafe { std::ffi::CStr::from_ptr(def.key) }
            .to_string_lossy()
            .into_owned();
        let desc = if def.desc.is_null() {
            key.clone()
        } else {
            unsafe { std::ffi::CStr::from_ptr(def.desc) }
                .to_string_lossy()
                .into_owned()
        };
        let default = unsafe { std::ffi::CStr::from_ptr(def.default_value) }
            .to_string_lossy()
            .into_owned();

        let mut values = Vec::new();
        for i in 0..128 {
            let val = &def.values[i];
            if val.value.is_null() {
                break;
            }
            values.push(
                unsafe { std::ffi::CStr::from_ptr(val.value) }
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        if !values.is_empty() && values.contains(&default) {
            opts.options.insert(
                key.clone(),
                CoreOption {
                    key: key.clone(),
                    desc,
                    values: values.clone(),
                    default: default.clone(),
                },
            );
            opts.current_values.insert(key, default);
        } else {
            warn!(
                "Core option '{}' default '{}' not in values {:?}",
                key, default, values
            );
        }
    }

    /// Get the current value for a key, or the default if none set.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.current_values.get(key).map(|s| s.as_str())
    }

    /// Set a value. Returns true if the value actually changed.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        if let Some(opt) = self.options.get(key) {
            if opt.values.contains(&value.to_string()) {
                let old = self.current_values.get(key).cloned();
                if old.as_deref() != Some(value) {
                    self.current_values.insert(key.to_string(), value.to_string());
                    self.dirty = true;
                    return true;
                }
            } else {
                warn!(
                    "Tried to set core option '{}' to invalid value '{}' (valid: {:?})",
                    key, value, opt.values
                );
            }
        } else {
            warn!("Tried to set unknown core option '{}'", key);
        }
        false
    }

    /// Write the value pointer for GET_VARIABLE.
    /// Stores the string persistently so the pointer remains valid.
    pub fn get_ptr(&mut self, key: &str) -> Option<*const u8> {
        self.get(key).map(|s| {
            // Leak the string to keep it valid for the core's lifetime.
            // This is fine for the small number of core options.
            let leaked = Box::leak(s.to_string().into_boxed_str());
            leaked.as_ptr() as *const u8
        })
    }

    /// Check and reset the dirty flag.
    pub fn take_dirty(&mut self) -> bool {
        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }

    /// Merge values from a HashMap (e.g. loaded from disk).
    pub fn merge_values(&mut self, values: &HashMap<String, String>) {
        for (key, value) in values {
            if self.options.contains_key(key) {
                self.current_values.insert(key.clone(), value.clone());
            }
        }
        self.dirty = true;
    }
}

/// Persisted representation of core option values (key -> value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreOptionsConfig {
    #[serde(default)]
    pub options: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_changes_value() {
        let mut opts = CoreOptions::default();
        opts.options.insert(
            "test_opt".into(),
            CoreOption {
                key: "test_opt".into(),
                desc: "Test".into(),
                values: vec!["off".into(), "on".into()],
                default: "off".into(),
            },
        );
        opts.current_values.insert("test_opt".into(), "off".into());

        assert!(opts.set("test_opt", "on"));
        assert_eq!(opts.get("test_opt"), Some("on"));
        assert!(!opts.set("test_opt", "on")); // no change
    }

    #[test]
    fn rejects_invalid_value() {
        let mut opts = CoreOptions::default();
        opts.options.insert(
            "test_opt".into(),
            CoreOption {
                key: "test_opt".into(),
                desc: "Test".into(),
                values: vec!["off".into(), "on".into()],
                default: "off".into(),
            },
        );
        opts.current_values.insert("test_opt".into(), "off".into());

        assert!(!opts.set("test_opt", "invalid"));
        assert_eq!(opts.get("test_opt"), Some("off"));
    }
}
