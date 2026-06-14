#[cfg(feature = "miyoo")]
pub mod miyoo;
#[cfg(feature = "minime")]
pub mod minime;
#[cfg(feature = "simulator")]
pub mod simulator;

#[cfg(not(any(feature = "miyoo", feature = "minime", feature = "simulator")))]
mod mock;

use anyhow::Result;
use async_trait::async_trait;
use enum_map::Enum;
use serde::{Deserialize, Serialize};

use crate::{
    battery::Battery,
    display::{Display, settings::DisplaySettings},
};

#[cfg(feature = "miyoo")]
pub type DefaultPlatform = miyoo::MiyooPlatform;

#[cfg(feature = "minime")]
pub type DefaultPlatform = minime::MinimePlatform;

#[cfg(feature = "simulator")]
pub type DefaultPlatform = simulator::SimulatorPlatform;

#[cfg(not(any(feature = "miyoo", feature = "minime", feature = "simulator")))]
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

    fn try_poll(&mut self) -> Option<KeyEvent> {
        None
    }

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
        let s = match self {
            Key::Up => "Up",
            Key::Down => "Down",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::A => "A",
            Key::B => "B",
            Key::X => "X",
            Key::Y => "Y",
            Key::Start => "Start",
            Key::Select => "Select",
            Key::L => "L",
            Key::R => "R",
            Key::Menu => "Menu",
            Key::L2 => "L2",
            Key::R2 => "R2",
            Key::Power => "Power",
            Key::VolDown => "VolDown",
            Key::VolUp => "VolUp",
            Key::LidClose => "LidClose",
            Key::Unknown => "Unknown",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for Key {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Up" => Ok(Key::Up),
            "Down" => Ok(Key::Down),
            "Left" => Ok(Key::Left),
            "Right" => Ok(Key::Right),
            "A" => Ok(Key::A),
            "B" => Ok(Key::B),
            "X" => Ok(Key::X),
            "Y" => Ok(Key::Y),
            "Start" => Ok(Key::Start),
            "Select" => Ok(Key::Select),
            "L" => Ok(Key::L),
            "R" => Ok(Key::R),
            "Menu" => Ok(Key::Menu),
            "L2" => Ok(Key::L2),
            "R2" => Ok(Key::R2),
            "Power" => Ok(Key::Power),
            "VolDown" => Ok(Key::VolDown),
            "VolUp" => Ok(Key::VolUp),
            "LidClose" => Ok(Key::LidClose),
            "Unknown" => Ok(Key::Unknown),
            _ => Err(format!("Unknown key: {}", s)),
        }
    }
}
