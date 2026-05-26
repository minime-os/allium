// Desktop host platform bootstrapper coordinating video window, sound output, and input listening.

pub mod audio;
pub mod stats;
pub mod video;

use crate::audio::AudioConsumer;
use crate::commands::ControlEvent;
use crate::input::JoypadState;
use crate::video::ScaleMode;
use anyhow::Result;
use audio::SimulatorAudio;
use common::platform::KeyEvent;
use std::sync::mpsc::Receiver;
use video::SimulatorVideo;

pub struct SimulatorPlatform {
    pub video: SimulatorVideo,
    _audio: SimulatorAudio,
    input: SimulatorInput,
    stats: stats::SimulatorStats,
}

pub struct SimulatorInput {
    key_rx: Receiver<KeyEvent>,
    control_rx: Receiver<ControlEvent>,
}

impl SimulatorPlatform {
    pub fn new(
        _core_id: &str,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
        sample_rate: u32,
        audio_consumer: AudioConsumer,
    ) -> Result<Self> {
        let (key_tx, key_rx) = std::sync::mpsc::channel();
        let (control_tx, control_rx) = std::sync::mpsc::channel();
        let video = SimulatorVideo::new(
            source_width,
            source_height,
            aspect_ratio,
            scale,
            key_tx,
            control_tx,
        )?;
        let _audio = SimulatorAudio::new(sample_rate, audio_consumer)?;
        let input = SimulatorInput { key_rx, control_rx };
        let stats = stats::SimulatorStats::new();
        Ok(Self {
            video,
            _audio,
            input,
            stats,
        })
    }

    pub fn poll_input(&mut self, joypad: &mut JoypadState) -> Vec<ControlEvent> {
        while let Ok(key_event) = self.input.key_rx.try_recv() {
            joypad.apply(key_event);
        }
        let mut control_events = Vec::new();
        while let Ok(control_event) = self.input.control_rx.try_recv() {
            control_events.push(control_event);
        }
        control_events
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
