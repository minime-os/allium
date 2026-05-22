// Miyoo platform bootstrapper coordinating video, audio, and physical input.

pub mod audio;
pub mod env;
pub mod stats;
pub mod video;

use crate::control::ControlEvent;
use crate::input::JoypadState;
use crate::platform::{EmulationPlatform, HostStats, InputBackend};
use crate::video::ScaleMode;
use anyhow::Result;
use audio::MiyooAudio;
use common::platform::{DefaultPlatform as CommonPlatform, Platform};
use env::MiyooSystemGuard;
use video::MiyooVideo;

pub struct MiyooPlatform {
    video: MiyooVideo,
    audio: MiyooAudio,
    input: MiyooInput,
    stats: stats::MiyooStats,
    signal: Option<tokio::signal::unix::Signal>,
    _guard: MiyooSystemGuard,
}

pub struct MiyooInput {
    platform: CommonPlatform,
}

impl InputBackend for MiyooInput {
    fn poll(&mut self, joypad: &mut JoypadState) -> Vec<ControlEvent> {
        while let Some(key_event) = self.platform.try_poll() {
            joypad.apply(key_event);
        }
        Vec::new()
    }
}

impl EmulationPlatform for MiyooPlatform {
    type Video = MiyooVideo;
    type Audio = MiyooAudio;
    type Input = MiyooInput;

    fn initialize(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
        sample_rate: u32,
        audio_consumer: crate::audio::AudioConsumer,
    ) -> Result<Self> {
        let _guard = MiyooSystemGuard::new(&get_core_id_from_args());
        let video = MiyooVideo::new(source_width, source_height, aspect_ratio, scale)?;
        let platform = CommonPlatform::new()?;
        let input = MiyooInput { platform };
        let audio = MiyooAudio::new(sample_rate, audio_consumer)?;
        let stats = stats::MiyooStats::new();
        let signal = None;
        Ok(Self {
            video,
            audio,
            input,
            stats,
            signal,
            _guard,
        })
    }

    fn video(&mut self) -> &mut Self::Video {
        &mut self.video
    }

    fn audio(&mut self) -> &mut Self::Audio {
        &mut self.audio
    }

    fn input(&mut self) -> &mut Self::Input {
        &mut self.input
    }

    fn stats(&mut self) -> &mut dyn HostStats {
        &mut self.stats
    }

    fn skip_presentation_when_paused(&self) -> bool {
        true
    }

    async fn wait_for_shutdown(&mut self) {
        let signal = self.signal.get_or_insert_with(|| {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
        });
        signal.recv().await;
    }
}

pub fn init_logging() -> Result<()> {
    use std::fs;
    use log::LevelFilter;
    use simple_logger::SimpleLogger;
    use common::constants::ALLIUM_PLAY_LOG;

    let _ = fs::write("/mnt/SDCARD/.allium/logs/play_started.marker", "started");
    let _ = common::log::init_hardware_log(&*ALLIUM_PLAY_LOG);
    println!("--- Play starting at {} ---", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));

    SimpleLogger::new().with_level(LevelFilter::Info).init()?;
    Ok(())
}

fn get_core_id_from_args() -> String {
    std::env::args()
        .position(|arg| arg == "--core")
        .and_then(|pos| std::env::args().nth(pos + 1))
        .and_then(|path| {
            std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_default()
}
