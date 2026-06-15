#[cfg(not(feature = "minime"))]
mod mock;

#[cfg(feature = "minime")]
pub mod minime;

use anyhow::Result;
use async_trait::async_trait;
use enum_map::Enum;
use serde::{Deserialize, Serialize};

use crate::{
    battery::Battery,
    display::{Display, settings::DisplaySettings},
};

#[cfg(feature = "minime")]
pub type DefaultPlatform = minime::MinimePlatform;

#[cfg(not(feature = "minime"))]
pub type DefaultPlatform = mock::MockPlatform;

// Platform is not threadsafe because it is ?Send
#[async_trait(?Send)]
pub trait Platform {
    type Display: Display;
    type Battery: Battery + 'static;
    type SuspendContext;

    fn new() -> Result<Self>
    where
        Self: Sized;

    fn display(&mut self) -> Result<Self::Display>;

    fn battery(&self) -> Result<Self::Battery>;

    async fn poll(&mut self) -> KeyEvent;

    fn shutdown(&self) -> Result<()>;

    fn suspend(&self) -> Result<Self::SuspendContext>;

    fn unsuspend(&self, ctx: Self::SuspendContext) -> Result<()>;

    fn set_volume(&mut self, volume: i32) -> Result<()>;

    fn get_brightness(&self) -> Result<u8>;

    fn set_brightness(&mut self, brightness: u8) -> Result<()>;

    fn set_display_settings(&mut self, settings: &mut DisplaySettings) -> Result<()>;

    fn device_model() -> String;

    fn firmware() -> String;

    fn has_wifi() -> bool;

    fn has_lid() -> bool;

    fn daemon(&self) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Pressed(Key),
    Released(Key),
    Autorepeat(Key),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Enum)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    C,
    Z,
    Start,
    Select,
    L,
    R,
    Menu,
    L2,
    R2,
    Power,
    VolDown,
    VolUp,
    LidClose,
    Unknown,
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for Key {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        [
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::A,
            Key::B,
            Key::C,
            Key::X,
            Key::Y,
            Key::Z,
            Key::Start,
            Key::Select,
            Key::L,
            Key::R,
            Key::Menu,
            Key::L2,
            Key::R2,
            Key::Power,
            Key::VolDown,
            Key::VolUp,
            Key::LidClose,
            Key::Unknown,
        ]
        .into_iter()
        .find(|key| key.to_string() == value)
        .ok_or_else(|| format!("Unknown key: {value}"))
    }
}
