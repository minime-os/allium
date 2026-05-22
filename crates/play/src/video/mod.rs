// Pure video module coordinating frame structures, layout hud overlay, and PPM file screenshots.

pub mod frame;
pub mod ppm;
pub mod hud;

#[derive(Default)]
pub struct VideoPresentResult {
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub should_quit: bool,
}
