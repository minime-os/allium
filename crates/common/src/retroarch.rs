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
    fn test_parse_invalid() {
        assert!(RetroArchCommand::from_str("UNKNOWN").is_err());
        assert!(RetroArchCommand::from_str("SET_STATE_SLOT abc").is_err());
    }
}
