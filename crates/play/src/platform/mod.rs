// Emulation platform abstractions mapping hardware-specific layers.
// This module provides traits that physical devices (Miyoo, Simulator, Mock)
// must implement to connect video presentation, audio playback, and button inputs.

pub mod mock;

#[cfg(feature = "miyoo")]
pub mod miyoo;

#[cfg(feature = "simulator")]
pub mod simulator;

use crate::shortcuts::ControlEvent;
use crate::input::JoypadState;
use crate::video::ScaleMode;
use crate::video::{CapturedFrame, VideoFrameFormat};
use anyhow::Result;

#[derive(Default)]
pub struct VideoPresentResult {
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub should_quit: bool,
}

pub trait HostStats {
    fn cpu_usage(&mut self) -> Option<f64>;
}

pub trait VideoBackend {
    fn present(
        &mut self,
        frame: &CapturedFrame,
        format: VideoFrameFormat,
    ) -> Result<VideoPresentResult>;

    fn set_scale(
        &mut self,
        mode: ScaleMode,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
    ) -> Result<()>;
}

pub trait AudioBackend {}

pub trait InputBackend {
    fn poll(&mut self, joypad: &mut JoypadState) -> Vec<ControlEvent>;
}

pub trait EmulationPlatform {
    type Video: VideoBackend;
    type Audio: AudioBackend;
    type Input: InputBackend;

    fn initialize(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
        sample_rate: u32,
        audio_consumer: crate::audio::AudioConsumer,
    ) -> Result<Self>
    where
        Self: Sized;

    fn video(&mut self) -> &mut Self::Video;
    fn audio(&mut self) -> &mut Self::Audio;
    fn input(&mut self) -> &mut Self::Input;
    fn stats(&mut self) -> &mut dyn HostStats;
    fn skip_presentation_when_paused(&self) -> bool { false }
    async fn wait_for_shutdown(&mut self) {
        std::future::pending::<()>().await;
    }
}

pub fn init_logging() -> Result<()> {
    #[cfg(feature = "miyoo")]
    return miyoo::init_logging();
    #[cfg(feature = "simulator")]
    return simulator::init_logging();
    #[cfg(not(any(feature = "miyoo", feature = "simulator")))]
    return mock::init_logging();
}

#[cfg(feature = "miyoo")]
pub type DefaultPlatform = miyoo::MiyooPlatform;

#[cfg(feature = "simulator")]
pub type DefaultPlatform = simulator::SimulatorPlatform;

#[cfg(not(any(feature = "miyoo", feature = "simulator")))]
pub type DefaultPlatform = mock::MockPlatform;
