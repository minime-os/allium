// This file acts as the glue connecting our game runner to different devices.
// If we are on a computer, it opens a game window and listens to keyboard clicks.
// If we are on a Miyoo handheld, it sends pixels to the hardware screen and reads physical buttons.

use crate::control::ControlEvent;
use crate::scale::ScaleMode;
use crate::video::frame::{CapturedFrame, VideoFrameFormat};
use crate::video::{VideoBackend, VideoPresentResult};
use crate::input::JoypadState;
use anyhow::Result;

#[cfg(feature = "miyoo")]
use common::platform::{DefaultPlatform, Platform};
#[cfg(feature = "miyoo")]
use crate::video::miyoo::MiyooVideo;

#[cfg(feature = "simulator")]
use crate::video::simulator::SimulatorVideo;

pub enum PlatformDriver {
    #[cfg(feature = "simulator")]
    Simulator(SimulatorVideo),
    #[cfg(feature = "miyoo")]
    Miyoo(MiyooVideo, DefaultPlatform),
}

impl PlatformDriver {
    // Create a new platform driver based on where we are running
    pub fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self> {
        #[cfg(feature = "simulator")]
        {
            let video = SimulatorVideo::new(source_width, source_height, aspect_ratio, scale)?;
            Ok(Self::Simulator(video))
        }
        #[cfg(feature = "miyoo")]
        {
            let video = MiyooVideo::new(source_width, source_height, aspect_ratio, scale)?;
            let platform = DefaultPlatform::new()?;
            Ok(Self::Miyoo(video, platform))
        }
    }

    // Get the video screen component
    pub fn video(&mut self) -> &mut dyn VideoBackend {
        match self {
            #[cfg(feature = "simulator")]
            Self::Simulator(v) => v,
            #[cfg(feature = "miyoo")]
            Self::Miyoo(v, _) => v,
        }
    }

    // Check buttons pressed by player and return shortcut tasks (like saving or quitting)
    pub fn poll_input(&mut self, joypad: &mut JoypadState) -> Vec<ControlEvent> {
        match self {
            #[cfg(feature = "simulator")]
            Self::Simulator(video) => {
                video.take_key_events().into_iter().for_each(|ev| joypad.apply(ev));
                video.take_control_events()
            }
            #[cfg(feature = "miyoo")]
            Self::Miyoo(_, platform) => {
                while let Some(key_event) = platform.try_poll() {
                    joypad.apply(key_event);
                }
                Vec::new()
            }
        }
    }

    // Present the game picture to the screen
    pub fn present_frame(
        &mut self,
        frame: &CapturedFrame,
        format: VideoFrameFormat,
    ) -> Result<VideoPresentResult> {
        self.video().present(frame, format)
    }
}
