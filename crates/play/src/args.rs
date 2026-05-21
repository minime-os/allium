use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use crate::scale::ScaleMode;

// Play is launched by Allium, so the CLI is a small contract between processes.
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
    pub fn from_env() -> Result<Self> {
        Ok(Self::parse())
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
}
