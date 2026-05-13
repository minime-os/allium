use crate::args::Args;
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
}
