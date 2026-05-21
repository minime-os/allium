use anyhow::{Context, Result};
use common::constants::ALLIUM_CONFIG_PLAY;
use serde::Deserialize;

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
