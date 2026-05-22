use crate::control::ControlEvent;
use anyhow::Result;
use common::constants::RETROARCH_UDP_SOCKET;
use common::retroarch::RetroArchCommand;
use log::{debug, warn};
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicI8, Ordering},
};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::UnboundedSender;

pub struct CommandState {
    state_slot: AtomicI8,
}

impl CommandState {
    pub fn new(state_slot: i8) -> Arc<Self> {
        Arc::new(Self {
            state_slot: AtomicI8::new(state_slot),
        })
    }

    pub fn set_state_slot(&self, state_slot: i8) {
        self.state_slot.store(state_slot, Ordering::Relaxed);
    }

    fn state_slot(&self) -> i8 {
        self.state_slot.load(Ordering::Relaxed)
    }
}

pub async fn run_command_server(
    tx: UnboundedSender<ControlEvent>,
    state: Arc<CommandState>,
) -> Result<()> {
    let socket = UdpSocket::bind(RETROARCH_UDP_SOCKET).await?;
    let mut buf = [0u8; 256];
    debug!("Play UDP command server bound at {}", RETROARCH_UDP_SOCKET);
    while process_next_datagram(&socket, &mut buf, &tx, &state).await? {}
    Ok(())
}

fn parse_udp_command(raw: &str) -> Option<RetroArchCommand> {
    match RetroArchCommand::from_str(raw.trim()) {
        Ok(command) => Some(command),
        Err(err) => {
            warn!("Ignoring invalid UDP command {:?}: {}", raw, err);
            None
        }
    }
}

async fn process_next_datagram(
    socket: &UdpSocket,
    buf: &mut [u8; 256],
    tx: &UnboundedSender<ControlEvent>,
    state: &CommandState,
) -> Result<bool> {
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

fn reply_for_command(command: &RetroArchCommand, state: &CommandState) -> Option<String> {
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

    #[test]
    fn get_info_reply_matches_menu_parser_shape() {
        let state = CommandState::new(-1);

        assert_eq!(
            reply_for_command(&RetroArchCommand::GetInfo, &state),
            Some("GET_INFO 0 0 -1".to_string())
        );
    }
}
