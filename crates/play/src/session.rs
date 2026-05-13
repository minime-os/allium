use crate::args::Args;
use crate::callbacks::{self, LibretroCallbacks};
use crate::core::Core;
use crate::frame::{CapturedFrame, encode_rgb565_ppm, encode_xrgb8888_ppm};
use crate::libretro_sys::*;
use crate::paths::PlayPaths;
#[cfg(feature = "simulator")]
use crate::simulator_video::{SimulatorPixelFormat, SimulatorVideo};
use anyhow::{Context, Result, anyhow};
use log::{debug, info};
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_uint, c_void};
use std::ptr;
use std::time::{Duration, Instant};

// One session owns the mutable runtime state so callbacks have one place to land.
pub struct PlaySession {
    args: Args,
    paths: PlayPaths,
    core: Option<Core>,
    rom_data: Option<Vec<u8>>,
    captured_frame: Option<CapturedFrame>,
    pixel_format: Option<FramePixelFormat>,
    av_info: Option<retro_system_av_info>,
    system_dir: CString,
    save_dir: CString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FramePixelFormat {
    Rgb565,
    Xrgb8888,
}

const DUMP_WARMUP_FRAMES: usize = 60;

impl PlaySession {
    // Resolve paths up front so later stages do not repeat path policy.
    pub fn new(args: Args) -> Self {
        let paths = PlayPaths::from_args(&args);
        let system_dir = CString::new(paths.config_dir.to_string_lossy().into_owned())
            .expect("Play system dir must not contain NUL");
        let save_dir = CString::new(paths.save_dir.to_string_lossy().into_owned())
            .expect("Play save dir must not contain NUL");
        Self {
            args,
            paths,
            core: None,
            rom_data: None,
            captured_frame: None,
            pixel_format: None,
            av_info: None,
            system_dir,
            save_dir,
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
            info!("Running {} warmup frames for dump...", DUMP_WARMUP_FRAMES);
            if let Some(core) = &self.core {
                for _ in 0..DUMP_WARMUP_FRAMES {
                    core.run();
                }
            }
            self.dump_captured_frame()?;
        } else {
            self.start_main_loop().await?;
        }

        self.unload_game();

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
        fs::create_dir_all(&self.paths.config_dir)?;
        fs::create_dir_all(&self.paths.save_dir)?;

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
        self.av_info = Some(av_info);

        Ok(())
    }

    // One retro_run call advances one emulated frame; this keeps that cadence near core FPS.
    async fn start_main_loop(&mut self) -> Result<()> {
        let av_info = self
            .av_info
            .as_ref()
            .ok_or_else(|| anyhow!("AV info not loaded"))?;
        let target_fps = av_info.timing.fps;
        let frame_interval = frame_interval(target_fps)?;
        let mut frames_run = 0u64;
        let started_at = Instant::now();
        let mut next_frame_at = started_at;
        let shutdown_reason;
        let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("Failed to install SIGTERM handler")?;
        #[cfg(feature = "simulator")]
        let mut simulator_video =
            SimulatorVideo::new(av_info.geometry.base_width, av_info.geometry.base_height)?;

        info!(
            "Starting main emulation loop at {} fps{}",
            target_fps,
            self.args
                .frames
                .map(|frames| format!(" for {} frames", frames))
                .unwrap_or_else(|| " until shutdown".to_string())
        );

        loop {
            if self.args.frames == Some(frames_run) {
                shutdown_reason = "frame cap reached";
                break;
            }

            self.core
                .as_ref()
                .ok_or_else(|| anyhow!("Core not loaded"))?
                .run();
            frames_run += 1;
            #[cfg(feature = "simulator")]
            if self.present_simulator_frame(&mut simulator_video)? {
                shutdown_reason = "window closed";
                break;
            }
            next_frame_at += frame_interval;

            let sleep_until = tokio::time::Instant::from_std(next_frame_at);
            let shutdown_requested = {
                #[cfg(unix)]
                {
                    wait_for_next_frame(sleep_until, &mut ctrl_c, &mut sigterm).await
                }
                #[cfg(not(unix))]
                {
                    wait_for_next_frame(sleep_until, &mut ctrl_c).await
                }
            };
            if shutdown_requested {
                shutdown_reason = "signal received";
                break;
            }
        }

        let elapsed = started_at.elapsed();
        let avg_frame_time = if frames_run == 0 {
            Duration::ZERO
        } else {
            elapsed.div_f64(frames_run as f64)
        };
        info!(
            "Frame loop stopped: reason={}, frames={}, elapsed={:?}, avg_frame_time={:?}, target_fps={}",
            shutdown_reason, frames_run, elapsed, avg_frame_time, target_fps
        );
        Ok(())
    }

    fn unload_game(&mut self) {
        if let Some(core) = self.core.take() {
            core.unload_game();
        }
    }

    #[cfg(feature = "simulator")]
    fn present_simulator_frame(&self, video: &mut SimulatorVideo) -> Result<bool> {
        let frame = match &self.captured_frame {
            Some(frame) => frame,
            None => return Ok(false),
        };
        let format = match self.pixel_format {
            Some(FramePixelFormat::Rgb565) => SimulatorPixelFormat::Rgb565,
            Some(FramePixelFormat::Xrgb8888) => SimulatorPixelFormat::Xrgb8888,
            None => return Ok(false),
        };

        video.present(frame, format)
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
        let ppm_data = match self.pixel_format {
            Some(FramePixelFormat::Rgb565) => encode_rgb565_ppm(frame)?,
            Some(FramePixelFormat::Xrgb8888) => encode_xrgb8888_ppm(frame)?,
            None => return Err(anyhow!("Frame dump requires a supported pixel format")),
        };

        fs::write(path, ppm_data)?;
        info!("Frame dumped to {:?}", path);
        Ok(())
    }
}

fn frame_interval(fps: f64) -> Result<Duration> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(anyhow!("Core reported invalid FPS: {}", fps));
    }

    Ok(Duration::from_secs_f64(1.0 / fps))
}

#[cfg(unix)]
async fn wait_for_next_frame<F>(
    deadline: tokio::time::Instant,
    ctrl_c: &mut std::pin::Pin<&mut F>,
    sigterm: &mut tokio::signal::unix::Signal,
) -> bool
where
    F: std::future::Future<Output = std::io::Result<()>>,
{
    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => false,
        _ = ctrl_c.as_mut() => true,
        _ = sigterm.recv() => true,
    }
}

#[cfg(not(unix))]
async fn wait_for_next_frame<F>(
    deadline: tokio::time::Instant,
    ctrl_c: &mut std::pin::Pin<&mut F>,
) -> bool
where
    F: std::future::Future<Output = std::io::Result<()>>,
{
    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => false,
        _ = ctrl_c.as_mut() => true,
    }
}

// Callback methods mutate session state instead of using scattered globals.
impl LibretroCallbacks for PlaySession {
    fn on_environment(&mut self, cmd: c_uint, data: *mut c_void) -> bool {
        match cmd {
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
                let format = unsafe { *(data as *const retro_pixel_format) };
                if format == retro_pixel_format_RETRO_PIXEL_FORMAT_RGB565 {
                    self.pixel_format = Some(FramePixelFormat::Rgb565);
                    info!("Core set pixel format: RGB565");
                    true
                } else if format == retro_pixel_format_RETRO_PIXEL_FORMAT_XRGB8888 {
                    self.pixel_format = Some(FramePixelFormat::Xrgb8888);
                    info!("Core set pixel format: XRGB8888");
                    true
                } else {
                    info!("Unsupported pixel format: {}", format);
                    false
                }
            }
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY => self.write_env_path(data, &self.system_dir),
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => self.write_env_path(data, &self.save_dir),
            _ => {
                debug!("Unsupported environment command: {}", cmd);
                false
            }
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
        let frame = self
            .captured_frame
            .get_or_insert_with(|| CapturedFrame::new(vec![0u8; size], width, height, pitch));

        if frame.data.len() != size {
            frame.data.resize(size, 0);
        }

        frame.width = width;
        frame.height = height;
        frame.pitch = pitch;
        unsafe {
            ptr::copy_nonoverlapping(data as *const u8, frame.data.as_mut_ptr(), size);
        }
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

impl PlaySession {
    fn write_env_path(&self, data: *mut c_void, path: &CString) -> bool {
        if data.is_null() {
            return false;
        }

        unsafe {
            *(data as *mut *const c_char) = path.as_ptr();
        }
        true
    }
}

// Unload content before deinit so the core can release game-specific state cleanly.
impl Drop for PlaySession {
    fn drop(&mut self) {
        self.unload_game();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::c_char;
    use std::path::PathBuf;
    use std::time::Duration;

    fn test_session() -> PlaySession {
        PlaySession::new(Args {
            rom: PathBuf::from("game.nes"),
            core_path: PathBuf::from("nestopia_libretro.dylib"),
            core_id: "nestopia".to_string(),
            dump_frame: None,
            frames: None,
        })
    }

    #[test]
    fn frame_interval_uses_core_fps() {
        let interval = frame_interval(60.0).unwrap();

        assert_eq!(interval, Duration::from_nanos(16_666_667));
    }

    #[test]
    fn frame_interval_rejects_zero_fps() {
        let err = frame_interval(0.0).unwrap_err();

        assert_eq!(err.to_string(), "Core reported invalid FPS: 0");
    }

    #[test]
    fn frame_interval_rejects_nan_fps() {
        let err = frame_interval(f64::NAN).unwrap_err();

        assert_eq!(err.to_string(), "Core reported invalid FPS: NaN");
    }

    #[test]
    fn returns_system_directory_to_core() {
        let mut session = test_session();
        let mut system_dir: *const c_char = ptr::null();

        let handled = session.on_environment(
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY,
            &mut system_dir as *mut *const c_char as *mut c_void,
        );

        assert!(handled);
        assert!(!system_dir.is_null());
        let path = unsafe { CStr::from_ptr(system_dir) }
            .to_string_lossy()
            .into_owned();
        assert!(path.contains(".allium/config/play/nestopia"));
    }

    #[test]
    fn returns_save_directory_to_core() {
        let mut session = test_session();
        let mut save_dir: *const c_char = ptr::null();

        let handled = session.on_environment(
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY,
            &mut save_dir as *mut *const c_char as *mut c_void,
        );

        assert!(handled);
        assert!(!save_dir.is_null());
        let path = unsafe { CStr::from_ptr(save_dir) }
            .to_string_lossy()
            .into_owned();
        assert!(path.contains("Saves/CurrentProfile/play/nestopia"));
    }

    #[test]
    fn accepts_xrgb8888_pixel_format() {
        let mut session = test_session();
        let mut format = retro_pixel_format_RETRO_PIXEL_FORMAT_XRGB8888;

        let handled = session.on_environment(
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
            &mut format as *mut retro_pixel_format as *mut c_void,
        );

        assert!(handled);
    }

    #[test]
    fn reuses_frame_buffer_when_geometry_matches() {
        let mut session = test_session();
        let first = [1u8, 2, 3, 4];
        let second = [5u8, 6, 7, 8];

        session.on_video_refresh(first.as_ptr() as *const c_void, 1, 1, 4);
        let first_ptr = session.captured_frame.as_ref().unwrap().data.as_ptr();

        session.on_video_refresh(second.as_ptr() as *const c_void, 1, 1, 4);
        let frame = session.captured_frame.as_ref().unwrap();

        assert_eq!(frame.data.as_ptr(), first_ptr);
        assert_eq!(frame.data, second);
    }
}
