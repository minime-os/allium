use libretro::*;
use common::platform::{Key, KeyEvent};
use std::os::raw::c_uint;

const JOYPAD_BUTTONS: usize = 16;

pub struct JoypadState {
    pressed: [bool; JOYPAD_BUTTONS],
}

impl JoypadState {
    pub fn new() -> Self {
        Self {
            pressed: [false; JOYPAD_BUTTONS],
        }
    }

    pub fn apply(&mut self, event: KeyEvent) {
        let (key, pressed) = match event {
            KeyEvent::Pressed(key) | KeyEvent::Autorepeat(key) => (key, true),
            KeyEvent::Released(key) => (key, false),
        };
        let Some(id) = joypad_id_for_key(key) else {
            return;
        };
        let Some(button) = self.pressed.get_mut(id as usize) else {
            return;
        };

        *button = pressed;
    }

    pub fn input_state(&self, port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16 {
        if port != 0 || index != 0 || device & RETRO_DEVICE_MASK != RETRO_DEVICE_JOYPAD {
            return 0;
        }

        self.pressed
            .get(id as usize)
            .copied()
            .map(i16::from)
            .unwrap_or(0)
    }
}

pub fn joypad_id_for_key(key: Key) -> Option<c_uint> {
    match key {
        Key::B => Some(RETRO_DEVICE_ID_JOYPAD_B),
        Key::Y => Some(RETRO_DEVICE_ID_JOYPAD_Y),
        Key::Select => Some(RETRO_DEVICE_ID_JOYPAD_SELECT),
        Key::Start => Some(RETRO_DEVICE_ID_JOYPAD_START),
        Key::Up => Some(RETRO_DEVICE_ID_JOYPAD_UP),
        Key::Down => Some(RETRO_DEVICE_ID_JOYPAD_DOWN),
        Key::Left => Some(RETRO_DEVICE_ID_JOYPAD_LEFT),
        Key::Right => Some(RETRO_DEVICE_ID_JOYPAD_RIGHT),
        Key::A => Some(RETRO_DEVICE_ID_JOYPAD_A),
        Key::X => Some(RETRO_DEVICE_ID_JOYPAD_X),
        Key::L => Some(RETRO_DEVICE_ID_JOYPAD_L),
        Key::R => Some(RETRO_DEVICE_ID_JOYPAD_R),
        Key::L2 => Some(RETRO_DEVICE_ID_JOYPAD_L2),
        Key::R2 => Some(RETRO_DEVICE_ID_JOYPAD_R2),
        Key::Menu | Key::Power | Key::VolDown | Key::VolUp | Key::LidClose | Key::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_game_keys_to_libretro_joypad_ids() {
        assert_eq!(joypad_id_for_key(Key::B), Some(RETRO_DEVICE_ID_JOYPAD_B));
        assert_eq!(joypad_id_for_key(Key::Y), Some(RETRO_DEVICE_ID_JOYPAD_Y));
        assert_eq!(
            joypad_id_for_key(Key::Select),
            Some(RETRO_DEVICE_ID_JOYPAD_SELECT)
        );
        assert_eq!(
            joypad_id_for_key(Key::Start),
            Some(RETRO_DEVICE_ID_JOYPAD_START)
        );
        assert_eq!(joypad_id_for_key(Key::Up), Some(RETRO_DEVICE_ID_JOYPAD_UP));
        assert_eq!(
            joypad_id_for_key(Key::Down),
            Some(RETRO_DEVICE_ID_JOYPAD_DOWN)
        );
        assert_eq!(
            joypad_id_for_key(Key::Left),
            Some(RETRO_DEVICE_ID_JOYPAD_LEFT)
        );
        assert_eq!(
            joypad_id_for_key(Key::Right),
            Some(RETRO_DEVICE_ID_JOYPAD_RIGHT)
        );
        assert_eq!(joypad_id_for_key(Key::A), Some(RETRO_DEVICE_ID_JOYPAD_A));
        assert_eq!(joypad_id_for_key(Key::X), Some(RETRO_DEVICE_ID_JOYPAD_X));
        assert_eq!(joypad_id_for_key(Key::L), Some(RETRO_DEVICE_ID_JOYPAD_L));
        assert_eq!(joypad_id_for_key(Key::R), Some(RETRO_DEVICE_ID_JOYPAD_R));
        assert_eq!(joypad_id_for_key(Key::L2), Some(RETRO_DEVICE_ID_JOYPAD_L2));
        assert_eq!(joypad_id_for_key(Key::R2), Some(RETRO_DEVICE_ID_JOYPAD_R2));
    }

    #[test]
    fn leaves_system_keys_unmapped() {
        assert_eq!(joypad_id_for_key(Key::Menu), None);
        assert_eq!(joypad_id_for_key(Key::Power), None);
        assert_eq!(joypad_id_for_key(Key::VolDown), None);
        assert_eq!(joypad_id_for_key(Key::VolUp), None);
        assert_eq!(joypad_id_for_key(Key::LidClose), None);
        assert_eq!(joypad_id_for_key(Key::Unknown), None);
    }

    #[test]
    fn key_events_update_joypad_state() {
        let mut state = JoypadState::new();

        state.apply(KeyEvent::Pressed(Key::A));
        assert_eq!(
            state.input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_A),
            1
        );

        state.apply(KeyEvent::Released(Key::A));
        assert_eq!(
            state.input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_A),
            0
        );
    }

    #[test]
    fn autorepeat_keeps_key_pressed() {
        let mut state = JoypadState::new();

        state.apply(KeyEvent::Autorepeat(Key::Start));

        assert_eq!(
            state.input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_START),
            1
        );
    }

    #[test]
    fn ignores_non_player_one_joypad_queries() {
        let mut state = JoypadState::new();
        state.apply(KeyEvent::Pressed(Key::A));

        assert_eq!(
            state.input_state(1, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_A),
            0
        );
        assert_eq!(
            state.input_state(0, RETRO_DEVICE_JOYPAD, 1, RETRO_DEVICE_ID_JOYPAD_A),
            0
        );
        assert_eq!(
            state.input_state(0, RETRO_DEVICE_ID_JOYPAD_MASK, 0, RETRO_DEVICE_ID_JOYPAD_A),
            0
        );
    }
}
