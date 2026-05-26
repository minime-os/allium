use std::{borrow::Cow, time::Duration};

use anyhow::Result;
use log::{debug, error, trace};
use tokio::net::UdpSocket;

use crate::constants::RETROARCH_UDP_SOCKET;

#[allow(unused)]
#[derive(Debug)]
pub enum RetroArchCommand {
    FastForward,
    FastForwardHold,
    LoadState,
    SaveState,
    FullscreenToggle,
    Quit,
    StateSlotPlus,
    StateSlotMinus,
    Rewind,
    MovieRecordToggle,
    PauseToggle,
    FrameAdvance,
    Reset,
    ShaderNext,
    ShaderPrev,
    CheatIndexPlus,
    CheatIndexMinus,
    CheatToggle,
    Screenshot,
    Mute,
    NetplayFlip,
    SlowMotion,
    VolumeUp,
    VolumeDown,
    OverlayNext,
    DiskEjectToggle,
    DiskNext,
    DiskPrev,
    GrabMouseToggle,
    MenuToggle,
    Pause,
    Unpause,
    GetInfo,
    GetDiskCount,
    GetDiskSlot,
    SetDiskSlot(u8),
    GetStateSlot,
    SetStateSlot(i8),
    SaveStateSlot(i8),
    LoadStateSlot(i8),
    // Play-specific settings commands (not understood by RetroArch).
    SetScale(String),
    SetEffect(String),
    SetSharpness(String),
    SetTearing(String),
    SetOverclock(String),
    SetThreadVideo(bool),
    SetDebugHUD(bool),
    SetMaxFF(u8),
    SetCoreOption { key: String, value: String },
    ReloadConfig,
    SetControl { retro_button: String, key: String },
    SetShortcut { action: String, combo: String },
}

impl RetroArchCommand {
    pub async fn send(&self) -> Result<()> {
        debug!("Sending RetroArch command: {}", self.as_str());
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        trace!("Bound UDP socket: {}", socket.local_addr()?);
        socket.connect(RETROARCH_UDP_SOCKET).await?;
        trace!(
            "Connecting to RetroArch UDP socket: {}",
            RETROARCH_UDP_SOCKET
        );
        socket.send(self.as_str().as_bytes()).await?;
        Ok(())
    }

    pub async fn send_recv(&self) -> Result<Option<String>> {
        debug!("Sending and awaiting RetroArch command: {}", self.as_str(),);
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        trace!("Bound UDP socket: {}", socket.local_addr()?);
        socket.connect(RETROARCH_UDP_SOCKET).await?;
        trace!(
            "Connecting to RetroArch UDP socket: {}",
            RETROARCH_UDP_SOCKET
        );
        socket.send(self.as_str().as_bytes()).await?;
        let mut reply = vec![0; 128];
        match tokio::time::timeout(Duration::from_millis(250), socket.recv_from(&mut reply)).await {
            Ok(Ok((len, _socket))) => {
                reply.truncate(len);
                let reply = String::from_utf8(reply)?;
                debug!("Received reply from RetroArch: {:?}", reply);
                Ok(Some(reply))
            }
            Ok(Err(e)) => {
                error!("Error receiving from RetroArch: {}", e);
                Err(e.into())
            }
            Err(e) => {
                error!("Timeout receiving from RetroArch: {}", e);
                Ok(None)
            }
        }
    }

    fn as_str(&self) -> Cow<'static, str> {
        match self {
            RetroArchCommand::FastForward => Cow::Borrowed("FAST_FORWARD"),
            RetroArchCommand::FastForwardHold => Cow::Borrowed("FAST_FORWARD_HOLD"),
            RetroArchCommand::LoadState => Cow::Borrowed("LOAD_STATE"),
            RetroArchCommand::SaveState => Cow::Borrowed("SAVE_STATE"),
            RetroArchCommand::FullscreenToggle => Cow::Borrowed("FULLSCREEN_TOGGLE"),
            RetroArchCommand::Quit => Cow::Borrowed("QUIT"),
            RetroArchCommand::StateSlotPlus => Cow::Borrowed("STATE_SLOT_PLUS"),
            RetroArchCommand::StateSlotMinus => Cow::Borrowed("STATE_SLOT_MINUS"),
            RetroArchCommand::Rewind => Cow::Borrowed("REWIND"),
            RetroArchCommand::MovieRecordToggle => Cow::Borrowed("MOVIE_RECORD_TOGGLE"),
            RetroArchCommand::PauseToggle => Cow::Borrowed("PAUSE_TOGGLE"),
            RetroArchCommand::FrameAdvance => Cow::Borrowed("FRAMEADVANCE"),
            RetroArchCommand::Reset => Cow::Borrowed("RESET"),
            RetroArchCommand::ShaderNext => Cow::Borrowed("SHADER_NEXT"),
            RetroArchCommand::ShaderPrev => Cow::Borrowed("SHADER_PREV"),
            RetroArchCommand::CheatIndexPlus => Cow::Borrowed("CHEAT_INDEX_PLUS"),
            RetroArchCommand::CheatIndexMinus => Cow::Borrowed("CHEAT_INDEX_MINUS"),
            RetroArchCommand::CheatToggle => Cow::Borrowed("CHEAT_TOGGLE"),
            RetroArchCommand::Screenshot => Cow::Borrowed("SCREENSHOT"),
            RetroArchCommand::Mute => Cow::Borrowed("MUTE"),
            RetroArchCommand::NetplayFlip => Cow::Borrowed("NETPLAY_FLIP"),
            RetroArchCommand::SlowMotion => Cow::Borrowed("SLOWMOTION"),
            RetroArchCommand::VolumeUp => Cow::Borrowed("VOLUME_UP"),
            RetroArchCommand::VolumeDown => Cow::Borrowed("VOLUME_DOWN"),
            RetroArchCommand::OverlayNext => Cow::Borrowed("OVERLAY_NEXT"),
            RetroArchCommand::DiskEjectToggle => Cow::Borrowed("DISK_EJECT_TOGGLE"),
            RetroArchCommand::DiskNext => Cow::Borrowed("DISK_NEXT"),
            RetroArchCommand::DiskPrev => Cow::Borrowed("DISK_PREV"),
            RetroArchCommand::GrabMouseToggle => Cow::Borrowed("GRAB_MOUSE_TOGGLE"),
            RetroArchCommand::MenuToggle => Cow::Borrowed("MENU_TOGGLE"),
            RetroArchCommand::Pause => Cow::Borrowed("PAUSE"),
            RetroArchCommand::Unpause => Cow::Borrowed("UNPAUSE"),
            RetroArchCommand::GetInfo => Cow::Borrowed("GET_INFO"),
            RetroArchCommand::GetDiskCount => Cow::Borrowed("GET_DISK_COUNT"),
            RetroArchCommand::GetDiskSlot => Cow::Borrowed("GET_DISK_SLOT"),
            RetroArchCommand::SetDiskSlot(slot) => Cow::Owned(format!("SET_DISK_SLOT {slot}")),
            RetroArchCommand::GetStateSlot => Cow::Borrowed("GET_STATE_SLOT"),
            RetroArchCommand::SetStateSlot(slot) => Cow::Owned(format!("SET_STATE_SLOT {slot}")),
            RetroArchCommand::SaveStateSlot(slot) => Cow::Owned(format!("SAVE_STATE_SLOT {slot}")),
            RetroArchCommand::LoadStateSlot(slot) => Cow::Owned(format!("LOAD_STATE_SLOT {slot}")),
            RetroArchCommand::SetScale(mode) => Cow::Owned(format!("SET_SCALE {mode}")),
            RetroArchCommand::SetEffect(mode) => Cow::Owned(format!("SET_EFFECT {mode}")),
            RetroArchCommand::SetSharpness(mode) => Cow::Owned(format!("SET_SHARPNESS {mode}")),
            RetroArchCommand::SetTearing(mode) => Cow::Owned(format!("SET_TEARING {mode}")),
            RetroArchCommand::SetOverclock(mode) => Cow::Owned(format!("SET_OVERCLOCK {mode}")),
            RetroArchCommand::SetThreadVideo(enabled) => {
                Cow::Owned(format!("SET_THREAD_VIDEO {enabled}"))
            }
            RetroArchCommand::SetDebugHUD(enabled) => {
                Cow::Owned(format!("SET_DEBUG_HUD {enabled}"))
            }
            RetroArchCommand::SetMaxFF(speed) => Cow::Owned(format!("SET_MAX_FF {speed}")),
            RetroArchCommand::SetCoreOption { key, value } => {
                Cow::Owned(format!("SET_CORE_OPTION {key} {value}"))
            }
            RetroArchCommand::ReloadConfig => Cow::Borrowed("RELOAD_CONFIG"),
            RetroArchCommand::SetControl { retro_button, key } => {
                Cow::Owned(format!("SET_CONTROL {retro_button} {key}"))
            }
            RetroArchCommand::SetShortcut { action, combo } => {
                Cow::Owned(format!("SET_SHORTCUT {action} {combo}"))
            }
        }
    }
}

impl std::str::FromStr for RetroArchCommand {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Empty command".to_string());
        }

        match parts[0] {
            "FAST_FORWARD" => Ok(RetroArchCommand::FastForward),
            "FAST_FORWARD_HOLD" => Ok(RetroArchCommand::FastForwardHold),
            "LOAD_STATE" => Ok(RetroArchCommand::LoadState),
            "SAVE_STATE" => Ok(RetroArchCommand::SaveState),
            "FULLSCREEN_TOGGLE" => Ok(RetroArchCommand::FullscreenToggle),
            "QUIT" => Ok(RetroArchCommand::Quit),
            "STATE_SLOT_PLUS" => Ok(RetroArchCommand::StateSlotPlus),
            "STATE_SLOT_MINUS" => Ok(RetroArchCommand::StateSlotMinus),
            "REWIND" => Ok(RetroArchCommand::Rewind),
            "MOVIE_RECORD_TOGGLE" => Ok(RetroArchCommand::MovieRecordToggle),
            "PAUSE_TOGGLE" => Ok(RetroArchCommand::PauseToggle),
            "FRAMEADVANCE" => Ok(RetroArchCommand::FrameAdvance),
            "RESET" => Ok(RetroArchCommand::Reset),
            "SHADER_NEXT" => Ok(RetroArchCommand::ShaderNext),
            "SHADER_PREV" => Ok(RetroArchCommand::ShaderPrev),
            "CHEAT_INDEX_PLUS" => Ok(RetroArchCommand::CheatIndexPlus),
            "CHEAT_INDEX_MINUS" => Ok(RetroArchCommand::CheatIndexMinus),
            "CHEAT_TOGGLE" => Ok(RetroArchCommand::CheatToggle),
            "SCREENSHOT" => Ok(RetroArchCommand::Screenshot),
            "MUTE" => Ok(RetroArchCommand::Mute),
            "NETPLAY_FLIP" => Ok(RetroArchCommand::NetplayFlip),
            "SLOWMOTION" => Ok(RetroArchCommand::SlowMotion),
            "VOLUME_UP" => Ok(RetroArchCommand::VolumeUp),
            "VOLUME_DOWN" => Ok(RetroArchCommand::VolumeDown),
            "OVERLAY_NEXT" => Ok(RetroArchCommand::OverlayNext),
            "DISK_EJECT_TOGGLE" => Ok(RetroArchCommand::DiskEjectToggle),
            "DISK_NEXT" => Ok(RetroArchCommand::DiskNext),
            "DISK_PREV" => Ok(RetroArchCommand::DiskPrev),
            "GRAB_MOUSE_TOGGLE" => Ok(RetroArchCommand::GrabMouseToggle),
            "MENU_TOGGLE" => Ok(RetroArchCommand::MenuToggle),
            "PAUSE" => Ok(RetroArchCommand::Pause),
            "UNPAUSE" => Ok(RetroArchCommand::Unpause),
            "GET_INFO" => Ok(RetroArchCommand::GetInfo),
            "GET_DISK_COUNT" => Ok(RetroArchCommand::GetDiskCount),
            "GET_DISK_SLOT" => Ok(RetroArchCommand::GetDiskSlot),
            "SET_DISK_SLOT" => {
                let slot = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or("Invalid slot")?;
                Ok(RetroArchCommand::SetDiskSlot(slot))
            }
            "GET_STATE_SLOT" => Ok(RetroArchCommand::GetStateSlot),
            "SET_STATE_SLOT" => {
                let slot = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or("Invalid slot")?;
                Ok(RetroArchCommand::SetStateSlot(slot))
            }
            "SAVE_STATE_SLOT" => {
                let slot = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or("Invalid slot")?;
                Ok(RetroArchCommand::SaveStateSlot(slot))
            }
            "LOAD_STATE_SLOT" => {
                let slot = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or("Invalid slot")?;
                Ok(RetroArchCommand::LoadStateSlot(slot))
            }
            "SET_SCALE" => Ok(RetroArchCommand::SetScale(
                parts.get(1).unwrap_or(&"aspect").to_string(),
            )),
            "SET_EFFECT" => Ok(RetroArchCommand::SetEffect(
                parts.get(1).unwrap_or(&"none").to_string(),
            )),
            "SET_SHARPNESS" => Ok(RetroArchCommand::SetSharpness(
                parts.get(1).unwrap_or(&"soft").to_string(),
            )),
            "SET_TEARING" => Ok(RetroArchCommand::SetTearing(
                parts.get(1).unwrap_or(&"lenient").to_string(),
            )),
            "SET_OVERCLOCK" => Ok(RetroArchCommand::SetOverclock(
                parts.get(1).unwrap_or(&"normal").to_string(),
            )),
            "SET_THREAD_VIDEO" => {
                let enabled = parts
                    .get(1)
                    .map(|s| *s == "true" || *s == "1")
                    .unwrap_or(true);
                Ok(RetroArchCommand::SetThreadVideo(enabled))
            }
            "SET_DEBUG_HUD" => {
                let enabled = parts
                    .get(1)
                    .map(|s| *s == "true" || *s == "1")
                    .unwrap_or(true);
                Ok(RetroArchCommand::SetDebugHUD(enabled))
            }
            "SET_MAX_FF" => {
                let speed = parts
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or("Invalid speed")?;
                Ok(RetroArchCommand::SetMaxFF(speed))
            }
            "SET_CORE_OPTION" => {
                let key = parts.get(1).ok_or("Missing key")?.to_string();
                let value = parts.get(2).ok_or("Missing value")?.to_string();
                Ok(RetroArchCommand::SetCoreOption { key, value })
            }
            "RELOAD_CONFIG" => Ok(RetroArchCommand::ReloadConfig),
            "SET_CONTROL" => {
                let retro_button = parts.get(1).ok_or("Missing retro button")?.to_string();
                let key = parts.get(2).ok_or("Missing key")?.to_string();
                Ok(RetroArchCommand::SetControl { retro_button, key })
            }
            "SET_SHORTCUT" => {
                let action = parts.get(1).ok_or("Missing action")?.to_string();
                let combo = parts.get(2).ok_or("Missing combo")?.to_string();
                Ok(RetroArchCommand::SetShortcut { action, combo })
            }
            _ => Err(format!("Unknown command: {}", parts[0])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_commands() {
        assert!(matches!(
            RetroArchCommand::from_str("QUIT"),
            Ok(RetroArchCommand::Quit)
        ));
        assert!(matches!(
            RetroArchCommand::from_str("PAUSE"),
            Ok(RetroArchCommand::Pause)
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_STATE_SLOT 1"),
            Ok(RetroArchCommand::SetStateSlot(1))
        ));
    }

    #[test]
    fn test_parse_settings_commands() {
        assert!(matches!(
            RetroArchCommand::from_str("SET_SCALE native"),
            Ok(RetroArchCommand::SetScale(ref s)) if s == "native"
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_EFFECT grid"),
            Ok(RetroArchCommand::SetEffect(ref s)) if s == "grid"
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_SHARPNESS sharp"),
            Ok(RetroArchCommand::SetSharpness(ref s)) if s == "sharp"
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_TEARING strict"),
            Ok(RetroArchCommand::SetTearing(ref s)) if s == "strict"
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_OVERCLOCK performance"),
            Ok(RetroArchCommand::SetOverclock(ref s)) if s == "performance"
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_THREAD_VIDEO true"),
            Ok(RetroArchCommand::SetThreadVideo(true))
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_DEBUG_HUD false"),
            Ok(RetroArchCommand::SetDebugHUD(false))
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_MAX_FF 4"),
            Ok(RetroArchCommand::SetMaxFF(4))
        ));
        assert!(matches!(
            RetroArchCommand::from_str("SET_CORE_OPTION gambatte_gb_colorization internal"),
            Ok(RetroArchCommand::SetCoreOption { ref key, ref value }) if key == "gambatte_gb_colorization" && value == "internal"
        ));
        assert!(matches!(
            RetroArchCommand::from_str("RELOAD_CONFIG"),
            Ok(RetroArchCommand::ReloadConfig)
        ));
    }

    #[test]
    fn test_parse_invalid() {
        assert!(RetroArchCommand::from_str("UNKNOWN").is_err());
        assert!(RetroArchCommand::from_str("SET_STATE_SLOT abc").is_err());
    }
}
