mod battery;
mod evdev;
#[path = "../miyoo/framebuffer.rs"]
mod framebuffer;
mod screen;
mod volume;

use anyhow::Result;
use async_trait::async_trait;

use self::framebuffer::FramebufferDisplay;
use crate::display::settings::DisplaySettings;
use crate::platform::rg35xxsp::battery::Rg35xxSpBattery;
use crate::platform::rg35xxsp::evdev::Rg35xxSpKeys;
use crate::platform::{KeyEvent, Platform};

pub struct Rg35xxSpPlatform {
    keys: Rg35xxSpKeys,
}

pub struct SuspendContext {
    brightness: u8,
}

#[async_trait(?Send)]
impl Platform for Rg35xxSpPlatform {
    type Display = FramebufferDisplay;
    type Battery = Rg35xxSpBattery;
    type SuspendContext = SuspendContext;

    fn new() -> Result<Self> {
        Ok(Self {
            keys: Rg35xxSpKeys::new()?,
        })
    }

    fn display(&mut self) -> Result<Self::Display> {
        FramebufferDisplay::new()
    }

    fn battery(&self) -> Result<Self::Battery> {
        Ok(Rg35xxSpBattery::new())
    }

    async fn poll(&mut self) -> KeyEvent {
        self.keys.poll().await
    }

    fn shutdown(&self) -> Result<()> {
        std::process::Command::new("poweroff").spawn()?.wait()?;
        Ok(())
    }

    fn suspend(&self) -> Result<Self::SuspendContext> {
        let brightness = screen::get_brightness()?;
        screen::set_brightness(0)?;
        screen::blank(true)?;
        std::fs::write("/sys/power/state", "mem")?;
        Ok(SuspendContext { brightness })
    }

    fn unsuspend(&self, ctx: Self::SuspendContext) -> Result<()> {
        screen::blank(false)?;
        screen::set_brightness(ctx.brightness)
    }

    fn set_volume(&mut self, volume: i32) -> Result<()> {
        volume::set_volume(volume)
    }

    fn get_brightness(&self) -> Result<u8> {
        screen::get_brightness()
    }

    fn set_brightness(&mut self, brightness: u8) -> Result<()> {
        screen::set_brightness(brightness)
    }

    fn set_display_settings(&mut self, _settings: &mut DisplaySettings) -> Result<()> {
        Ok(())
    }

    fn device_model() -> String {
        "Anbernic RG35XXSP".to_string()
    }

    fn firmware() -> String {
        crate::constants::ALLIUM_VERSION.to_string()
    }

    fn has_wifi() -> bool {
        true
    }

    fn has_lid() -> bool {
        true
    }
}
