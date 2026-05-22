use crate::session::PlaySession;
use anyhow::{Result, anyhow};
use log::info;

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

    pub fn from_retroarch_command(command: common::retroarch::RetroArchCommand) -> Option<Self> {
        use common::retroarch::RetroArchCommand;

        match command {
            RetroArchCommand::SaveState => Some(Self::SaveState),
            RetroArchCommand::LoadState => Some(Self::LoadState),
            RetroArchCommand::SaveStateSlot(slot) => Some(Self::SaveStateSlot(slot)),
            RetroArchCommand::LoadStateSlot(slot) => Some(Self::LoadStateSlot(slot)),
            RetroArchCommand::SetStateSlot(slot) => Some(Self::SelectStateSlot(slot)),
            RetroArchCommand::StateSlotPlus => Some(Self::StateSlotPlus),
            RetroArchCommand::StateSlotMinus => Some(Self::StateSlotMinus),
            RetroArchCommand::Pause => Some(Self::SetPaused(true)),
            RetroArchCommand::Unpause => Some(Self::SetPaused(false)),
            RetroArchCommand::PauseToggle => Some(Self::TogglePaused),
            RetroArchCommand::FastForward => Some(Self::ToggleFastForward),
            RetroArchCommand::FastForwardHold => Some(Self::SetFastForward(true)),
            RetroArchCommand::Reset => Some(Self::Reset),
            RetroArchCommand::Quit => Some(Self::Quit),
            RetroArchCommand::ShaderNext => Some(Self::CycleScale),
            _ => None,
        }
    }

    pub fn apply(&self, session: &mut PlaySession) -> Result<()> {
        match *self {
            Self::SaveState | Self::LoadState | Self::SaveStateSlot(_) | Self::LoadStateSlot(_) | Self::SelectStateSlot(_) | Self::StateSlotPlus | Self::StateSlotMinus => {
                self.apply_state(session)
            }
            Self::SetPaused(_) | Self::TogglePaused | Self::ToggleFastForward | Self::SetFastForward(_) => {
                self.apply_playback(session)
            }
            Self::Reset | Self::Quit | Self::CycleScale => {
                self.apply_system(session)
            }
        }
    }

    fn apply_state(&self, session: &mut PlaySession) -> Result<()> {
        match *self {
            Self::SaveState => session.save_state(),
            Self::LoadState => session.load_state(),
            Self::SaveStateSlot(slot) => {
                session.select_state_slot(slot)?;
                session.save_state()
            }
            Self::LoadStateSlot(slot) => {
                session.select_state_slot(slot)?;
                session.load_state()
            }
            Self::SelectStateSlot(slot) => session.select_state_slot(slot),
            Self::StateSlotPlus => session.select_state_slot((session.state_slot + 1).min(9)),
            Self::StateSlotMinus => session.select_state_slot((session.state_slot - 1).max(-1)),
            _ => Ok(()),
        }
    }

    fn apply_playback(&self, session: &mut PlaySession) -> Result<()> {
        match *self {
            Self::SetPaused(paused) => session.paused = paused,
            Self::TogglePaused => session.paused = !session.paused,
            Self::ToggleFastForward => {
                session.fast_forwarding = !session.fast_forwarding;
                set_audio_muted(session, session.fast_forwarding);
            }
            Self::SetFastForward(enabled) => {
                session.fast_forwarding = enabled;
                set_audio_muted(session, enabled);
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_system(&self, session: &mut PlaySession) -> Result<()> {
        match *self {
            Self::Reset => {
                let core = session.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
                core.reset();
            }
            Self::Quit => session.should_quit = true,
            Self::CycleScale => {
                session.scale_mode = session.scale_mode.next();
                info!("Selected scale mode: {:?}", session.scale_mode);
            }
            _ => {}
        }
        Ok(())
    }
}

fn set_audio_muted(session: &mut PlaySession, muted: bool) {
    if let Some(producer) = &mut session.audio_producer {
        producer.set_muted(muted);
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
}
