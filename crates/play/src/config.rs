//! Manages all configuration for the Play application.
//! This includes parsing command-line arguments (CLI overrides) and loading/parsing
//! the application configuration file (TOML settings) to form a unified configuration.

// File Flow:
// 1. CLI Arguments (`Args` struct and parsing).
// 2. File Configuration (`PlayConfig` struct, default values, and loading from disk).
// 3. Helper Structs for TOML parsing (`ConfigFile`, `PlaySection`).
// 4. Combined tests for both CLI parsing and file configuration.

use anyhow::{Context, Result};
use clap::Parser;
use common::constants::ALLIUM_CONFIG_PLAY;
use serde::Deserialize;
use std::path::PathBuf;

use crate::scale::ScaleMode;

#[derive(Debug, Parser, PartialEq)]
#[command(name = "play")]
pub struct Args {
    #[arg(long)]
    pub rom: PathBuf,

    #[arg(long)]
    pub core: PathBuf,

    #[arg(long = "core-id")]
    pub core_id: String,

    #[arg(long)]
    pub dump_frame: Option<PathBuf>,

    #[arg(long)]
    pub frames: Option<u64>,

    #[arg(long, default_value = "aspect")]
    pub scale: ScaleMode,

    #[arg(long)]
    pub hud: bool,
}

impl Args {
    /// Parses CLI arguments directly from the OS environment to configure the execution of Play.
    pub fn from_env() -> Result<Self> {
        Ok(Self::parse())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayConfig {
    pub autosave: bool,
    pub autoload: bool,
    pub hud: bool,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    play: Option<PlaySection>,
}

#[derive(Debug, Deserialize)]
struct PlaySection {
    autosave: Option<bool>,
    autoload: Option<bool>,
    hud: Option<bool>,
}

impl Default for PlayConfig {
    fn default() -> Self {
        Self {
            autosave: true,
            autoload: true,
            hud: false,
        }
    }
}

impl PlayConfig {
    pub fn load() -> Result<Self> {
        let contents = match std::fs::read_to_string(&*ALLIUM_CONFIG_PLAY) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err).with_context(|| ALLIUM_CONFIG_PLAY.display().to_string()),
        };

        Self::from_str(&contents)
    }

    fn from_str(contents: &str) -> Result<Self> {
        let parsed: ConfigFile = toml::from_str(contents)?;
        let Some(play) = parsed.play else {
            return Ok(Self::default());
        };

        Ok(Self {
            autosave: play.autosave.unwrap_or(true),
            autoload: play.autoload.unwrap_or(true),
            hud: play.hud.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid() {
        let args = Args::parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
        ]);
        assert_eq!(args.rom, PathBuf::from("test.nes"));
        assert_eq!(args.core, PathBuf::from("nes_libretro.so"));
        assert_eq!(args.core_id, "nes");
        assert_eq!(args.scale, ScaleMode::Aspect);
        assert!(!args.hud);
    }

    #[test]
    fn parse_dump_frame() {
        let args = Args::parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--dump-frame",
            "frame.ppm",
        ]);
        assert_eq!(args.dump_frame, Some(PathBuf::from("frame.ppm")));
    }

    #[test]
    fn parse_frames() {
        let args = Args::parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--frames",
            "600",
        ]);
        assert_eq!(args.frames, Some(600));
    }

    #[test]
    fn parse_scale_native() {
        let args = Args::parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--scale",
            "native",
        ]);
        assert_eq!(args.scale, ScaleMode::Native);
    }

    #[test]
    fn parse_scale_fullscreen() {
        let args = Args::parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--scale",
            "fullscreen",
        ]);
        assert_eq!(args.scale, ScaleMode::Fullscreen);
    }

    #[test]
    fn missing_rom_fails() {
        let result = Args::try_parse_from(["play", "--core", "nes_libretro.so", "--core-id", "nes"]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_scale_fails() {
        let result = Args::try_parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--scale",
            "wide",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_hud() {
        let args = Args::parse_from([
            "play",
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--hud",
        ]);
        assert!(args.hud);
    }

    #[test]
    fn defaults_autosave_and_autoload_on() {
        assert_eq!(
            PlayConfig::from_str("[play]\n").unwrap(),
            PlayConfig {
                autosave: true,
                autoload: true,
                hud: false,
            }
        );
    }

    #[test]
    fn config_can_disable_autosave_or_autoload() {
        assert_eq!(
            PlayConfig::from_str("[play]\nautosave = false\nautoload = false\n").unwrap(),
            PlayConfig {
                autosave: false,
                autoload: false,
                hud: false,
            }
        );
    }

    #[test]
    fn config_can_enable_hud() {
        assert_eq!(
            PlayConfig::from_str("[play]\nhud = true\n").unwrap(),
            PlayConfig {
                autosave: true,
                autoload: true,
                hud: true,
            }
        );
    }
}
