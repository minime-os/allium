//! Manages the complete lifecycle, execution loop, and I/O callbacks of a libretro emulation session.
//! This includes loading the core and game, executing emulated frames at target speed, handling asynchronous
//! IPC control commands, and synchronizing video, audio, and save state actions.

// This module coordinates setup, game loading, and the main asynchronous emulation frame loop.

use crate::config::Args;
use crate::audio::{AudioProducer, AudioQueue, validate_sample_rate};
use crate::callbacks::{self, LibretroCallbacks};
use crate::config::PlayConfig;
use crate::control::ControlEvent;
use crate::core::Core;
use crate::hud;
use crate::input::JoypadState;
use crate::libretro_sys::*;
use crate::paths::PlayPaths;
use crate::save;
use crate::scale::ScaleMode;
use crate::udp::CommandState;
use crate::frame::{CapturedFrame, VideoFrameFormat};
use crate::platform::{DefaultPlatform, EmulationPlatform, VideoBackend, InputBackend};
use crate::unzip;
use crate::timing::{self, LoopWait};
use anyhow::{Result, anyhow};
use log::{debug, info, warn};
use std::ffi::CString;
use std::fs;
use std::os::raw::c_void;
use std::ptr;
use std::sync::Arc;
use std::time::{Duration, Instant};

// One session owns the mutable runtime state so callbacks have one place to land.
pub struct PlaySession {
    pub(crate) args: Args,
    pub(crate) paths: PlayPaths,
    pub(crate) config: PlayConfig,
    pub(crate) core: Option<Core>,
    pub(crate) rom_data: Option<Vec<u8>>,
    pub(crate) rom_path_cstring: Option<CString>,
    pub(crate) resolved_rom: Option<unzip::ResolvedRom>,
    pub(crate) captured_frame: Option<CapturedFrame>,
    pub(crate) pixel_format: Option<VideoFrameFormat>,
    pub(crate) av_info: Option<retro_system_av_info>,
    pub(crate) audio_producer: Option<AudioProducer>,
    pub(crate) joypad_state: JoypadState,
    pub(crate) fast_forwarding: bool,
    pub(crate) paused: bool,
    pub(crate) should_quit: bool,
    pub(crate) scale_mode: ScaleMode,
    pub(crate) state_slot: i8,
    pub(crate) command_state: Arc<CommandState>,
    pub(crate) system_dir: CString,
    pub(crate) save_dir: CString,
    pub(crate) hud_state: hud::HudState,
    pub(crate) host_cpu: f64,
}

const DUMP_WARMUP_FRAMES: usize = 60;

fn to_cstring(path: &std::path::Path) -> CString {
    CString::new(path.to_string_lossy().into_owned()).expect("Path must not contain NUL")
}

impl PlaySession {
    fn create_instance(args: Args, paths: PlayPaths, config: PlayConfig, scale: ScaleMode, sys_dir: CString, sav_dir: CString, hud: bool) -> Self {
        Self {
            args, paths, config, core: None, rom_data: None, rom_path_cstring: None,
            resolved_rom: None, captured_frame: None, pixel_format: None, av_info: None,
            audio_producer: None, joypad_state: JoypadState::new(), fast_forwarding: false,
            paused: false, should_quit: false, scale_mode: scale, state_slot: 0,
            command_state: CommandState::new(0), system_dir: sys_dir, save_dir: sav_dir,
            hud_state: hud::HudState::new(hud),
            host_cpu: 0.0,
        }
    }

    /// Constructs a new `PlaySession` from parsed CLI arguments, resolving config and target directories.
    pub fn new(args: Args) -> Self {
        let paths = PlayPaths::from_args(&args);
        let sys_dir = to_cstring(&paths.config_dir);
        let sav_dir = to_cstring(&paths.save_dir);
        let config = PlayConfig::load().unwrap_or_else(|_| PlayConfig::default());
        let hud = args.hud || config.hud || std::env::var("ALLIUM_HUD").is_ok();
        let scale = args.scale;
        Self::create_instance(args, paths, config, scale, sys_dir, sav_dir, hud)
    }

    /// Sets the global callback target to this session, runs the main execution flow, and cleans up the callback target.
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

    /// Stabilizes the initial emulated frame through warmup before dumping it.
    fn warm_up_and_dump(&self) -> Result<()> {
        info!("Running {} warmup frames for dump...", DUMP_WARMUP_FRAMES);
        if let Some(core) = &self.core {
            for _ in 0..DUMP_WARMUP_FRAMES {
                core.run();
            }
        }
        self.dump_captured_frame()
    }

    /// Runs the linear load-and-run sequence for the emulator core and content.
    async fn execute_session(&mut self) -> Result<()> {
        info!("execute_session: loading core...");
        self.load_core()?;
        info!("execute_session: loading game...");
        self.load_game()?;
        info!("execute_session: entering run loop...");

        if self.args.dump_frame.is_some() {
            self.warm_up_and_dump()?;
        } else {
            self.start_main_loop().await?;
        }

        self.unload_game();
        Ok(())
    }

    /// Dynamically loads the libretro shared library core and prints version information.
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

    /// Ensures system and save directories exist prior to content loading.
    fn prepare_directories(&self) -> Result<()> {
        fs::create_dir_all(&self.paths.config_dir)?;
        fs::create_dir_all(&self.paths.save_dir)?;
        Ok(())
    }

    /// Resolves the archive/path structure for the ROM and constructs
    /// the game info descriptor passed to the emulation core.
    fn resolve_and_prepare_rom(&mut self, sys_info: &crate::core::CoreInfo) -> Result<retro_game_info> {
        let resolved = unzip::resolve_rom_path(&self.paths.rom, &sys_info.valid_extensions, sys_info.block_extract)?;
        let path_str = resolved.active_path.to_str().ok_or_else(|| anyhow!("Invalid ROM path"))?;
        self.rom_path_cstring = Some(CString::new(path_str)?);
        let mut game_info = retro_game_info {
            path: self.rom_path_cstring.as_ref().unwrap().as_ptr(),
            data: ptr::null(), size: 0, meta: ptr::null(),
        };
        if !sys_info.need_fullpath {
            let data = fs::read(&resolved.active_path)?;
            game_info.data = data.as_ptr() as *const c_void;
            game_info.size = data.len();
            self.rom_data = Some(data);
        }
        self.resolved_rom = Some(resolved);
        Ok(game_info)
    }

    /// Automatically restores previous SRAM saves or autosaved states if configured.
    fn autoload_sram_and_state(&self) -> Result<()> {
        let core = self.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
        save::load_sram(core, &self.paths)?;
        if self.config.autoload {
            if let Err(err) = save::load_state_slot(core, &self.paths, -1) {
                debug!("Autosave autoload skipped: {}", err);
            }
        }
        Ok(())
    }

    /// Extracts and caches active video/audio geometry from the core.
    fn store_av_info(&mut self) -> Result<()> {
        let core = self.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
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

    /// Loads the active ROM, parsing compressed archives if necessary, loading save files, and resolving AV metadata.
    fn load_game(&mut self) -> Result<()> {
        self.prepare_directories()?;
        let sys_info = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?
            .get_system_info();

        let game_info = self.resolve_and_prepare_rom(&sys_info)?;
        self.core.as_ref().unwrap().load_game(&game_info)?;

        self.autoload_sram_and_state()?;
        self.store_av_info()?;
        Ok(())
    }

    /// Configures audio output buffers and launches the background command server.
    fn setup_audio_and_command_server(&mut self, rate: u32) -> Result<(crate::audio::AudioConsumer, tokio::sync::mpsc::UnboundedReceiver<ControlEvent>, tokio::task::JoinHandle<()>)> {
        let (mut prod, cons) = AudioQueue::for_sample_rate(rate);
        prod.set_muted(self.fast_forwarding);
        self.audio_producer = Some(prod);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Arc::clone(&self.command_state);
        let srv = tokio::spawn(async move {
            if let Err(err) = crate::udp::run_command_server(tx, state).await {
                warn!("Play UDP command server stopped: {}", err);
            }
        });
        Ok((cons, rx, srv))
    }

    /// Processes any pending external UDP or user control events before processing the next frame.
    fn process_pending_control_events(
        &mut self,
        control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
        driver: &mut DefaultPlatform,
    ) -> Result<()> {
        while let Ok(event) = control_rx.try_recv() {
            self.apply_control_event(event)?;
            self.apply_scale(driver.video())?;
        }
        Ok(())
    }

    /// Gathers input changes from the platform layer and applies them to the emulate system keys.
    fn poll_and_apply_platform_inputs(&mut self, driver: &mut DefaultPlatform) {
        let platform_events = driver.input().poll(&mut self.joypad_state);
        for event in platform_events {
            if let Err(err) = self.apply_control_event(event) {
                warn!("Control event failed: {}", err);
            }
        }
    }

    /// Advances the emulator state by one tick, updating the internal hardware.
    fn emulate_single_frame(&mut self, frames_run: &mut u64) -> Result<()> {
        if !self.paused {
            self.core
                .as_ref()
                .ok_or_else(|| anyhow!("Core not loaded"))?
                .run();
            *frames_run += 1;
            self.hud_state.tick_cpu();
        }
        Ok(())
    }

    /// Displays the current frame buffer on the screen if paused logic allows it.
    fn present_captured_frame(&self, driver: &mut DefaultPlatform) -> Result<bool> {
        if self.paused && driver.skip_presentation_when_paused() {
            return Ok(false);
        }
        if let Some(frame) = &self.captured_frame {
            let format = self.pixel_format.ok_or_else(|| anyhow!("No pixel format set"))?;
            if driver.video().present(frame, format)?.should_quit {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn shutdown_loop(&mut self, reason: &str, frames: u64, started_at: Instant, target_fps: f64) {
        let elapsed = started_at.elapsed();
        let avg = if frames == 0 { Duration::ZERO } else { elapsed.div_f64(frames as f64) };
        info!(
            "Frame loop stopped: reason={}, frames={}, elapsed={:?}, avg_frame_time={:?}, target_fps={}",
            reason, frames, elapsed, avg, target_fps
        );
        self.audio_producer = None;
    }

    fn run_loop_step(&mut self, rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>, drv: &mut DefaultPlatform, frames: &mut u64, next_at: &mut Instant, interval: Duration) -> Result<Option<&'static str>> {
        if self.args.frames == Some(*frames) { return Ok(Some("frame cap reached")); }
        self.process_pending_control_events(rx, drv)?;
        if self.should_quit { return Ok(Some("quit command")); }
        self.poll_and_apply_platform_inputs(drv);
        self.host_cpu = drv.stats().cpu_usage().unwrap_or(0.0);
        self.emulate_single_frame(frames)?;
        if self.present_captured_frame(drv)? { return Ok(Some("window closed")); }
        *next_at = (*next_at + interval).max(Instant::now());
        Ok(None)
    }

    async fn wait_frame(
        &mut self,
        deadline: tokio::time::Instant,
        ctrl_c: &mut std::pin::Pin<&mut impl std::future::Future<Output = std::io::Result<()>>>,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
        drv: &mut DefaultPlatform,
    ) -> Result<Option<&'static str>> {
        let result = {
            let mut shutdown = std::pin::pin!(drv.wait_for_shutdown());
            timing::wait_for_next_frame_or_control(deadline, ctrl_c, &mut shutdown, rx).await
        };
        match result {
            LoopWait::Frame => Ok(None),
            LoopWait::Signal => Ok(Some("signal received")),
            LoopWait::Control(event) => {
                self.apply_control_event(event)?;
                self.apply_scale(drv.video())?;
                Ok(None)
            }
        }
    }

    async fn run_emulation_loop(
        &mut self,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
        drv: &mut DefaultPlatform,
        frames: &mut u64,
        next_at: &mut Instant,
        interval: Duration,
        ctrl_c: &mut std::pin::Pin<&mut impl std::future::Future<Output = std::io::Result<()>>>,
    ) -> Result<&'static str> {
        loop {
            if let Some(reason) = self.run_loop_step(rx, drv, frames, next_at, interval)? {
                return Ok(reason);
            }
            if self.fast_forwarding {
                tokio::task::yield_now().await;
                continue;
            }
            let sleep = tokio::time::Instant::from_std(*next_at);
            if let Some(r) = self.wait_frame(sleep, ctrl_c, rx, drv).await? {
                return Ok(r);
            }
        }
    }

    /// Spawns background I/O tasks and loops emulated frames indefinitely, respecting the target core framerate.
    async fn start_main_loop(&mut self) -> Result<()> {
        let av = self.av_info.as_ref().unwrap();
        let fps = av.timing.fps;
        let frame_interval = timing::frame_interval(fps)?;
        let sample_rate = validate_sample_rate(av.timing.sample_rate)?;
        let base_width = av.geometry.base_width;
        let base_height = av.geometry.base_height;
        let aspect_ratio = av.geometry.aspect_ratio;
        let (cons, mut rx, command_server) = self.setup_audio_and_command_server(sample_rate)?;
        let mut drv = DefaultPlatform::initialize(
            base_width,
            base_height,
            aspect_ratio,
            self.args.scale,
            sample_rate,
            cons,
        )?;
        let mut frames = 0u64;
        let started_at = Instant::now();
        let mut next_at = started_at;
        let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
        let reason = self.run_emulation_loop(&mut rx, &mut drv, &mut frames, &mut next_at, frame_interval, &mut ctrl_c,
        ).await?;
        self.shutdown_loop(reason, frames, started_at, fps);
        command_server.abort();
        Ok(())
    }

    /// Unloads the core game cleanly, triggering an autosave write and saving emulated SRAM cartridge contents.
    fn unload_game(&mut self) {
        if let Some(core) = &self.core {
            if self.config.autosave {
                if let Err(err) = save::save_state_slot(core, &self.paths, -1) {
                    warn!("Failed to autosave state: {}", err);
                }
            }
        }
        if let Some(core) = self.core.take() {
            if let Err(err) = save::save_sram(&core, &self.paths) {
                warn!("Failed to save SRAM: {}", err);
            }
            core.unload_game();
        }
        self.resolved_rom = None;
    }

    /// Changes the active save state slot index, validating bounds.
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub(crate) fn select_state_slot(&mut self, slot: i8) -> Result<()> {
        if !(-1..=9).contains(&slot) {
            return Err(anyhow!("Save state slot must be between 0 and 9"));
        }

        self.state_slot = slot;
        self.command_state.set_state_slot(slot);
        info!("Selected state slot {}", slot);
        Ok(())
    }

    /// Recalculates output coordinates and applies scaling changes to the video backend.
    fn apply_scale(&self, video: &mut impl VideoBackend) -> Result<()> {
        let av_info = self
            .av_info
            .as_ref()
            .ok_or_else(|| anyhow!("AV info not loaded"))?;
        video.set_scale(
            self.scale_mode,
            av_info.geometry.base_width,
            av_info.geometry.base_height,
            av_info.geometry.aspect_ratio,
        )
    }

    /// Encodes and dumps the currently active captured frame buffer to disk in PPM format.
    fn dump_captured_frame(&self) -> Result<()> {
        let Some(path) = &self.args.dump_frame else {
            return Ok(());
        };

        let frame = self
            .captured_frame
            .as_ref()
            .ok_or_else(|| anyhow!("No frame captured"))?;
        crate::dump::dump_frame(path, frame, self.pixel_format)?;
        info!("Frame dumped to {:?}", path);
        Ok(())
    }

    /// Applies a specific execution control event mutation onto this session.
    fn apply_control_event(&mut self, event: ControlEvent) -> Result<()> {
        event.apply(self)
    }
}

impl Drop for PlaySession {
    /// Assures resources and open games are unloaded correctly on scope exit.
    fn drop(&mut self) {
        self.unload_game();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::ffi::CStr;
    use std::os::raw::c_char;

    fn test_session() -> PlaySession {
        PlaySession::new(Args::parse_from([
            "play",
            "--rom",
            "game.nes",
            "--core",
            "nestopia_libretro.dylib",
            "--core-id",
            "nestopia",
        ]))
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

