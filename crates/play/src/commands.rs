// Defines runtime execution control events for the Play emulator.
// Includes UDP command server and RetroArch command translation.

use log::{debug, warn};
use std::sync::Arc;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlEvent {
    SaveState,
    LoadState,
    SaveStateSlot(i8),
    LoadStateSlot(i8),
    SelectStateSlot(i8),
    StateSlotPlus,
    StateSlotMinus,
    SetPaused(bool),
    TogglePaused,
    ToggleFastForward,
    SetFastForward(bool),
    Reset,
    Quit,
    CycleScale,
    // Settings events (S1)
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
    SaveConfigConsole,
    SaveConfigGame,
    RestoreDefaults,
    SetControl { retro_button: String, key: String },
    SetShortcut { action: String, combo: String },
}

impl ControlEvent {
    /// Translates external RetroArch UDP commands into type-safe internal control events.
    pub fn from_retroarch_command(command: common::retroarch::RetroArchCommand) -> Option<Self> {
        use common::retroarch::RetroArchCommand as C;
        match command {
            C::SaveState => Some(Self::SaveState),
            C::LoadState => Some(Self::LoadState),
            C::SaveStateSlot(slot) => Some(Self::SaveStateSlot(slot)),
            C::LoadStateSlot(slot) => Some(Self::LoadStateSlot(slot)),
            C::SetStateSlot(slot) => Some(Self::SelectStateSlot(slot)),
            C::StateSlotPlus => Some(Self::StateSlotPlus),
            C::StateSlotMinus => Some(Self::StateSlotMinus),
            C::Pause => Some(Self::SetPaused(true)),
            C::Unpause => Some(Self::SetPaused(false)),
            C::PauseToggle => Some(Self::TogglePaused),
            C::FastForward => Some(Self::ToggleFastForward),
            C::FastForwardHold => Some(Self::SetFastForward(true)),
            C::Reset => Some(Self::Reset),
            C::Quit => Some(Self::Quit),
            C::ShaderNext => Some(Self::CycleScale),
            C::SetScale(mode) => Some(Self::SetScale(mode)),
            C::SetEffect(mode) => Some(Self::SetEffect(mode)),
            C::SetSharpness(mode) => Some(Self::SetSharpness(mode)),
            C::SetTearing(mode) => Some(Self::SetTearing(mode)),
            C::SetOverclock(mode) => Some(Self::SetOverclock(mode)),
            C::SetThreadVideo(enabled) => Some(Self::SetThreadVideo(enabled)),
            C::SetDebugHUD(enabled) => Some(Self::SetDebugHUD(enabled)),
            C::SetMaxFF(speed) => Some(Self::SetMaxFF(speed)),
            C::SetCoreOption { key, value } => Some(Self::SetCoreOption { key, value }),
            C::ReloadConfig => Some(Self::ReloadConfig),
            C::SaveConfigConsole => Some(Self::SaveConfigConsole),
            C::SaveConfigGame => Some(Self::SaveConfigGame),
            C::RestoreDefaults => Some(Self::RestoreDefaults),
            C::SetControl { retro_button, key } => Some(Self::SetControl { retro_button, key }),
            C::SetShortcut { action, combo } => Some(Self::SetShortcut { action, combo }),
            _ => None,
        }
    }
}

// UDP command state shared between the session and the async command server.
pub struct CommandState {
    pub(crate) state_slot: std::sync::atomic::AtomicI8,
}

impl CommandState {
    pub fn new(state_slot: i8) -> Arc<Self> {
        Arc::new(Self {
            state_slot: std::sync::atomic::AtomicI8::new(state_slot),
        })
    }

    pub fn set_state_slot(&self, state_slot: i8) {
        self.state_slot
            .store(state_slot, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn state_slot(&self) -> i8 {
        self.state_slot.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ---- UDP command server ----

pub async fn run_command_server(
    tx: tokio::sync::mpsc::UnboundedSender<ControlEvent>,
    state: Arc<CommandState>,
) -> anyhow::Result<()> {
    let socket = tokio::net::UdpSocket::bind(common::constants::RETROARCH_UDP_SOCKET).await?;
    let mut buf = [0u8; 256];
    debug!(
        "Play UDP command server bound at {}",
        common::constants::RETROARCH_UDP_SOCKET
    );
    while process_next_datagram(&socket, &mut buf, &tx, &state).await? {}
    Ok(())
}

fn parse_udp_command(raw: &str) -> Option<common::retroarch::RetroArchCommand> {
    match std::str::FromStr::from_str(raw.trim()) {
        Ok(command) => Some(command),
        Err(err) => {
            warn!("Ignoring invalid UDP command {:?}: {}", raw, err);
            None
        }
    }
}

async fn process_next_datagram(
    socket: &tokio::net::UdpSocket,
    buf: &mut [u8; 256],
    tx: &tokio::sync::mpsc::UnboundedSender<ControlEvent>,
    state: &CommandState,
) -> anyhow::Result<bool> {
    let (len, peer) = socket.recv_from(buf).await?;
    let raw = String::from_utf8_lossy(&buf[..len]);
    let Some(cmd) = parse_udp_command(&raw) else {
        return Ok(true);
    };
    if let Some(reply) = reply_for_command(&cmd, state) {
        socket.send_to(reply.as_bytes(), peer).await?;
    } else if let Some(ev) = ControlEvent::from_retroarch_command(cmd) {
        return Ok(tx.send(ev).is_ok());
    }
    Ok(true)
}

pub fn reply_for_command(
    command: &common::retroarch::RetroArchCommand,
    state: &CommandState,
) -> Option<String> {
    use common::retroarch::RetroArchCommand;
    match command {
        RetroArchCommand::GetInfo => Some(format!("GET_INFO 0 0 {}", state.state_slot())),
        RetroArchCommand::GetDiskCount => Some("GET_DISK_COUNT 0".to_string()),
        RetroArchCommand::GetDiskSlot => Some("GET_DISK_SLOT 0".to_string()),
        RetroArchCommand::GetStateSlot => Some(format!("GET_STATE_SLOT {}", state.state_slot())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::retroarch::RetroArchCommand;

    #[test]
    fn maps_menu_commands_to_control_events() {
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::Pause),
            Some(ControlEvent::SetPaused(true))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::Unpause),
            Some(ControlEvent::SetPaused(false))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::FastForward),
            Some(ControlEvent::ToggleFastForward)
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SaveStateSlot(-1)),
            Some(ControlEvent::SaveStateSlot(-1))
        );
    }

    #[test]
    fn maps_settings_commands_to_control_events() {
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetScale("native".to_string())),
            Some(ControlEvent::SetScale("native".to_string()))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetEffect("grid".to_string())),
            Some(ControlEvent::SetEffect("grid".to_string()))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetSharpness(
                "sharp".to_string()
            )),
            Some(ControlEvent::SetSharpness("sharp".to_string()))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetTearing(
                "strict".to_string()
            )),
            Some(ControlEvent::SetTearing("strict".to_string()))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetOverclock(
                "performance".to_string()
            )),
            Some(ControlEvent::SetOverclock("performance".to_string()))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetThreadVideo(true)),
            Some(ControlEvent::SetThreadVideo(true))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetDebugHUD(false)),
            Some(ControlEvent::SetDebugHUD(false))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetMaxFF(4)),
            Some(ControlEvent::SetMaxFF(4))
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SetCoreOption {
                key: "gambatte_gb_colorization".to_string(),
                value: "internal".to_string(),
            }),
            Some(ControlEvent::SetCoreOption {
                key: "gambatte_gb_colorization".to_string(),
                value: "internal".to_string(),
            })
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::ReloadConfig),
            Some(ControlEvent::ReloadConfig)
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SaveConfigConsole),
            Some(ControlEvent::SaveConfigConsole)
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::SaveConfigGame),
            Some(ControlEvent::SaveConfigGame)
        );
        assert_eq!(
            ControlEvent::from_retroarch_command(RetroArchCommand::RestoreDefaults),
            Some(ControlEvent::RestoreDefaults)
        );
    }

    #[test]
    fn get_info_reply_matches_menu_parser_shape() {
        let state = CommandState::new(-1);
        assert_eq!(
            reply_for_command(&RetroArchCommand::GetInfo, &state),
            Some("GET_INFO 0 0 -1".to_string())
        );
    }
}
