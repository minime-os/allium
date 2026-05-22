//! Defines runtime execution control events for the Play emulator.
//! Maps raw input/IPC commands (like RetroArch command signals) into type-safe events.

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
    /// Translates external command signals into type-safe internal control events.
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
