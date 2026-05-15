pub mod convert;
pub mod frame;
pub mod ppm;

#[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
pub mod miyoo;
#[cfg(feature = "simulator")]
pub mod simulator;

use crate::scale::ScaleMode;
use anyhow::Result;
use frame::{CapturedFrame, VideoFrameFormat};

#[derive(Default)]
pub struct VideoPresentResult {
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub should_quit: bool,
}

pub trait VideoBackend {
    fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self>
    where
        Self: Sized;

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
