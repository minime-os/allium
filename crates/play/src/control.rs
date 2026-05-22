//! Defines and manages runtime execution control events for the Play emulator.
//! This includes mapping raw input/IPC commands (like RetroArch command signals)
//! and applying state, playback, or system mutations directly to the active `PlaySession`.

// File Flow:
// 1. Definition of the `ControlEvent` enum representing all emulator operations.
// 2. Conversion helper `from_retroarch_command` from RetroArch command signals.
// 3. Dispatch function `apply` routing events to specific state categories.
// 4. Specialized application logic:
//    - `apply_state` (save-states, slot changes).
//    - `apply_playback` (pause, fast-forward control).
//    - `apply_system` (reset, quit, scaling cycle).
// 5. Utility helper `set_audio_muted` to pause/resume audio during fast-forwards.
// 6. Unit tests verifying command translation correctness.

use crate::session::PlaySession;
use crate::save;
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
    /// Translates external command signals (received over UDP/IPC) into type-safe internal control events.
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

    /// Dispatches the control event to the specialized category-specific handler.
    pub fn apply(&self, session: &mut PlaySession) -> Result<()> {
        match *self {
            Self::SaveState
            | Self::LoadState
            | Self::SaveStateSlot(_)
            | Self::LoadStateSlot(_)
            | Self::SelectStateSlot(_)
            | Self::StateSlotPlus
            | Self::StateSlotMinus => self.apply_state(session),
            Self::SetPaused(_)
            | Self::TogglePaused
            | Self::ToggleFastForward
            | Self::SetFastForward(_) => self.apply_playback(session),
            Self::Reset | Self::Quit | Self::CycleScale => self.apply_system(session),
        }
    }

    /// Handles save-state and slot selection mutations on the active session.
    fn apply_state(&self, session: &mut PlaySession) -> Result<()> {
        match *self {
            Self::SaveState => core_save(session, session.state_slot),
            Self::LoadState => core_load(session, session.state_slot),
            Self::SaveStateSlot(slot) => {
                session.select_state_slot(slot)?;
                core_save(session, slot)
            }
            Self::LoadStateSlot(slot) => {
                session.select_state_slot(slot)?;
                core_load(session, slot)
            }
            Self::SelectStateSlot(slot) => session.select_state_slot(slot),
            Self::StateSlotPlus => session.select_state_slot((session.state_slot + 1).min(9)),
            Self::StateSlotMinus => session.select_state_slot((session.state_slot - 1).max(-1)),
            _ => Ok(()),
        }
    }

    /// Handles play-state timing adjustments (pauses and fast-forwards) on the active session.
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

    /// Handles hardware/OS lifecycle transitions (hard resets, shutdowns, screen scaling) on the active session.
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

fn core_save(session: &mut PlaySession, slot: i8) -> Result<()> {
    let core = session.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
    save::save_state_slot(core, &session.paths, slot)
}

fn core_load(session: &mut PlaySession, slot: i8) -> Result<()> {
    let core = session.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
    save::load_state_slot(core, &session.paths, slot)
}

/// Mutes/unmutes the audio driver's consumer output channel.
/// This prevents high-pitched, fast, or distorted sound output when fast-forwarding emulation frames.
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
