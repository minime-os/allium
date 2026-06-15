// Minime platform bootstrapper coordinating video, audio, and physical input.

pub mod audio;
pub mod stats;
pub mod video;

use crate::commands::ControlEvent;
use crate::input::JoypadState;
use crate::video::ScaleMode;
use anyhow::Result;
use audio::MinimeAudio;
use common::platform::minime::Traits;
use evdev::{Device, EventStream, EventType};
use video::MinimeVideo;

pub struct MinimePlatform {
    pub video: MinimeVideo,
    _audio: MinimeAudio,
    inputs: Vec<EventStream>,
    traits: Traits,
    stats: stats::MinimeStats,
    signal: Option<tokio::signal::unix::Signal>,
}

impl MinimePlatform {
    pub fn new(
        _core_id: &str,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
        sample_rate: u32,
        audio_consumer: crate::audio::AudioConsumer,
    ) -> Result<Self> {
        set_governor("performance");

        let traits = Traits::load()?;
        let video = MinimeVideo::new(&traits, source_width, source_height, aspect_ratio, scale)?;
        let mut inputs = Vec::new();

        for entry in std::fs::read_dir("/dev/input")? {
            let path = entry?.path();
            if !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("event"))
            {
                continue;
            }
            let device = Device::open(path)?;
            if device.name().is_some_and(|name| {
                traits
                    .input_device_names
                    .iter()
                    .any(|expected| expected == name)
            }) {
                inputs.push(device.into_event_stream()?);
            }
        }

        let _audio = MinimeAudio::new(sample_rate, audio_consumer)?;
        let stats = stats::MinimeStats::new();
        let signal = None;

        Ok(Self {
            video,
            _audio,
            inputs,
            traits,
            stats,
            signal,
        })
    }

    pub fn poll_input(&mut self, joypad: &mut JoypadState) -> Vec<ControlEvent> {
        use futures::FutureExt;
        for stream in &mut self.inputs {
            while let Some(result) = stream.next_event().now_or_never() {
                let event = match result {
                    Ok(ev) => ev,
                    Err(err) => {
                        log::warn!("Evdev event read error: {}", err);
                        continue;
                    }
                };
                if event.event_type() == EventType::KEY {
                    let Some(&key) = self.traits.keycodes.get(&event.code()) else {
                        continue;
                    };
                    if event.timestamp().elapsed().unwrap() > common::constants::MAXIMUM_FRAME_TIME
                    {
                        continue;
                    }
                    joypad.apply(match event.value() {
                        0 => common::platform::KeyEvent::Released(key),
                        1 => common::platform::KeyEvent::Pressed(key),
                        _ => common::platform::KeyEvent::Autorepeat(key),
                    });
                }
            }
        }
        Vec::new()
    }

    pub fn cpu_usage(&mut self) -> Option<f64> {
        self.stats.cpu_usage()
    }

    pub fn skip_presentation_when_paused(&self) -> bool {
        true
    }

    pub async fn wait_for_shutdown(&mut self) {
        let signal = self.signal.get_or_insert_with(|| {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
        });
        signal.recv().await;
    }
}

impl Drop for MinimePlatform {
    fn drop(&mut self) {
        set_governor("ondemand");
    }
}

pub fn init_logging() -> Result<()> {
    use common::constants::ALLIUM_PLAY_LOG;
    use log::LevelFilter;
    use simple_logger::SimpleLogger;
    std::fs::create_dir_all(common::constants::ALLIUM_BASE_DIR.join("logs"))?;
    let _ = common::log::init_hardware_log(&ALLIUM_PLAY_LOG);
    println!(
        "--- Play starting at {} ---",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    SimpleLogger::new().with_level(LevelFilter::Info).init()?;
    Ok(())
}

pub fn set_governor(governor: &str) {
    let path = "/sys/devices/system/cpu/cpufreq/policy0/scaling_governor";
    if !std::path::Path::new(path).exists() {
        return;
    }
    if let Err(err) = std::fs::write(path, governor) {
        log::warn!("Failed to set CPU governor to {}: {}", governor, err);
    } else {
        log::info!("Successfully set CPU governor to {}", governor);
    }
}
