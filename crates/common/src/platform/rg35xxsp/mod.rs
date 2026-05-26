/*
 * rg35xxsp/mod.rs
 *
 * Implements the Platform trait for the Anbernic RG35xxSP handheld.
 * Uses generic Linux interfaces (/dev/fb0, /dev/input/event*, and sysfs)
 * to maintain high portability and absolute separation from core Allium.
 */

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use evdev::{Device, EventStream, EventType, KeyCode};
use framebuffer::Framebuffer;
use tiny_skia::{Pixmap, PixmapMut, PixmapRef};

use crate::battery::Battery;
use crate::display::Display;
use crate::display::color::Color;
use crate::display::settings::DisplaySettings;
use crate::geom::Rect;
use crate::platform::{KeyEvent, Platform, Key};

pub const SCREEN_WIDTH: u32 = 640;
pub const SCREEN_HEIGHT: u32 = 480;

pub struct Rg35xxspPlatform {
    display: Rg35xxspDisplay,
    battery: Rg35xxspBattery,
    inputs: Vec<EventStream>,
}

impl From<u16> for Key {
    fn from(code: u16) -> Self {
        match KeyCode(code) {
            KeyCode::KEY_UP => Key::Up,
            KeyCode::KEY_DOWN => Key::Down,
            KeyCode::KEY_LEFT => Key::Left,
            KeyCode::KEY_RIGHT => Key::Right,
            KeyCode::KEY_ENTER => Key::Start,
            KeyCode::KEY_RIGHTCTRL => Key::Select,
            KeyCode::KEY_SPACE => Key::A,
            KeyCode::KEY_LEFTCTRL => Key::B,
            KeyCode::KEY_LEFTSHIFT => Key::X,
            KeyCode::KEY_LEFTALT => Key::Y,
            KeyCode::KEY_E => Key::L,
            KeyCode::KEY_T => Key::R,
            KeyCode::KEY_TAB => Key::L2,
            KeyCode::KEY_BACKSPACE => Key::R2,
            KeyCode::KEY_ESC => Key::Menu,
            KeyCode::KEY_POWER => Key::Power,
            KeyCode::KEY_VOLUMEDOWN => Key::VolDown,
            KeyCode::KEY_VOLUMEUP => Key::VolUp,
            _ => Key::Unknown,
        }
    }
}

#[async_trait(?Send)]
impl Platform for Rg35xxspPlatform {
    type Display = Rg35xxspDisplay;
    type Battery = Rg35xxspBattery;
    type SuspendContext = ();

    fn new() -> Result<Self> {
        let display = Rg35xxspDisplay::new()?;
        let battery = Rg35xxspBattery::new()?;
        let mut inputs = Vec::new();

        // Scan all available event devices on standard Linux
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("event") {
                        if let Ok(dev) = Device::open(entry.path()) {
                            if let Ok(stream) = dev.into_event_stream() {
                                inputs.push(stream);
                            }
                        }
                    }
                }
            }
        }

        Ok(Self { display, battery, inputs })
    }

    fn display(&mut self) -> Result<Self::Display> {
        Ok(self.display.clone())
    }

    fn battery(&self) -> Result<Self::Battery> {
        Ok(self.battery.clone())
    }

    async fn poll(&mut self) -> KeyEvent {
        if self.inputs.is_empty() {
            return std::future::pending().await;
        }

        // Poll events from all opened input devices in a non-blocking select
        loop {
            for stream in &mut self.inputs {
                use futures::FutureExt;
                if let Some(Ok(event)) = stream.next_event().now_or_never() {
                    if event.event_type() == EventType::KEY {
                        let key: Key = event.code().into();
                        return match event.value() {
                            0 => KeyEvent::Released(key),
                            1 => KeyEvent::Pressed(key),
                            _ => KeyEvent::Autorepeat(key),
                        };
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    fn shutdown(&self) -> Result<()> {
        let _ = std::process::Command::new("poweroff").spawn();
        Ok(())
    }

    fn suspend(&self) -> Result<Self::SuspendContext> {
        Ok(())
    }

    fn unsuspend(&self, _ctx: Self::SuspendContext) -> Result<()> {
        Ok(())
    }

    fn set_volume(&mut self, volume: i32) -> Result<()> {
        let _ = std::process::Command::new("amixer")
            .arg("set")
            .arg("Master")
            .arg(format!("{}%", volume))
            .spawn();
        Ok(())
    }

    fn get_brightness(&self) -> Result<u8> {
        let path = Path::new("/sys/class/backlight/backlight/brightness");
        if !path.exists() {
            return Ok(100);
        }
        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let val: u8 = content.trim().parse().unwrap_or(100);
        Ok(val)
    }

    fn set_brightness(&mut self, brightness: u8) -> Result<()> {
        let path = Path::new("/sys/class/backlight/backlight/brightness");
        if path.exists() {
            fs::write(path, format!("{}", brightness))?;
        }
        Ok(())
    }

    fn set_display_settings(&mut self, _settings: &mut DisplaySettings) -> Result<()> {
        Ok(())
    }

    fn device_model() -> String {
        "RG35xxSP".into()
    }

    fn firmware() -> String {
        "Alpine-Allium".into()
    }

    fn has_wifi() -> bool {
        true
    }

    fn has_lid() -> bool {
        true
    }
}

impl Default for Rg35xxspPlatform {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

#[derive(Clone)]
pub struct Rg35xxspDisplay {
    pixmap: Pixmap,
}

impl Rg35xxspDisplay {
    pub fn new() -> Result<Self> {
        let pixmap = Pixmap::new(SCREEN_WIDTH, SCREEN_HEIGHT)
            .ok_or_else(|| anyhow!("Failed to allocate display pixmap"))?;
        Ok(Self { pixmap })
    }
}

impl Display for Rg35xxspDisplay {
    fn width(&self) -> u32 {
        SCREEN_WIDTH
    }

    fn height(&self) -> u32 {
        SCREEN_HEIGHT
    }

    fn pixmap(&self) -> PixmapRef<'_> {
        self.pixmap.as_ref()
    }

    fn pixmap_mut(&mut self) -> PixmapMut<'_> {
        self.pixmap.as_mut()
    }

    fn map_pixels<F>(&mut self, mut f: F) -> Result<()>
    where
        F: FnMut(Color) -> Color,
    {
        for pixel in self.pixmap.pixels_mut() {
            let color: Color = (*pixel).into();
            *pixel = f(color).into();
        }
        Ok(())
    }

    fn sync(&mut self) -> Result<()> {
        // Direct landscape write to framebuffer /dev/fb0 (Phase 1)
        if let Ok(mut fb) = Framebuffer::new("/dev/fb0") {
            let bytes_per_pixel = (fb.var_screen_info.bits_per_pixel / 8) as usize;
            let mut fb_frame = vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize * bytes_per_pixel];
            for (i, pixel) in self.pixmap.pixels().iter().enumerate() {
                let offset = i * bytes_per_pixel;
                let c: Color = (*pixel).into();
                if bytes_per_pixel == 4 {
                    // ARGB/RGBA format
                    fb_frame[offset] = c.b();
                    fb_frame[offset + 1] = c.g();
                    fb_frame[offset + 2] = c.r();
                    fb_frame[offset + 3] = c.a();
                } else if bytes_per_pixel == 2 {
                    // RGB565 format
                    let r5 = (c.r() >> 3) as u16;
                    let g6 = (c.g() >> 2) as u16;
                    let b5 = (c.b() >> 3) as u16;
                    let rgb565 = (r5 << 11) | (g6 << 5) | b5;
                    fb_frame[offset] = (rgb565 & 0xFF) as u8;
                    fb_frame[offset + 1] = ((rgb565 >> 8) & 0xFF) as u8;
                }
            }
            let _ = fb.write_frame(&fb_frame);
        }
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        Ok(())
    }

    fn load(&mut self, _area: Rect) -> Result<()> {
        Ok(())
    }

    fn pop(&mut self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct Rg35xxspBattery {
    capacity_path: Option<PathBuf>,
    status_path: Option<PathBuf>,
}

impl Rg35xxspBattery {
    pub fn new() -> Result<Self> {
        let mut capacity_path = None;
        let mut status_path = None;

        // Auto-configure the battery sysfs directory dynamically
        if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let cap = entry.path().join("capacity");
                let stat = entry.path().join("status");
                if cap.exists() && stat.exists() {
                    capacity_path = Some(cap);
                    status_path = Some(stat);
                    break;
                }
            }
        }

        Ok(Self { capacity_path, status_path })
    }
}

impl Battery for Rg35xxspBattery {
    fn update(&mut self) -> Result<()> {
        Ok(())
    }

    fn percentage(&self) -> i32 {
        if let Some(ref path) = self.capacity_path {
            if let Ok(content) = fs::read_to_string(path) {
                return content.trim().parse().unwrap_or(50);
            }
        }
        50
    }

    fn charging(&self) -> bool {
        if let Some(ref path) = self.status_path {
            if let Ok(content) = fs::read_to_string(path) {
                return content.trim().eq_ignore_ascii_case("charging")
                    || content.trim().eq_ignore_ascii_case("full");
            }
        }
        false
    }
}
