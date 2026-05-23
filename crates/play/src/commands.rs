// Defines runtime execution control events for the Play emulator.
// Includes UDP command server and RetroArch command translation.

use log::{debug, warn};
use std::sync::Arc;

#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        self.state_slot.store(state_slot, std::sync::atomic::Ordering::Relaxed);
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
    debug!("Play UDP command server bound at {}", common::constants::RETROARCH_UDP_SOCKET);
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
    let Some(cmd) = parse_udp_command(&raw) else { return Ok(true); };
    if let Some(reply) = reply_for_command(&cmd, state) {
        socket.send_to(reply.as_bytes(), peer).await?;
    } else if let Some(ev) = ControlEvent::from_retroarch_command(cmd) {
        return Ok(tx.send(ev).is_ok());
    }
    Ok(true)
}

pub fn reply_for_command(command: &common::retroarch::RetroArchCommand, state: &CommandState) -> Option<String> {
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
    fn get_info_reply_matches_menu_parser_shape() {
        let state = CommandState::new(-1);
        assert_eq!(
            reply_for_command(&RetroArchCommand::GetInfo, &state),
            Some("GET_INFO 0 0 -1".to_string())
        );
    }
}
