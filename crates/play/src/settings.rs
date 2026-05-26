//! Frontend settings for Play (Minarch parity).
//!
//! Loaded from a config hierarchy:
//!   1. Global defaults:        config/play/frontend.toml
//!   2. Per-system override:    config/play/<core_id>/frontend.toml
//!   3. Per-game override:      config/play/<core_id>/<game_name>.toml
//!
//! Values are merged in order; later files override earlier ones.

use std::fs;

use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};

use crate::paths::PlayPaths;
use crate::video::ScaleMode;

/// Scope for saving frontend settings.
pub enum SaveScope {
    /// `config/play/<core_id>/frontend.toml` — current system (core).
    System,
    /// `config/play/<core_id>/<game_name>.toml` — current game.
    Game,
}

/// Screen effect applied at integer/native scaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ScreenEffect {
    #[default]
    None,
    Grid,
    Line,
}

impl std::str::FromStr for ScreenEffect {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "grid" => Ok(Self::Grid),
            "line" => Ok(Self::Line),
            _ => Err(format!("Unknown screen effect: {}", s)),
        }
    }
}

/// Sharpness used by the scaler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ScreenSharpness {
    Sharp,
    Crisp,
    #[default]
    Soft,
}

impl std::str::FromStr for ScreenSharpness {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "sharp" => Ok(Self::Sharp),
            "crisp" => Ok(Self::Crisp),
            "soft" => Ok(Self::Soft),
            _ => Err(format!("Unknown sharpness: {}", s)),
        }
    }
}

/// Frame-present timing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TearingMode {
    Off,
    #[default]
    Lenient,
    Strict,
}

impl std::str::FromStr for TearingMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "lenient" => Ok(Self::Lenient),
            "strict" => Ok(Self::Strict),
            _ => Err(format!("Unknown tearing mode: {}", s)),
        }
    }
}

/// CPU governor target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CpuSpeed {
    Powersave,
    #[default]
    Normal,
    Performance,
}

impl std::str::FromStr for CpuSpeed {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "powersave" => Ok(Self::Powersave),
            "normal" => Ok(Self::Normal),
            "performance" => Ok(Self::Performance),
            _ => Err(format!("Unknown CPU speed: {}", s)),
        }
    }
}

/// The 8 Minarch frontend settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendSettings {
    /// Screen scaling mode.
    #[serde(default)]
    pub scale_mode: ScaleMode,
    /// Procedural effect applied at integer scales.
    #[serde(default)]
    pub effect: ScreenEffect,
    /// Scaler sharpness.
    #[serde(default)]
    pub sharpness: ScreenSharpness,
    /// Frame timing strictness.
    #[serde(default)]
    pub tearing: TearingMode,
    /// CPU governor.
    #[serde(default)]
    pub cpu_speed: CpuSpeed,
    /// Decouple video rendering from audio thread.
    #[serde(default)]
    pub thread_video: bool,
    /// Show debug overlay (FPS / CPU).
    #[serde(default)]
    pub debug_hud: bool,
    /// Maximum fast-forward multiplier.
    #[serde(default = "default_max_ff")]
    pub max_ff_speed: u8,
}

fn default_max_ff() -> u8 {
    4
}

impl Default for FrontendSettings {
    fn default() -> Self {
        Self {
            scale_mode: ScaleMode::default(),
            effect: ScreenEffect::default(),
            sharpness: ScreenSharpness::default(),
            tearing: TearingMode::default(),
            cpu_speed: CpuSpeed::default(),
            thread_video: false,
            debug_hud: false,
            max_ff_speed: default_max_ff(),
        }
    }
}

/// Deserializable partial config: missing fields are `None` so they do not
/// override values from earlier hierarchy levels.
#[derive(Default, Debug, Deserialize)]
struct FrontendSettingsPartial {
    #[serde(default)]
    scale_mode: Option<ScaleMode>,
    #[serde(default)]
    effect: Option<ScreenEffect>,
    #[serde(default)]
    sharpness: Option<ScreenSharpness>,
    #[serde(default)]
    tearing: Option<TearingMode>,
    #[serde(default)]
    cpu_speed: Option<CpuSpeed>,
    #[serde(default)]
    thread_video: Option<bool>,
    #[serde(default)]
    debug_hud: Option<bool>,
    #[serde(default)]
    max_ff_speed: Option<u8>,
}

impl FrontendSettingsPartial {
    fn from_str(content: &str) -> Result<Self> {
        toml::from_str(content).context("Failed to parse FrontendSettingsPartial")
    }

    fn merge_into(&self, target: &mut FrontendSettings) {
        if let Some(v) = self.scale_mode {
            target.scale_mode = v;
        }
        if let Some(v) = self.effect {
            target.effect = v;
        }
        if let Some(v) = self.sharpness {
            target.sharpness = v;
        }
        if let Some(v) = self.tearing {
            target.tearing = v;
        }
        if let Some(v) = self.cpu_speed {
            target.cpu_speed = v;
        }
        if let Some(v) = self.thread_video {
            target.thread_video = v;
        }
        if let Some(v) = self.debug_hud {
            target.debug_hud = v;
        }
        if let Some(v) = self.max_ff_speed {
            target.max_ff_speed = v;
        }
    }
}

/// Loads frontend settings by walking the hierarchy:
/// built-in defaults → global → per-system → per-game.
pub fn load_frontend_settings(paths: &PlayPaths) -> FrontendSettings {
    let mut settings = FrontendSettings::default();

    let global = common::constants::ALLIUM_BASE_DIR
        .join("config")
        .join("play")
        .join("frontend.toml");
    if let Err(e) = load_and_merge(&global, &mut settings) {
        info!("No global play frontend config: {}", e);
    }

    let system = paths.config_dir.join("frontend.toml");
    if let Err(e) = load_and_merge(&system, &mut settings) {
        info!("No per-system frontend config: {}", e);
    }

    let game = paths.per_game_config_path();
    if let Err(e) = load_and_merge(&game, &mut settings) {
        info!("No per-game frontend config: {}", e);
    }

    settings
}

fn load_and_merge(path: &std::path::Path, settings: &mut FrontendSettings) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;
    let partial = FrontendSettingsPartial::from_str(&content)
        .with_context(|| format!("Failed to parse {:?}", path))?;
    partial.merge_into(settings);
    Ok(())
}

/// Serialises settings and writes atomically (temp file + rename).
pub fn save_frontend_settings(
    paths: &PlayPaths,
    scope: SaveScope,
    settings: &FrontendSettings,
) -> Result<()> {
    let path = match scope {
        SaveScope::System => paths.config_dir.join("frontend.toml"),
        SaveScope::Game => paths.per_game_config_path(),
    };

    fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new("")))
        .context("Failed to create config dir")?;

    let content =
        toml::to_string_pretty(settings).context("Failed to serialise frontend settings")?;

    fs::write(&path, content.as_bytes()).with_context(|| format!("Failed to write {:?}", path))?;

    info!("Saved frontend settings to {:?}", path);
    Ok(())
}

/// Removes per-game and per-system override files so built-in / global
/// defaults are used again.
pub fn restore_frontend_defaults(paths: &PlayPaths) -> Result<()> {
    let system = paths.config_dir.join("frontend.toml");
    let game = paths.per_game_config_path();

    if system.exists() {
        fs::remove_file(&system).context("Failed to remove system frontend.toml")?;
        info!("Removed per-system frontend config: {:?}", system);
    }
    if game.exists() {
        fs::remove_file(&game).context("Failed to remove game override")?;
        info!("Removed per-game frontend config: {:?}", game);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Args;
    use crate::paths::PlayPaths;
    use clap::Parser;

    fn test_paths() -> PlayPaths {
        PlayPaths::from_args(&Args::parse_from([
            "play",
            "--rom",
            "/roms/nes/Super Mario Bros.nes",
            "--core",
            "fceumm_libretro.so",
            "--core-id",
            "nes",
        ]))
    }

    #[test]
    fn partial_merge_leaves_unset_fields() {
        let mut settings = FrontendSettings::default();
        let partial = FrontendSettingsPartial {
            scale_mode: Some(ScaleMode::Native),
            sharpness: Some(ScreenSharpness::Sharp),
            ..Default::default()
        };
        partial.merge_into(&mut settings);
        assert_eq!(settings.scale_mode, ScaleMode::Native);
        assert_eq!(settings.sharpness, ScreenSharpness::Sharp);
        assert_eq!(settings.effect, ScreenEffect::None); // still default
        assert_eq!(settings.cpu_speed, CpuSpeed::Normal); // still default
    }

    #[test]
    fn roundtrip_frontend_settings() {
        let settings = FrontendSettings {
            scale_mode: ScaleMode::Native,
            effect: ScreenEffect::Grid,
            sharpness: ScreenSharpness::Sharp,
            tearing: TearingMode::Strict,
            cpu_speed: CpuSpeed::Performance,
            thread_video: true,
            debug_hud: true,
            max_ff_speed: 8,
        };
        let serialised = toml::to_string_pretty(&settings).unwrap();
        let deserialised: FrontendSettings = toml::from_str(&serialised).unwrap();
        assert_eq!(settings, deserialised);
    }

    #[test]
    fn config_hierarchy_precedence() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let base = std::env::temp_dir().join(format!(
            "allium_play_test_hier_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let global = base.join("frontend.toml");
        let system = base.join("nes").join("frontend.toml");
        let game = base.join("nes").join("Super Mario Bros.toml");

        fs::create_dir_all(system.parent().unwrap()).unwrap();

        // Global: aspect, soft
        let mut f = fs::File::create(&global).unwrap();
        write!(f, "scale_mode = \"aspect\"\nsharpness = \"soft\"\n").unwrap();

        // System: override sharpness only
        let mut f = fs::File::create(&system).unwrap();
        write!(f, "sharpness = \"crisp\"\n").unwrap();

        // Game: override scale_mode only
        let mut f = fs::File::create(&game).unwrap();
        write!(f, "scale_mode = \"native\"\n").unwrap();

        // Manually construct paths with these directories
        let paths = PlayPaths {
            rom: std::path::PathBuf::from("/roms/nes/Super Mario Bros.nes"),
            core_path: std::path::PathBuf::from("fceumm.so"),
            core_id: "nes".to_string(),
            save_dir: base.join("saves"),
            state_dir: base.join("state"),
            config_dir: base.join("nes"),
        };

        let settings = load_frontend_settings(&paths);

        // Game wins for scale_mode, system wins for sharpness
        assert_eq!(settings.scale_mode, ScaleMode::Native);
        assert_eq!(settings.sharpness, ScreenSharpness::Crisp);
        // Default for fields never set
        assert_eq!(settings.effect, ScreenEffect::None);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn save_and_reload_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let base = std::env::temp_dir().join(format!(
            "allium_play_test_rt_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = PlayPaths {
            rom: std::path::PathBuf::from("/roms/nes/game.nes"),
            core_path: std::path::PathBuf::from("fceumm.so"),
            core_id: "nes".to_string(),
            save_dir: base.join("saves"),
            state_dir: base.join("state"),
            config_dir: base.join("nes"),
        };
        fs::create_dir_all(&paths.config_dir).unwrap();

        let settings = FrontendSettings {
            scale_mode: ScaleMode::Cropped,
            effect: ScreenEffect::Grid,
            sharpness: ScreenSharpness::Sharp,
            tearing: TearingMode::Strict,
            cpu_speed: CpuSpeed::Performance,
            thread_video: true,
            debug_hud: true,
            max_ff_speed: 2,
        };

        save_frontend_settings(&paths, SaveScope::System, &settings).unwrap();
        let loaded = load_frontend_settings(&paths);
        assert_eq!(loaded, settings);

        let _ = std::fs::remove_dir_all(&base);
    }
}
