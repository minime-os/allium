// Headless mock video backend.
// It stores the last captured frame to support headless automated testing and PPM screenshots on demand.

use crate::platform::VideoBackend;
use crate::video::ScaleMode;
use crate::video::{CapturedFrame, VideoFrameFormat};
use crate::platform::VideoPresentResult;
use crate::diagnostics::dump_frame;
use anyhow::{anyhow, Result};
use std::path::Path;

pub struct MockVideo {
    pub last_frame: Option<CapturedFrame>,
    pub last_format: Option<VideoFrameFormat>,
}

impl VideoBackend for MockVideo {
    // Saves the presented frame to in-memory state.
    fn present(
        &mut self,
        frame: &CapturedFrame,
        format: VideoFrameFormat,
    ) -> Result<VideoPresentResult> {
        self.last_frame = Some(frame.clone());
        self.last_format = Some(format);
        Ok(VideoPresentResult::default())
    }

    // Adjusts scale configuration (no-op in headless mode).
    fn set_scale(
        &mut self,
        _mode: ScaleMode,
        _source_width: u32,
        _source_height: u32,
        _aspect_ratio: f32,
    ) -> Result<()> {
        Ok(())
    }
}

impl MockVideo {
    // Initializes the mock video backend.
    pub fn new() -> Self {
        Self {
            last_frame: None,
            last_format: None,
        }
    }

    // Dumps the last presented frame to disk in PPM format.
    pub fn dump_last_frame(&self, path: &Path) -> Result<()> {
        let frame = self.last_frame.as_ref().ok_or_else(|| anyhow!("No frame captured yet"))?;
        let format = self.last_format.ok_or_else(|| anyhow!("No frame format recorded"))?;
        dump_frame(path, frame, Some(format))
    }
}
