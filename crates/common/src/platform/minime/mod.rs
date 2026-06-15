use std::collections::HashMap;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use evdev::{Device, EventStream, EventType};
use framebuffer::Framebuffer;
use futures::future::select_all;
use tiny_skia::{Pixmap, PixmapMut, PixmapRef};

use crate::battery::Battery;
use crate::display::Display;
use crate::display::color::Color;
use crate::display::settings::DisplaySettings;
use crate::geom::Rect;
use crate::platform::{Key, KeyEvent, Platform};

const TRAITS_PATH: &str = "/mnt/sdcard/.minime/traits";

#[derive(Debug, PartialEq, Eq)]
pub struct Traits {
    pub device_model: String,
    pub video_device: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_rotation: u32,
    backlight_path: Option<PathBuf>,
    framebuffer_blank_path: Option<PathBuf>,
    battery_capacity_path: Option<PathBuf>,
    charger_online_path: Option<PathBuf>,
    sound_card: Option<String>,
    sound_mixer: Option<String>,
    wifi_interface: Option<String>,
    lid_switch_path: Option<PathBuf>,
    pub input_device_names: Vec<String>,
    pub keycodes: HashMap<u16, Key>,
}

impl Traits {
    pub fn load() -> Result<Self> {
        Self::parse(&fs::read_to_string(TRAITS_PATH).context("failed to read Minime traits")?)
    }

    fn parse(input: &str) -> Result<Self> {
        let values = parse_values(input);
        Ok(Self {
            device_model: required(&values, "device_model")?.to_owned(),
            video_device: required(&values, "video_device")?.to_owned(),
            screen_width: parse_number(&values, "screen_width")?,
            screen_height: parse_number(&values, "screen_height")?,
            screen_rotation: parse_number(&values, "screen_rotation")?,
            backlight_path: optional_path(&values, "backlight_path"),
            framebuffer_blank_path: optional_path(&values, "framebuffer_blank_path"),
            battery_capacity_path: optional_path(&values, "battery_capacity_path"),
            charger_online_path: optional_path(&values, "charger_online_path"),
            sound_card: optional(&values, "sound_card"),
            sound_mixer: optional(&values, "sound_mixer"),
            wifi_interface: optional(&values, "wifi_interface"),
            lid_switch_path: optional_path(&values, "lid_switch_path"),
            input_device_names: parse_input_names(&values),
            keycodes: parse_keycodes(&values)?,
        })
    }
}

fn parse_values(input: &str) -> HashMap<&str, &str> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

fn required<'a>(values: &'a HashMap<&str, &str>, key: &str) -> Result<&'a str> {
    values
        .get(key)
        .copied()
        .filter(|value| !value.is_empty() && *value != "na")
        .ok_or_else(|| anyhow!("missing required trait: {key}"))
}

fn optional(values: &HashMap<&str, &str>, key: &str) -> Option<String> {
    values
        .get(key)
        .filter(|value| !value.is_empty() && **value != "na")
        .map(|value| (*value).to_owned())
}

fn optional_path(values: &HashMap<&str, &str>, key: &str) -> Option<PathBuf> {
    optional(values, key).map(PathBuf::from)
}

fn parse_number<T>(values: &HashMap<&str, &str>, key: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    required(values, key)?
        .parse()
        .with_context(|| format!("invalid trait: {key}"))
}

fn parse_input_names(values: &HashMap<&str, &str>) -> Vec<String> {
    [
        "input_gamepad_device_name",
        "input_power_device_name",
        "input_volume_device_name",
    ]
    .iter()
    .filter_map(|key| optional(values, key))
    .collect()
}

fn parse_keycodes(values: &HashMap<&str, &str>) -> Result<HashMap<u16, Key>> {
    let mut keycodes = HashMap::new();
    for (name, key) in KEYS {
        if let Some(value) = values
            .get(name)
            .filter(|value| !value.is_empty() && **value != "na")
        {
            keycodes.insert(
                value
                    .parse()
                    .with_context(|| format!("invalid trait: {name}"))?,
                *key,
            );
        }
    }
    Ok(keycodes)
}

const KEYS: &[(&str, Key)] = &[
    ("key_a", Key::A),
    ("key_b", Key::B),
    ("key_c", Key::C),
    ("key_x", Key::X),
    ("key_y", Key::Y),
    ("key_z", Key::Z),
    ("key_up", Key::Up),
    ("key_down", Key::Down),
    ("key_left", Key::Left),
    ("key_right", Key::Right),
    ("key_start", Key::Start),
    ("key_select", Key::Select),
    ("key_l1", Key::L),
    ("key_r1", Key::R),
    ("key_l2", Key::L2),
    ("key_r2", Key::R2),
    ("key_menu", Key::Menu),
    ("key_power", Key::Power),
    ("key_vol_down", Key::VolDown),
    ("key_vol_up", Key::VolUp),
];

pub struct MinimeBattery {
    capacity_path: Option<PathBuf>,
    online_path: Option<PathBuf>,
    charging: bool,
    percentage: i32,
}

impl MinimeBattery {
    fn new(capacity_path: Option<PathBuf>, online_path: Option<PathBuf>) -> Self {
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
        if let Some(path) = self.capacity_path.as_deref() {
            self.percentage = read_number(&power_supply_file(path, "capacity"))?;
            let status = fs::read_to_string(power_supply_file(path, "status")).unwrap_or_default();
            self.charging = matches!(status.trim(), "Charging" | "Full");
        }
        if let Some(path) = self.online_path.as_deref() {
            self.charging = read_number::<i32>(path)? != 0;
        }
        Ok(())
    }

    fn percentage(&self) -> i32 {
        self.percentage
    }

    fn charging(&self) -> bool {
        self.charging
    }
}

fn power_supply_file(path: &Path, name: &str) -> PathBuf {
    if path.is_dir() {
        path.join(name)
    } else {
        path.to_owned()
    }
}

fn read_number<T>(path: &Path) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    fs::read_to_string(path)?
        .trim()
        .parse()
        .with_context(|| format!("invalid number in {}", path.display()))
}

pub struct MinimeDisplay {
    pixmap: Pixmap,
    framebuffer: Framebuffer,
    rotation: u32,
    saved: Vec<Pixmap>,
}

impl MinimeDisplay {
    fn new(traits: &Traits) -> Result<Self> {
        let framebuffer = Framebuffer::new(&traits.video_device)?;
        if framebuffer.var_screen_info.bits_per_pixel != 32 {
            bail!(
                "unsupported framebuffer depth: {}",
                framebuffer.var_screen_info.bits_per_pixel
            );
        }
        let pixmap = Pixmap::new(traits.screen_width, traits.screen_height)
            .ok_or_else(|| anyhow!("failed to create display pixmap"))?;
        Ok(Self {
            pixmap,
            framebuffer,
            rotation: traits.screen_rotation,
            saved: Vec::new(),
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

    fn map_pixels<F>(&mut self, mut f: F) -> Result<()>
    where
        F: FnMut(Color) -> Color,
    {
        for pixel in self.pixmap.pixels_mut() {
            *pixel = f((*pixel).into()).into();
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        let physical_width = self.framebuffer.var_screen_info.xres as usize;
        let physical_height = self.framebuffer.var_screen_info.yres as usize;
        for y in 0..self.height() as usize {
            for x in 0..self.width() as usize {
                let (frame_x, frame_y) =
                    rotate(x, y, physical_width, physical_height, self.rotation);
                let frame_index = (frame_y * physical_width + frame_x) * 4;
                let pixel = self.pixmap.pixels()[y * self.width() as usize + x];
                self.framebuffer.frame[frame_index..frame_index + 4].copy_from_slice(&[
                    pixel.blue(),
                    pixel.green(),
                    pixel.red(),
                    pixel.alpha(),
                ]);
            }
        }
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        self.saved.push(self.pixmap.clone());
        Ok(())
    }

    fn load(&mut self, rect: Rect) -> Result<()> {
        let saved = self.saved.last().context("no saved image")?;
        for y in rect.y.max(0) as usize..(rect.y.max(0) as u32 + rect.h).min(self.height()) as usize
        {
            for x in
                rect.x.max(0) as usize..(rect.x.max(0) as u32 + rect.w).min(self.width()) as usize
            {
                let index = y * self.width() as usize + x;
                self.pixmap.pixels_mut()[index] = saved.pixels()[index];
            }
        }
        Ok(())
    }

    fn pop(&mut self) -> bool {
        self.saved.pop();
        !self.saved.is_empty()
    }
}

fn rotate(x: usize, y: usize, width: usize, height: usize, rotation: u32) -> (usize, usize) {
    match rotation {
        90 => (width - y - 1, x),
        180 => (width - x - 1, height - y - 1),
        270 => (y, height - x - 1),
        _ => (x, y),
    }
}

pub struct SuspendContext {
    brightness: u8,
}

pub struct MinimePlatform {
    traits: Traits,
    inputs: Vec<EventStream>,
}

impl MinimePlatform {
    fn open_inputs(traits: &Traits) -> Result<Vec<EventStream>> {
        traits
            .input_device_names
            .iter()
            .map(|name| open_input(name))
            .collect()
    }
}

fn open_input(expected_name: &str) -> Result<EventStream> {
    for entry in fs::read_dir("/dev/input").context("failed to read /dev/input")? {
        let path = entry?.path();
        if !path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("event"))
        {
            continue;
        }
        let device = Device::open(&path)?;
        if device.name() == Some(expected_name) {
            return device.into_event_stream().map_err(Into::into);
        }
    }
    bail!("input device not found: {expected_name}")
}

#[async_trait(?Send)]
impl Platform for MinimePlatform {
    type Display = MinimeDisplay;
    type Battery = Box<dyn Battery>;
    type SuspendContext = SuspendContext;

    fn new() -> Result<Self> {
        let traits = Traits::load()?;
        let inputs = Self::open_inputs(&traits)?;
        Ok(Self { traits, inputs })
    }

    fn display(&mut self) -> Result<Self::Display> {
        MinimeDisplay::new(&self.traits)
    }

    fn battery(&self) -> Result<Self::Battery> {
        Ok(Box::new(MinimeBattery::new(
            self.traits.battery_capacity_path.clone(),
            self.traits.charger_online_path.clone(),
        )))
    }

    async fn poll(&mut self) -> KeyEvent {
        loop {
            let events = self
                .inputs
                .iter_mut()
                .map(|input| Box::pin(input.next_event()));
            let (event, _, _) = select_all(events).await;
            if let Ok(event) = event
                && event.event_type() == EventType::KEY
                && let Some(key) = self.traits.keycodes.get(&event.code())
            {
                return match event.value() {
                    0 => KeyEvent::Released(*key),
                    1 => KeyEvent::Pressed(*key),
                    2 => KeyEvent::Autorepeat(*key),
                    _ => continue,
                };
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn shutdown(&self) -> Result<()> {
        require_success(Command::new("sync").status()?, "sync")?;
        let error = Command::new("poweroff").exec();
        Err(error.into())
    }

    fn suspend(&self) -> Result<Self::SuspendContext> {
        let brightness = self.get_brightness()?;
        write_optional(self.traits.backlight_path.as_deref(), "0")?;
        write_optional(self.traits.framebuffer_blank_path.as_deref(), "4")?;
        Ok(SuspendContext { brightness })
    }

    fn unsuspend(&self, context: Self::SuspendContext) -> Result<()> {
        write_optional(self.traits.framebuffer_blank_path.as_deref(), "0")?;
        write_optional(
            self.traits.backlight_path.as_deref(),
            &context.brightness.to_string(),
        )
    }

    fn set_volume(&mut self, volume: i32) -> Result<()> {
        let (Some(card), Some(mixer)) = (&self.traits.sound_card, &self.traits.sound_mixer) else {
            return Ok(());
        };
        let status = Command::new("amixer")
            .args([
                "-q",
                "-D",
                card,
                "sset",
                mixer,
                &format!("{}%", volume.clamp(0, 20) * 5),
            ])
            .status()?;
        require_success(status, "amixer")
    }

    fn get_brightness(&self) -> Result<u8> {
        self.traits
            .backlight_path
            .as_deref()
            .map(read_number)
            .transpose()?
            .map_or(Ok(0), Ok)
    }

    fn set_brightness(&mut self, brightness: u8) -> Result<()> {
        write_optional(
            self.traits.backlight_path.as_deref(),
            &brightness.to_string(),
        )
    }

    fn set_display_settings(&mut self, _settings: &mut DisplaySettings) -> Result<()> {
        Ok(())
    }

    fn device_model() -> String {
        Traits::load()
            .map(|traits| traits.device_model)
            .unwrap_or_else(|_| "Minime".to_owned())
    }

    fn firmware() -> String {
        fs::read_to_string("/etc/minime-version")
            .map(|version| version.trim().to_owned())
            .unwrap_or_else(|_| "Minime".to_owned())
    }

    fn has_wifi() -> bool {
        Traits::load().is_ok_and(|traits| traits.wifi_interface.is_some())
    }

    fn has_lid() -> bool {
        Traits::load().is_ok_and(|traits| traits.lid_switch_path.is_some())
    }
}

fn write_optional(path: Option<&Path>, value: &str) -> Result<()> {
    if let Some(path) = path {
        fs::write(path, value).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn require_success(status: std::process::ExitStatus, command: &str) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        bail!("{command} exited with {status}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARC_D_TRAITS: &str = "\
device_model=Anbernic RG ARC-D
video_device=/dev/fb0
screen_width=640
screen_height=480
screen_rotation=90
key_a=305
key_c=306
key_z=309
";

    #[test]
    fn parses_required_traits_and_six_button_keys() {
        let traits = Traits::parse(ARC_D_TRAITS).unwrap();
        assert_eq!(traits.device_model, "Anbernic RG ARC-D");
        assert_eq!(traits.video_device, "/dev/fb0");
        assert_eq!(traits.screen_width, 640);
        assert_eq!(traits.screen_height, 480);
        assert_eq!(traits.screen_rotation, 90);
        assert_eq!(traits.keycodes.get(&305), Some(&Key::A));
        assert_eq!(traits.keycodes.get(&306), Some(&Key::C));
        assert_eq!(traits.keycodes.get(&309), Some(&Key::Z));
    }

    #[test]
    fn rejects_missing_required_traits() {
        let error = Traits::parse("device_model=Anbernic RG ARC-D\n").unwrap_err();
        assert_eq!(error.to_string(), "missing required trait: video_device");
    }
}
