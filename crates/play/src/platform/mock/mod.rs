// Headless mock platform bootstrapper coordinating video/audio/input components in headless test runs.
#![allow(dead_code)]

pub mod audio;
pub mod stats;
pub mod video;

use crate::audio::AudioConsumer;
use crate::commands::ControlEvent;
use crate::input::JoypadState;
use crate::video::ScaleMode;
use anyhow::Result;
use audio::MockAudio;
use video::MockVideo;

pub struct MockPlatform {
    pub video: MockVideo,
    _audio: MockAudio,
    _input: MockInput,
}

pub struct MockInput;

impl MockPlatform {
    pub fn new(
        _core_id: &str,
        _source_width: u32,
        _source_height: u32,
        _aspect_ratio: f32,
        _scale: ScaleMode,
        _sample_rate: u32,
        _audio_consumer: AudioConsumer,
    ) -> Result<Self> {
        let video = MockVideo::new();
        let _audio = MockAudio::new();
        let _input = MockInput;
        Ok(Self {
            video,
            _audio,
            _input,
        })
    }

    pub fn poll_input(&mut self, _joypad: &mut JoypadState) -> Vec<ControlEvent> {
        Vec::new()
    }

    pub fn cpu_usage(&mut self) -> Option<f64> {
        None
    }

    pub fn skip_presentation_when_paused(&self) -> bool {
        false
    }

    pub async fn wait_for_shutdown(&mut self) {
        std::future::pending::<()>().await;
    }
}

pub fn init_logging() -> Result<()> {
    use log::LevelFilter;
    use simple_logger::SimpleLogger;

    SimpleLogger::new().with_level(LevelFilter::Info).init()?;
    Ok(())
}
