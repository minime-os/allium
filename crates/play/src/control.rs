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
