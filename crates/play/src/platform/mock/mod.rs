// Headless mock platform bootstrapper coordinating video/audio/input components in headless test runs.

pub mod audio;
pub mod stats;
pub mod video;

use crate::audio::AudioConsumer;
use crate::control::ControlEvent;
use crate::input::JoypadState;
use crate::platform::{EmulationPlatform, HostStats, InputBackend};
use crate::scale::ScaleMode;
use anyhow::Result;
use audio::MockAudio;
use video::MockVideo;

pub struct MockPlatform {
    video: MockVideo,
    audio: MockAudio,
    input: MockInput,
    stats: stats::MockStats,
}

pub struct MockInput;

impl InputBackend for MockInput {
    // Headless mock poll loop returning empty inputs.
    fn poll(&mut self, _joypad: &mut JoypadState) -> Vec<ControlEvent> {
        Vec::new()
    }
}

impl EmulationPlatform for MockPlatform {
    type Video = MockVideo;
    type Audio = MockAudio;
    type Input = MockInput;

    fn initialize(
        _source_width: u32,
        _source_height: u32,
        _aspect_ratio: f32,
        _scale: ScaleMode,
        _sample_rate: u32,
        _audio_consumer: AudioConsumer,
    ) -> Result<Self> {
        let video = MockVideo::new();
        let audio = MockAudio::new();
        let input = MockInput;
        let stats = stats::MockStats::new();
        Ok(Self {
            video,
            audio,
            input,
            stats,
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
}
