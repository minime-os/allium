use crate::args::Args;
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
use crate::video::frame::{CapturedFrame, VideoFrameFormat};
use crate::platform::PlatformDriver;
use crate::unzip;
use crate::video::{self, VideoBackend};
use anyhow::{Context, Result, anyhow};
use log::{debug, info, warn};
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_uint, c_void};
use std::ptr;
use std::sync::Arc;
use std::time::{Duration, Instant};

// One session owns the mutable runtime state so callbacks have one place to land.
pub struct PlaySession {
    args: Args,
    paths: PlayPaths,
    config: PlayConfig,
    pub(crate) core: Option<Core>,
    rom_data: Option<Vec<u8>>,
    rom_path_cstring: Option<CString>,
    resolved_rom: Option<unzip::ResolvedRom>,
    captured_frame: Option<CapturedFrame>,
    pixel_format: Option<VideoFrameFormat>,
    av_info: Option<retro_system_av_info>,
    pub(crate) audio_producer: Option<AudioProducer>,
    joypad_state: JoypadState,
    pub(crate) fast_forwarding: bool,
    pub(crate) paused: bool,
    pub(crate) should_quit: bool,
    pub(crate) scale_mode: ScaleMode,
    pub(crate) state_slot: i8,
    command_state: Arc<CommandState>,
    system_dir: CString,
    save_dir: CString,
    hud_state: hud::HudState,
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
        let config = PlayConfig::load().unwrap_or_else(|err| {
            warn!("Failed to load Play config, using defaults: {}", err);
            PlayConfig::default()
        });
        let scale_mode = args.scale;
        let hud_enabled = args.hud || config.hud || std::env::var("ALLIUM_HUD").is_ok();
        Self {
            args,
            paths,
            config,
            core: None,
            rom_data: None,
            rom_path_cstring: None,
            resolved_rom: None,
            captured_frame: None,
            pixel_format: None,
            av_info: None,
            audio_producer: None,
            joypad_state: JoypadState::new(),
            fast_forwarding: false,
            paused: false,
            should_quit: false,
            scale_mode,
            state_slot: 0,
            command_state: CommandState::new(0),
            system_dir,
            save_dir,
            hud_state: hud::HudState::new(hud_enabled),
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
        #[cfg(feature = "miyoo")]
        let _miyoo_guard = crate::miyoo_env::MiyooSystemGuard::new(&self.paths.core_id);

        info!("execute_session: loading core...");
        self.load_core()?;
        info!("execute_session: loading game...");
        self.load_game()?;
        info!("execute_session: entering run loop...");

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
    // The CString also needs session lifetime: cores often cache the path pointer internally,
    // so dropping it right after load_game is UB that shows up as a SIGSEGV on the next call.
    fn load_game(&mut self) -> Result<()> {
        info!("load_game: ensuring config/save dirs exist");
        fs::create_dir_all(&self.paths.config_dir)?;
        fs::create_dir_all(&self.paths.save_dir)?;

        let sys_info = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?
            .get_system_info();

        info!("load_game: resolving ROM path...");
        let resolved = unzip::resolve_rom_path(
            &self.paths.rom,
            &sys_info.valid_extensions,
            sys_info.block_extract,
        )?;
        let rom_path = &resolved.active_path;
        let path_str = rom_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid ROM path"))?;
        let c_path = CString::new(path_str)?;
        self.rom_path_cstring = Some(c_path);
        // Safe because c_path now lives in self for the whole session.
        let path_ptr = self.rom_path_cstring.as_ref().unwrap().as_ptr();

        let mut game_info = retro_game_info {
            path: path_ptr,
            data: ptr::null(),
            size: 0,
            meta: ptr::null(),
        };

        if !sys_info.need_fullpath {
            info!("load_game: reading ROM data into memory...");
            let data = fs::read(rom_path)?;
            game_info.data = data.as_ptr() as *const c_void;
            game_info.size = data.len();
            self.rom_data = Some(data);
        }

        let core = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?;
        info!("load_game: calling core.load_game...");
        core.load_game(&game_info)?;
        info!("load_game: core.load_game returned OK");

        info!("load_game: calling load_sram...");
        self.load_sram()?;
        info!("load_game: load_sram returned OK");

        if self.config.autoload
            && let Err(err) = self.load_state_slot(-1)
        {
            debug!("Autosave autoload skipped: {}", err);
        }

        info!("load_game: calling core.get_system_av_info...");
        let av_info = core.get_system_av_info();
        info!(
            "AV Info: {}x{} @ {} fps, sample_rate: {}",
            av_info.geometry.base_width,
            av_info.geometry.base_height,
            av_info.timing.fps,
            av_info.timing.sample_rate
        );
        self.av_info = Some(av_info);

        self.resolved_rom = Some(resolved);
        info!("load_game: complete");
        Ok(())
    }

    // One retro_run call advances one emulated frame; this keeps that cadence near core FPS.
    async fn start_main_loop(&mut self) -> Result<()> {
        let av_info = self
            .av_info
            .as_ref()
            .ok_or_else(|| anyhow!("AV info not loaded"))?;
        let target_fps = av_info.timing.fps;
        let audio_sample_rate = validate_sample_rate(av_info.timing.sample_rate)?;
        let frame_interval = frame_interval(target_fps)?;

        let mut driver = PlatformDriver::new(
            av_info.geometry.base_width,
            av_info.geometry.base_height,
            av_info.geometry.aspect_ratio,
            self.args.scale,
        )?;

        let mut frames_run = 0u64;
        let started_at = Instant::now();
        let mut next_frame_at = started_at;
        let shutdown_reason;
        let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("Failed to install SIGTERM handler")?;

        let (mut audio_producer, audio_consumer) = AudioQueue::for_sample_rate(audio_sample_rate);
        audio_producer.set_muted(self.fast_forwarding);
        self.audio_producer = Some(audio_producer);
        let _audio = crate::audio::AudioDriver::new(audio_sample_rate, audio_consumer)?;
        let (control_tx, mut control_rx) = tokio::sync::mpsc::unbounded_channel();
        let command_state = Arc::clone(&self.command_state);
        let command_server = tokio::spawn(async move {
            if let Err(err) = crate::udp::run_command_server(control_tx, command_state).await {
                warn!("Play UDP command server stopped: {}", err);
            }
        });

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
            while let Ok(event) = control_rx.try_recv() {
                self.apply_control_event(event)?;
                self.apply_scale(driver.video())?;
            }
            if self.should_quit {
                shutdown_reason = "quit command";
                break;
            }
            let platform_events = driver.poll_input(&mut self.joypad_state);
            for event in platform_events {
                if let Err(err) = self.apply_control_event(event) {
                    warn!("Control event failed: {}", err);
                }
            }
            if !self.paused {
                self.core
                    .as_ref()
                    .ok_or_else(|| anyhow!("Core not loaded"))?
                    .run();
                frames_run += 1;
                self.hud_state.tick_cpu();
            }
            #[allow(unused_mut)]
            let mut should_present = true;
            #[cfg(feature = "miyoo")]
            if self.paused {
                should_present = false;
            }
            if should_present {
                if let Some(frame) = &self.captured_frame {
                    let format = self.pixel_format.ok_or_else(|| anyhow!("No pixel format set"))?;
                    let present_res = driver.present_frame(frame, format)?;
                    if present_res.should_quit {
                        shutdown_reason = "window closed";
                        break;
                    }
                }
            }
            next_frame_at += frame_interval;
            let now = Instant::now();
            if next_frame_at < now {
                // The frame deadline must be bounded to the present time because any frame processing
                // lag or sleep overshoots can leave the target time in the past. Left unchecked, this
                // creates a tight, 100% CPU-bound catch-up loop that drops audio frames and completely
                // starves the current-thread async executor of time to poll UDP commands (like Menu presses).
                next_frame_at = now;
            }

            if self.fast_forwarding {
                tokio::task::yield_now().await;
                continue;
            }

            let sleep_until = tokio::time::Instant::from_std(next_frame_at);
            match {
                #[cfg(unix)]
                {
                    wait_for_next_frame_or_control(
                        sleep_until,
                        &mut ctrl_c,
                        &mut sigterm,
                        &mut control_rx,
                    )
                    .await
                }
                #[cfg(not(unix))]
                {
                    wait_for_next_frame_or_control(sleep_until, &mut ctrl_c, &mut control_rx).await
                }
            } {
                LoopWait::Frame => {}
                LoopWait::Signal => {
                    shutdown_reason = "signal received";
                    break;
                }
                LoopWait::Control(event) => {
                    self.apply_control_event(event)?;
                    self.apply_scale(driver.video())?;
                }
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
        self.audio_producer = None;
        command_server.abort();
        Ok(())
    }

    fn unload_game(&mut self) {
        if self.core.is_some()
            && self.config.autosave
            && let Err(err) = self.save_state_slot(-1)
        {
            warn!("Failed to autosave state: {}", err);
        }
        if let Some(core) = self.core.take() {
            if let Err(err) = self.save_sram(&core) {
                warn!("Failed to save SRAM: {}", err);
            }
            core.unload_game();
        }
        self.resolved_rom = None;
    }

    fn load_sram(&self) -> Result<()> {
        let core = self.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
        save::load_sram(core, &self.paths)
    }

    fn save_sram(&self, core: &Core) -> Result<()> {
        save::save_sram(core, &self.paths)
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub(crate) fn save_state(&self) -> Result<()> {
        self.save_state_slot(self.state_slot)
    }

    fn save_state_slot(&self, slot: i8) -> Result<()> {
        let core = self.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
        save::save_state_slot(core, &self.paths, slot)
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub(crate) fn load_state(&self) -> Result<()> {
        self.load_state_slot(self.state_slot)
    }

    fn load_state_slot(&self, slot: i8) -> Result<()> {
        let core = self.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))?;
        save::load_state_slot(core, &self.paths, slot)
    }

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

    fn apply_scale(&self, video: &mut dyn VideoBackend) -> Result<()> {
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

    fn dump_captured_frame(&self) -> Result<()> {
        let Some(path) = &self.args.dump_frame else {
            return Ok(());
        };

        let frame = self
            .captured_frame
            .as_ref()
            .ok_or_else(|| anyhow!("No frame captured"))?;
        let ppm_data = match self.pixel_format {
            Some(VideoFrameFormat::Rgb565) => video::ppm::encode_rgb565(frame)?,
            Some(VideoFrameFormat::Xrgb8888) => video::ppm::encode_xrgb8888(frame)?,
            None => return Err(anyhow!("Frame dump requires a supported pixel format")),
        };

        fs::write(path, ppm_data)?;
        info!("Frame dumped to {:?}", path);
        Ok(())
    }


    fn apply_control_event(&mut self, event: ControlEvent) -> Result<()> {
        event.apply(self)
    }
}

fn frame_interval(fps: f64) -> Result<Duration> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(anyhow!("Core reported invalid FPS: {}", fps));
    }

    Ok(Duration::from_secs_f64(1.0 / fps))
}


enum LoopWait {
    Frame,
    Signal,
    Control(ControlEvent),
}

#[cfg(unix)]
async fn wait_for_next_frame_or_control<F>(
    deadline: tokio::time::Instant,
    ctrl_c: &mut std::pin::Pin<&mut F>,
    sigterm: &mut tokio::signal::unix::Signal,
    control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
) -> LoopWait
where
    F: std::future::Future<Output = std::io::Result<()>>,
{
    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => LoopWait::Frame,
        _ = ctrl_c.as_mut() => LoopWait::Signal,
        _ = sigterm.recv() => LoopWait::Signal,
        event = control_rx.recv() => event.map_or(LoopWait::Signal, LoopWait::Control),
    }
}

#[cfg(not(unix))]
async fn wait_for_next_frame_or_control<F>(
    deadline: tokio::time::Instant,
    ctrl_c: &mut std::pin::Pin<&mut F>,
    control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
) -> LoopWait
where
    F: std::future::Future<Output = std::io::Result<()>>,
{
    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => LoopWait::Frame,
        _ = ctrl_c.as_mut() => LoopWait::Signal,
        event = control_rx.recv() => event.map_or(LoopWait::Signal, LoopWait::Control),
    }
}

// Callback methods mutate session state instead of using scattered globals.
impl LibretroCallbacks for PlaySession {
    fn on_environment(&mut self, cmd: c_uint, data: *mut c_void) -> bool {
        match cmd {
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
                if data.is_null() {
                    warn!("Core requested SET_PIXEL_FORMAT with null data");
                    return false;
                }
                let format = unsafe { *(data as *const retro_pixel_format) };
                if format == retro_pixel_format_RETRO_PIXEL_FORMAT_RGB565 {
                    self.pixel_format = Some(VideoFrameFormat::Rgb565);
                    info!("Core set pixel format: RGB565");
                    true
                } else if format == retro_pixel_format_RETRO_PIXEL_FORMAT_XRGB8888 {
                    self.pixel_format = Some(VideoFrameFormat::Xrgb8888);
                    info!("Core set pixel format: XRGB8888");
                    true
                } else {
                    info!("Unsupported pixel format: {}", format);
                    false
                }
            }
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY => self.write_env_path(data, &self.system_dir),
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => self.write_env_path(data, &self.save_dir),
            RETRO_ENVIRONMENT_GET_FASTFORWARDING => {
                self.write_env_bool(data, self.fast_forwarding)
            }
            RETRO_ENVIRONMENT_GET_CAN_DUPE => {
                if data.is_null() {
                    return false;
                }
                info!("Core queried CAN_DUPE: returning true");
                self.write_env_bool(data, true)
            }
            RETRO_ENVIRONMENT_SET_MESSAGE => {
                if data.is_null() {
                    return false;
                }
                info!("Core sent SET_MESSAGE: handled=false (not displayed)");
                false
            }
            _ => {
                if data.is_null() {
                    debug!("Unsupported env cmd={} with null data", cmd);
                } else {
                    debug!("Unsupported env cmd={}", cmd);
                }
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

        self.hud_state.tick_fps();

        if self.hud_state.is_enabled() {
            self.hud_state.update();
            if let Some(format) = self.pixel_format {
                let scale_mode = self.scale_mode;
                let aspect = self.av_info.as_ref().map(|av| av.geometry.aspect_ratio).unwrap_or(0.0);
                
                let frame = self.captured_frame.as_mut().unwrap();
                self.hud_state.draw(
                    &mut frame.data,
                    width,
                    height,
                    pitch,
                    format,
                    scale_mode,
                    aspect,
                );
            }
        }
    }

    fn on_audio_sample(&mut self, left: i16, right: i16) {
        if let Some(producer) = &mut self.audio_producer {
            producer.push_frame(left, right);
        }
    }

    fn on_audio_sample_batch(&mut self, data: *const i16, frames: usize) -> usize {
        if let Some(producer) = &mut self.audio_producer {
            if !data.is_null() {
                let samples = unsafe { std::slice::from_raw_parts(data, frames * 2) };
                producer.push_frames(samples, frames);
            }
        }

        frames
    }

    fn on_input_poll(&mut self) {}

    fn on_input_state(&mut self, port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16 {
        self.joypad_state.input_state(port, device, index, id)
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

    fn write_env_bool(&self, data: *mut c_void, value: bool) -> bool {
        if data.is_null() {
            return false;
        }

        unsafe {
            *(data as *mut bool) = value;
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
mod tests;



