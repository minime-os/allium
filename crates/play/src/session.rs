use crate::args::Args;
use crate::callbacks::{self, LibretroCallbacks};
use crate::core::Core;
use crate::frame::{CapturedFrame, encode_rgb565_ppm};
use crate::libretro_sys::*;
use crate::paths::PlayPaths;
use anyhow::{Result, anyhow};
use log::info;
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_uint, c_void};
use std::ptr;

// One session owns the mutable runtime state so callbacks have one place to land.
pub struct PlaySession {
    args: Args,
    paths: PlayPaths,
    core: Option<Core>,
    rom_data: Option<Vec<u8>>,
    captured_frame: Option<CapturedFrame>,
    rgb565_enabled: bool,
}

impl PlaySession {
    // Resolve paths up front so later stages do not repeat path policy.
    pub fn new(args: Args) -> Self {
        let paths = PlayPaths::from_args(&args);
        Self {
            args,
            paths,
            core: None,
            rom_data: None,
            captured_frame: None,
            rgb565_enabled: false,
        }
    }

    // C callbacks need a stable active session pointer while the core is running.
    pub async fn run(&mut self) -> Result<()> {
        info!("Initializing PlaySession for core: {}", self.paths.core_id);
        info!("ROM path: {:?}", self.paths.rom);

        unsafe {
            let ptr = self as *mut PlaySession;
            callbacks::set_handler(ptr);
        }

        let result = self.execute_session().await;

        unsafe {
            callbacks::clear_handler();
        }

        result
    }

    // The order mirrors libretro's lifecycle: core first, content second, frames last.
    async fn execute_session(&mut self) -> Result<()> {
        self.load_core()?;
        self.load_game()?;

        if self.args.dump_frame.is_some() {
            info!("Running one frame for dump...");
            if let Some(core) = &self.core {
                core.run();
            }
            self.dump_captured_frame()?;
        } else {
            self.start_main_loop()?;
        }

        Ok(())
    }

    // Logging core metadata makes wrong core/path issues visible early.
    fn load_core(&mut self) -> Result<()> {
        info!("Loading core from {:?}", self.paths.core_path);
        let core = unsafe { Core::load(&self.paths.core_path)? };
        let info = core.get_system_info();

        info!(
            "Core loaded: {} ({})",
            info.library_name, info.library_version
        );
        info!("Extensions: {}", info.valid_extensions);

        self.core = Some(core);
        Ok(())
    }

    // Some cores borrow the ROM buffer after retro_load_game, so the Vec lives in the session.
    fn load_game(&mut self) -> Result<()> {
        let core = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?;
        let sys_info = core.get_system_info();

        let path_str = self
            .paths
            .rom
            .to_str()
            .ok_or_else(|| anyhow!("Invalid ROM path"))?;
        let c_path = CString::new(path_str)?;

        let mut game_info = retro_game_info {
            path: c_path.as_ptr(),
            data: ptr::null(),
            size: 0,
            meta: ptr::null(),
        };

        if !sys_info.need_fullpath {
            info!("Core needs ROM data in memory, loading...");
            let data = fs::read(&self.paths.rom)?;
            game_info.data = data.as_ptr() as *const c_void;
            game_info.size = data.len();
            self.rom_data = Some(data);
        }

        core.load_game(&game_info)?;

        let av_info = core.get_system_av_info();
        info!(
            "AV Info: {}x{} @ {} fps, sample_rate: {}",
            av_info.geometry.base_width,
            av_info.geometry.base_height,
            av_info.timing.fps,
            av_info.timing.sample_rate
        );

        Ok(())
    }

    // This stays boring until video/audio/input timing exists.
    fn start_main_loop(&self) -> Result<()> {
        info!("(Skeleton) Starting main emulation loop");
        Ok(())
    }

    // A one-frame dump proves callbacks and pixel conversion before real video exists.
    fn dump_captured_frame(&self) -> Result<()> {
        let path = match &self.args.dump_frame {
            Some(p) => p,
            None => return Ok(()),
        };

        let frame = self
            .captured_frame
            .as_ref()
            .ok_or_else(|| anyhow!("No frame captured"))?;
        if !self.rgb565_enabled {
            return Err(anyhow!("Frame dump requires RGB565 pixel format"));
        }

        let ppm_data = encode_rgb565_ppm(frame)?;

        fs::write(path, ppm_data)?;
        info!("Frame dumped to {:?}", path);
        Ok(())
    }
}

// Callback methods mutate session state instead of using scattered globals.
impl LibretroCallbacks for PlaySession {
    fn on_environment(&mut self, cmd: c_uint, data: *mut c_void) -> bool {
        match cmd {
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
                let format = unsafe { *(data as *const retro_pixel_format) };
                if format == retro_pixel_format_RETRO_PIXEL_FORMAT_RGB565 {
                    self.rgb565_enabled = true;
                    info!("Core set pixel format: RGB565");
                    true
                } else {
                    info!("Unsupported pixel format: {}", format);
                    false
                }
            }
            _ => false,
        }
    }

    // Copy now because libretro only promises the pointer is valid during the callback.
    fn on_video_refresh(
        &mut self,
        data: *const c_void,
        width: c_uint,
        height: c_uint,
        pitch: usize,
    ) {
        if data.is_null() {
            return;
        }

        let size = pitch * height as usize;
        let mut buffer = vec![0u8; size];
        unsafe {
            ptr::copy_nonoverlapping(data as *const u8, buffer.as_mut_ptr(), size);
        }
        self.captured_frame = Some(CapturedFrame::new(buffer, width, height, pitch));
    }

    fn on_audio_sample(&mut self, _left: i16, _right: i16) {}

    fn on_audio_sample_batch(&mut self, _data: *const i16, frames: usize) -> usize {
        frames
    }

    fn on_input_poll(&mut self) {}

    fn on_input_state(
        &mut self,
        _port: c_uint,
        _device: c_uint,
        _index: c_uint,
        _id: c_uint,
    ) -> i16 {
        0
    }
}

// Unload content before deinit so the core can release game-specific state cleanly.
impl Drop for PlaySession {
    fn drop(&mut self) {
        if let Some(core) = &self.core {
            core.unload_game();
        }
    }
}
