use crate::args::Args;
use anyhow::{Result, anyhow};
use common::constants::{ALLIUM_BASE_DIR, ALLIUM_SD_ROOT};
use std::path::PathBuf;

// Centralizing paths keeps frontend state out of ROM folders.
#[derive(Debug, Clone)]
pub struct PlayPaths {
    pub rom: PathBuf,
    pub core_path: PathBuf,
    pub core_id: String,
    #[allow(dead_code)]
    pub save_dir: PathBuf,
    #[allow(dead_code)]
    pub state_dir: PathBuf,
    #[allow(dead_code)]
    pub config_dir: PathBuf,
}

impl PlayPaths {
    // The launcher owns content selection; Play owns only its config/state locations.
    pub fn from_args(args: &Args) -> Self {
        let core_id = args.core_id.clone();

        let save_dir = ALLIUM_SD_ROOT
            .join("Saves")
            .join("CurrentProfile")
            .join("play")
            .join(&core_id);
        let state_dir = ALLIUM_BASE_DIR.join("state").join("play").join(&core_id);
        let config_dir = ALLIUM_BASE_DIR.join("config").join("play").join(&core_id);

        Self {
            rom: args.rom.clone(),
            core_path: args.core_path.clone(),
            core_id,
            save_dir,
            state_dir,
            config_dir,
        }
    }

    pub fn sram_path(&self) -> PathBuf {
        self.save_dir.join(format!("{}.srm", self.rom_stem()))
    }

    pub fn state_path(&self, slot: i8) -> Result<PathBuf> {
        if !(-1..=9).contains(&slot) {
            return Err(anyhow!("Save state slot must be between 0 and 9"));
        }

        let file_name = if slot == -1 {
            format!("{}.auto.state", self.rom_stem())
        } else {
            format!("{}.state{}", self.rom_stem(), slot)
        };
        Ok(self.state_dir.join(file_name))
    }

    fn rom_stem(&self) -> String {
        self.rom
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("game")
            .to_string()
    }
}

// This guards against writing emulator files next to user ROM content.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_derivation() {
        let args = Args {
            rom: PathBuf::from("game.nes"),
            core_path: PathBuf::from("nestopia.so"),
            core_id: "nestopia".to_string(),
            dump_frame: None,
            frames: None,
            scale: crate::scale::ScaleMode::Aspect,
        };
        let paths = PlayPaths::from_args(&args);

        assert_eq!(paths.rom, PathBuf::from("game.nes"));
        assert_eq!(paths.core_path, PathBuf::from("nestopia.so"));
        assert_eq!(paths.core_id, "nestopia");
        assert!(
            paths
                .save_dir
                .to_string_lossy()
                .contains("Saves/CurrentProfile/play/nestopia")
        );
        assert!(
            paths
                .state_dir
                .to_string_lossy()
                .contains(".allium/state/play/nestopia")
        );
        assert!(
            paths
                .config_dir
                .to_string_lossy()
                .contains(".allium/config/play/nestopia")
        );
    }

    fn fceumm_paths() -> PlayPaths {
        PlayPaths::from_args(&Args {
            rom: PathBuf::from("/roms/NES/Alter Ego (World).nes"),
            core_path: PathBuf::from("fceumm_libretro.dylib"),
            core_id: "FCEUmm".to_string(),
            dump_frame: None,
            frames: None,
            scale: crate::scale::ScaleMode::Aspect,
        })
    }

    #[test]
    fn sram_path_uses_allium_save_area_and_rom_stem() {
        let path = fceumm_paths().sram_path();

        assert!(
            path.to_string_lossy()
                .contains("Saves/CurrentProfile/play/FCEUmm")
        );
        assert!(path.ends_with("Alter Ego (World).srm"));
        assert!(!path.to_string_lossy().contains("/roms/NES"));
    }

    #[test]
    fn state_slots_use_allium_state_area_and_rom_stem() {
        let paths = fceumm_paths();

        let slot_1 = paths.state_path(1).unwrap();
        let slot_2 = paths.state_path(2).unwrap();
        let slot_3 = paths.state_path(3).unwrap();

        assert!(
            slot_1
                .to_string_lossy()
                .contains(".allium/state/play/FCEUmm")
        );
        assert!(slot_1.ends_with("Alter Ego (World).state1"));
        assert!(slot_2.ends_with("Alter Ego (World).state2"));
        assert!(slot_3.ends_with("Alter Ego (World).state3"));
    }

    #[test]
    fn state_slot_rejects_values_above_nine() {
        let err = fceumm_paths().state_path(10).unwrap_err();

        assert_eq!(err.to_string(), "Save state slot must be between 0 and 9");
    }

    #[test]
    fn state_slot_minus_one_uses_autosave_path() {
        let path = fceumm_paths().state_path(-1).unwrap();

        assert!(path.ends_with("Alter Ego (World).auto.state"));
    }

    #[test]
    fn falls_back_when_rom_has_no_file_stem() {
        let args = Args {
            rom: PathBuf::from("/"),
            core_path: PathBuf::from("nestopia.so"),
            core_id: "nestopia".to_string(),
            dump_frame: None,
            frames: None,
            scale: crate::scale::ScaleMode::Aspect,
        };
        let paths = PlayPaths::from_args(&args);

        assert!(paths.sram_path().ends_with("game.srm"));
        assert!(paths.state_path(0).unwrap().ends_with("game.state0"));
    }
}
