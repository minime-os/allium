use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use framebuffer::Framebuffer;
use log::{debug, info, trace, warn};
use tiny_skia::{Pixmap, PixmapMut, PixmapRef};

use crate::{
    battery::Battery,
    display::{Display, settings::DisplaySettings},
    geom::Rect,
    display::color::Color,
    platform::{Key, KeyEvent, Platform},
};

#[derive(Debug, Clone)]
pub struct Traits {
    pub device_model: String,
    pub button_layout: String,
    pub video_backend: String,
    pub video_device: String,
    pub video_pixel_format: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_aspect: String,
    pub screen_refresh: u32,
    pub screen_rotation: u32,
    pub has_wifi: bool,
    pub has_bluetooth: bool,
    pub has_hdmi: bool,
    pub has_lid: bool,
    pub has_sticks: bool,
    pub has_touch: bool,
    pub battery_capacity_path: String,
    pub charger_online_path: String,
    pub backlight_path: String,
    pub lid_switch_path: String,
    pub rumble_path: String,
    pub power_led_path: String,
    pub sound_card: String,
    pub sound_mixer: String,
    pub input_gamepad_device_name: String,
    pub input_power_device_name: String,
    pub input_volume_device_name: String,
    pub keycodes: HashMap<u16, Key>,
}

impl Traits {
    pub fn load() -> Result<Self> {
        let file = File::open("/mnt/sdcard/.minime/traits")?;
        let reader = BufReader::new(file);
        let mut map = HashMap::new();
        let mut keycodes = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim().to_string();
                let v = v.trim().to_string();
                map.insert(k, v);
            }
        }

        let get = |k: &str, default: &str| map.get(k).cloned().unwrap_or_else(|| default.to_string());
        let get_bool = |k: &str| map.get(k).map(|v| v == "yes").unwrap_or(false);
        let get_int = |k: &str, default: u32| map.get(k).and_then(|v| v.parse().ok()).unwrap_or(default);

        let keys_to_map = [
            ("key_up", Key::Up),
            ("key_down", Key::Down),
            ("key_left", Key::Left),
            ("key_right", Key::Right),
            ("key_a", Key::A),
            ("key_b", Key::B),
            ("key_x", Key::X),
            ("key_y", Key::Y),
            ("key_start", Key::Start),
            ("key_select", Key::Select),
            ("key_menu", Key::Menu),
            ("key_l1", Key::L),
            ("key_r1", Key::R),
            ("key_l2", Key::L2),
            ("key_r2", Key::R2),
            ("key_power", Key::Power),
            ("key_vol_up", Key::VolUp),
            ("key_vol_down", Key::VolDown),
            ("key_lid_close", Key::LidClose),
        ];

        for (trait_key, logical_key) in keys_to_map {
            if let Some(val_str) = map.get(trait_key) {
                if let Ok(code) = val_str.parse::<u16>() {
                    keycodes.insert(code, logical_key);
                }
            }
        }

        if keycodes.is_empty() {
            keycodes.insert(544, Key::Up);
            keycodes.insert(545, Key::Down);
            keycodes.insert(546, Key::Left);
            keycodes.insert(547, Key::Right);
            keycodes.insert(305, Key::A);
            keycodes.insert(304, Key::B);
            keycodes.insert(308, Key::X);
            keycodes.insert(307, Key::Y);
            keycodes.insert(315, Key::Start);
            keycodes.insert(314, Key::Select);
            keycodes.insert(316, Key::Menu);
            keycodes.insert(310, Key::L);
            keycodes.insert(311, Key::R);
            keycodes.insert(312, Key::L2);
            keycodes.insert(313, Key::R2);
            keycodes.insert(116, Key::Power);
            keycodes.insert(115, Key::VolUp);
            keycodes.insert(114, Key::VolDown);
        }

        Ok(Traits {
            device_model: get("device_model", "Minime Device"),
            button_layout: get("button_layout", "nintendo"),
            video_backend: get("video_backend", "framebuffer"),
            video_device: get("video_device", "/dev/fb0"),
            video_pixel_format: get("video_pixel_format", "BGRA8888"),
            screen_width: get_int("screen_width", 640),
            screen_height: get_int("screen_height", 480),
            screen_aspect: get("screen_aspect", "4:3"),
            screen_refresh: get_int("screen_refresh", 60),
            screen_rotation: get_int("screen_rotation", 0),
            has_wifi: get_bool("has_wifi"),
            has_bluetooth: get_bool("has_bluetooth"),
            has_hdmi: get_bool("has_hdmi"),
            has_lid: get_bool("has_lid"),
            has_sticks: get_bool("has_sticks"),
            has_touch: get_bool("has_touch"),
            battery_capacity_path: get("battery_capacity_path", ""),
            charger_online_path: get("charger_online_path", ""),
            backlight_path: get("backlight_path", "/sys/class/backlight/backlight/brightness"),
            lid_switch_path: get("lid_switch_path", ""),
            rumble_path: get("rumble_path", ""),
            power_led_path: get("power_led_path", ""),
            sound_card: get("sound_card", "default"),
            sound_mixer: get("sound_mixer", "lineout volume"),
            input_gamepad_device_name: get("input_gamepad_device_name", "gpio-keys-gamepad"),
            input_power_device_name: get("input_power_device_name", "axp20x-pek"),
            input_volume_device_name: get("input_volume_device_name", "gpio-keys-volume"),
            keycodes,
        })
    }
}

fn open_input_by_name(expected: &str) -> Option<evdev::Device> {
    for i in 0..10 {
        let path = format!("/dev/input/event{}", i);
        if let Ok(dev) = evdev::Device::open(&path) {
            if let Some(name) = dev.name() {
                if name.contains(expected) {
                    return Some(dev);
                }
            }
        }
    }
    None
}

pub struct MinimeBattery {
    capacity_path: String,
    online_path: String,
    charging: bool,
    percentage: i32,
}

impl MinimeBattery {
    pub fn new(capacity_path: String, online_path: String) -> Self {
        Self {
            capacity_path,
            online_path,
            charging: false,
            percentage: 100,
        }
    }
}

impl Battery for MinimeBattery {
    fn update(&mut self) -> Result<()> {
        if !self.capacity_path.is_empty() && Path::new(&self.capacity_path).exists() {
            let cap_str = std::fs::read_to_string(&self.capacity_path)?;
            if let Ok(pct) = cap_str.trim().parse::<i32>() {
                self.percentage = pct;
            }
        }

        if !self.online_path.is_empty() && Path::new(&self.online_path).exists() {
            let online_str = std::fs::read_to_string(&self.online_path)?;
            if let Ok(online) = online_str.trim().parse::<i32>() {
                self.charging = online != 0;
            }
        } else if !self.capacity_path.is_empty() {
            let status_path = self.capacity_path.replace("/capacity", "/status");
            if Path::new(&status_path).exists() {
                if let Ok(status) = std::fs::read_to_string(status_path) {
                    let status = status.trim();
                    self.charging = status == "Charging" || status == "Full";
                }
            }
        }
        Ok(())
    }

    fn percentage(&self) -> i32 {
        self.percentage
    }

    fn charging(&self) -> bool {
        self.charging
    }

    fn update_led(_enabled: bool) {}
}

pub struct MinimeDisplay {
    pixmap: Pixmap,
    iface: Framebuffer,
    rotation: u32,
}

impl MinimeDisplay {
    pub fn new(device: &str, rotation: u32) -> Result<Self> {
        let iface = Framebuffer::new(device)?;
        let width = iface.var_screen_info.xres;
        let height = iface.var_screen_info.yres;

        let (log_w, log_h) = if rotation == 90 || rotation == 270 {
            (height, width)
        } else {
            (width, height)
        };

        let pixmap = Pixmap::new(log_w, log_h)
            .ok_or_else(|| anyhow!("Failed to create pixmap {}x{}", log_w, log_h))?;

        Ok(Self {
            pixmap,
            iface,
            rotation,
        })
    }
}

impl Display for MinimeDisplay {
    fn width(&self) -> u32 {
        self.pixmap.width()
    }

    fn height(&self) -> u32 {
        self.pixmap.height()
    }

    fn pixmap(&self) -> PixmapRef<'_> {
        self.pixmap.as_ref()
    }

    fn pixmap_mut(&mut self) -> PixmapMut<'_> {
        self.pixmap.as_mut()
    }

    fn sync(&mut self) -> Result<()> {
        Ok(())
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

    fn flush(&mut self) -> Result<()> {
        let (xoffset, yoffset) = (
            self.iface.var_screen_info.xoffset as usize,
            self.iface.var_screen_info.yoffset as usize,
        );
        let log_w = self.width() as usize;
        let log_h = self.height() as usize;
        
        let phys_w = self.iface.var_screen_info.xres as usize;
        let phys_h = self.iface.var_screen_info.yres as usize;
        
        let bytes_per_pixel = (self.iface.var_screen_info.bits_per_pixel / 8) as usize;
        let location = (yoffset * phys_w + xoffset) * bytes_per_pixel;

        let background = self.iface.read_frame();

        for y in 0..log_h {
            for x in 0..log_w {
                let idx = y * log_w + x;
                let pixel = self.pixmap.pixels()[idx];

                let (fb_x, fb_y) = match self.rotation {
                    90 => (phys_w - y - 1, x),
                    180 => (phys_w - x - 1, phys_h - y - 1),
                    270 => (y, phys_h - x - 1),
                    _ => (x, y),
                };

                let fb_idx = location + (fb_y * phys_w + fb_x) * bytes_per_pixel;

                self.iface.frame[fb_idx] = pixel.blue();
                self.iface.frame[fb_idx + 1] = pixel.green();
                self.iface.frame[fb_idx + 2] = pixel.red();
                self.iface.frame[fb_idx + 3] = pixel.alpha();
            }
        }

        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        Ok(())
    }

    fn load(&mut self, _rect: Rect) -> Result<()> {
        Ok(())
    }

    fn pop(&mut self) -> bool {
        false
    }
}

pub struct MinimePlatform {
    traits: Traits,
    _gamepad: Option<evdev::Device>,
    _power: Option<evdev::Device>,
    _volume: Option<evdev::Device>,
    gamepad_stream: Option<evdev::EventStream>,
    power_stream: Option<evdev::EventStream>,
    volume_stream: Option<evdev::EventStream>,
    last_lid_state: bool,
}

impl MinimePlatform {
    pub fn new() -> Result<Self> {
        let traits = Traits::load()?;

        let gamepad = open_input_by_name(&traits.input_gamepad_device_name);
        let power = open_input_by_name(&traits.input_power_device_name);
        let volume = open_input_by_name(&traits.input_volume_device_name);

        let gamepad_stream = gamepad.as_ref().and_then(|d| d.clone().into_event_stream().ok());
        let power_stream = power.as_ref().and_then(|d| d.clone().into_event_stream().ok());
        let volume_stream = volume.as_ref().and_then(|d| d.clone().into_event_stream().ok());

        let mut platform = Self {
            traits,
            _gamepad: gamepad,
            _power: power,
            _volume: volume,
            gamepad_stream,
            power_stream,
            volume_stream,
            last_lid_state: true, // assume open by default
        };

        if platform.traits.has_lid && !platform.traits.lid_switch_path.is_empty() {
            if let Ok(state_str) = std::fs::read_to_string(&platform.traits.lid_switch_path) {
                platform.last_lid_state = state_str.trim() == "1";
            }
        }

        Ok(platform)
    }
}

#[async_trait(?Send)]
impl Platform for MinimePlatform {
    type Display = MinimeDisplay;
    type Battery = Box<dyn Battery>;
    type SuspendContext = u8;

    fn new() -> Result<Self> {
        Self::new()
    }

    fn display(&mut self) -> Result<Self::Display> {
        MinimeDisplay::new(&self.traits.video_device, self.traits.screen_rotation)
    }

    fn battery(&self) -> Result<Self::Battery> {
        Ok(Box::new(MinimeBattery::new(
            self.traits.battery_capacity_path.clone(),
            self.traits.charger_online_path.clone(),
        )))
    }

    async fn poll(&mut self) -> KeyEvent {
        loop {
            // Check lid switch first
            if self.traits.has_lid && !self.traits.lid_switch_path.is_empty() {
                if let Ok(state_str) = std::fs::read_to_string(&self.traits.lid_switch_path) {
                    let is_open = state_str.trim() == "1";
                    if is_open != self.last_lid_state {
                        self.last_lid_state = is_open;
                        if is_open {
                            return KeyEvent::Released(Key::LidClose);
                        } else {
                            return KeyEvent::Pressed(Key::LidClose);
                        }
                    }
                }
            }

            let ev = tokio::select! {
                Some(Ok(event)) = async {
                    if let Some(stream) = &mut self.gamepad_stream {
                        stream.next_event().await.ok()
                    } else {
                        None
                    }
                } => event,
                Some(Ok(event)) = async {
                    if let Some(stream) = &mut self.power_stream {
                        stream.next_event().await.ok()
                    } else {
                        None
                    }
                } => event,
                Some(Ok(event)) = async {
                    if let Some(stream) = &mut self.volume_stream {
                        stream.next_event().await.ok()
                    } else {
                        None
                    }
                } => event,
                else => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
            };

            if ev.event_type() == evdev::EventType::KEY {
                let code = ev.code();
                if let Some(&logical_key) = self.traits.keycodes.get(&code) {
                    return match ev.value() {
                        0 => KeyEvent::Released(logical_key),
                        1 => KeyEvent::Pressed(logical_key),
                        2 => KeyEvent::Autorepeat(logical_key),
                        _ => continue,
                    };
                }
            }
        }
    }

    fn shutdown(&self) -> Result<()> {
        let _ = Command::new("sync").status();
        let _ = Command::new("poweroff").exec();
        Ok(())
    }

    fn suspend(&self) -> Result<Self::SuspendContext> {
        let br = self.get_brightness()?;
        let _ = self.set_brightness(0);
        if Path::new("/sys/class/graphics/fb0/blank").exists() {
            let _ = std::fs::write("/sys/class/graphics/fb0/blank", b"4");
        }
        Ok(br)
    }

    fn unsuspend(&self, ctx: Self::SuspendContext) -> Result<()> {
        if Path::new("/sys/class/graphics/fb0/blank").exists() {
            let _ = std::fs::write("/sys/class/graphics/fb0/blank", b"0");
        }
        let _ = self.set_brightness(ctx);
        Ok(())
    }

    fn set_volume(&mut self, volume: i32) -> Result<()> {
        let volume = volume.clamp(0, 20);
        let val = volume * 5;
        let card = &self.traits.sound_card;
        let mixer = &self.traits.sound_mixer;
        let _ = Command::new("amixer")
            .arg("-q")
            .arg("-c")
            .arg(card)
            .arg("sset")
            .arg(mixer)
            .arg(format!("{}%", val))
            .status();
        Ok(())
    }

    fn get_brightness(&self) -> Result<u8> {
        if Path::new(&self.traits.backlight_path).exists() {
            let s = std::fs::read_to_string(&self.traits.backlight_path)?;
            if let Ok(val) = s.trim().parse::<u8>() {
                return Ok(val);
            }
        }
        Ok(100)
    }

    fn set_brightness(&mut self, brightness: u8) -> Result<()> {
        if Path::new(&self.traits.backlight_path).exists() {
            let _ = std::fs::write(&self.traits.backlight_path, format!("{}", brightness));
        }
        Ok(())
    }

    fn set_display_settings(&mut self, _settings: &mut DisplaySettings) -> Result<()> {
        Ok(())
    }

    fn device_model() -> String {
        if let Ok(t) = Traits::load() {
            t.device_model
        } else {
            "Minime Device".to_string()
        }
    }

    fn firmware() -> String {
        "Minime OS".to_string()
    }

    fn has_wifi() -> bool {
        Traits::load().map(|t| t.has_wifi).unwrap_or(false)
    }

    fn has_lid() -> bool {
        Traits::load().map(|t| t.has_lid).unwrap_or(false)
    }
}
