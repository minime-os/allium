use crate::args::Args;
use crate::audio::{AudioProducer, AudioQueue, validate_sample_rate};
use crate::callbacks::{self, LibretroCallbacks};
use crate::config::PlayConfig;
use crate::control::ControlEvent;
use crate::core::Core;
use crate::input::JoypadState;
use crate::libretro_sys::*;
use crate::paths::PlayPaths;
use crate::scale::ScaleMode;
use crate::udp::CommandState;
use crate::video::frame::{CapturedFrame, VideoFrameFormat};
#[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
use crate::video::miyoo::MiyooVideo;
#[cfg(feature = "simulator")]
use crate::video::simulator::SimulatorVideo;
use crate::video::{self, VideoBackend};
use anyhow::{Context, Result, anyhow};
#[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
use common::platform::{DefaultPlatform, Platform};
use log::{debug, info, warn};
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// One session owns the mutable runtime state so callbacks have one place to land.
pub struct PlaySession {
    args: Args,
    paths: PlayPaths,
    config: PlayConfig,
    core: Option<Core>,
    rom_data: Option<Vec<u8>>,
    active_rom_path: Option<PathBuf>,
    extracted_rom_dir: Option<PathBuf>,
    captured_frame: Option<CapturedFrame>,
    pixel_format: Option<VideoFrameFormat>,
    av_info: Option<retro_system_av_info>,
    audio_producer: Option<AudioProducer>,
    joypad_state: JoypadState,
    fast_forwarding: bool,
    paused: bool,
    should_quit: bool,
    scale_mode: ScaleMode,
    state_slot: i8,
    command_state: Arc<CommandState>,
    system_dir: CString,
    save_dir: CString,
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
        Self {
            args,
            paths,
            config,
            core: None,
            rom_data: None,
            active_rom_path: None,
            extracted_rom_dir: None,
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

        let sys_info = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?
            .get_system_info();

        let rom_path = self.resolve_rom_path(&sys_info)?;
        let path_str = rom_path
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
            let data = fs::read(&rom_path)?;
            game_info.data = data.as_ptr() as *const c_void;
            game_info.size = data.len();
            self.rom_data = Some(data);
        }

        let core = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?;
        core.load_game(&game_info)?;
        self.load_sram()?;
        if self.config.autoload
            && let Err(err) = self.load_state_slot(-1)
        {
            debug!("Autosave autoload skipped: {}", err);
        }

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

    fn resolve_rom_path(&mut self, sys_info: &crate::core::CoreInfo) -> Result<PathBuf> {
        self.active_rom_path = None;
        self.extracted_rom_dir = None;

        if !is_zip_path(&self.paths.rom) || sys_info.block_extract {
            self.active_rom_path = Some(self.paths.rom.clone());
            return Ok(self.paths.rom.clone());
        }

        let extracted = self.extract_zip_rom(&sys_info.valid_extensions)?;
        self.active_rom_path = Some(extracted.clone());
        Ok(extracted)
    }

    fn extract_zip_rom(&mut self, valid_extensions: &str) -> Result<PathBuf> {
        let file = fs::File::open(&self.paths.rom)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let index = find_zip_rom_index(&mut archive, valid_extensions)?;
        let mut entry = archive.by_index(index)?;
        let file_name = Path::new(entry.name())
            .file_name()
            .ok_or_else(|| anyhow!("ZIP ROM entry has no file name"))?
            .to_owned();
        let dir = std::env::temp_dir().join(format!(
            "allium-play-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir)?;
        let path = dir.join(file_name);
        let mut out = fs::File::create(&path)?;
        std::io::copy(&mut entry, &mut out)?;

        info!("Extracted ZIP ROM to {:?}", path);
        self.extracted_rom_dir = Some(dir);
        Ok(path)
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
        #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
        let mut platform = DefaultPlatform::new()?;
        let mut frames_run = 0u64;
        let started_at = Instant::now();
        let mut next_frame_at = started_at;
        let shutdown_reason;
        let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("Failed to install SIGTERM handler")?;
        #[cfg(feature = "simulator")]
        let mut simulator_video = SimulatorVideo::new(
            av_info.geometry.base_width,
            av_info.geometry.base_height,
            av_info.geometry.aspect_ratio,
            self.args.scale,
        )?;
        #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
        let mut miyoo_video = MiyooVideo::new(
            av_info.geometry.base_width,
            av_info.geometry.base_height,
            av_info.geometry.aspect_ratio,
            self.args.scale,
        )?;
        let (mut audio_producer, audio_consumer) = AudioQueue::for_sample_rate(audio_sample_rate);
        audio_producer.set_muted(self.fast_forwarding);
        self.audio_producer = Some(audio_producer);
        #[cfg(feature = "simulator")]
        let _audio = crate::audio::SimulatorAudio::new(audio_sample_rate, audio_consumer)?;
        #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
        let _audio = crate::audio::MiyooAudio::new(audio_sample_rate, audio_consumer)?;
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
                #[cfg(feature = "simulator")]
                self.apply_scale_to_simulator_video(&mut simulator_video)?;
                #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
                self.apply_scale_to_miyoo_video(&mut miyoo_video)?;
            }
            if self.should_quit {
                shutdown_reason = "quit command";
                break;
            }

            #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
            self.poll_platform_input(&mut platform).await;
            if !self.paused {
                self.core
                    .as_ref()
                    .ok_or_else(|| anyhow!("Core not loaded"))?
                    .run();
                frames_run += 1;
            }
            #[cfg(feature = "simulator")]
            if self.present_simulator_frame(&mut simulator_video)? {
                shutdown_reason = "window closed";
                break;
            }
            #[cfg(feature = "simulator")]
            self.apply_simulator_input(&mut simulator_video);
            #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
            self.present_miyoo_frame(&mut miyoo_video)?;
            next_frame_at += frame_interval;

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
                    #[cfg(feature = "simulator")]
                    self.apply_scale_to_simulator_video(&mut simulator_video)?;
                    #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
                    self.apply_scale_to_miyoo_video(&mut miyoo_video)?;
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
        self.cleanup_extracted_rom();
    }

    fn load_sram(&self) -> Result<()> {
        let Some(core) = &self.core else {
            return Ok(());
        };
        let Some((data, size)) = core.memory_region(RETRO_MEMORY_SAVE_RAM) else {
            return Ok(());
        };
        let path = self.paths.sram_path();
        if !path.exists() {
            return Ok(());
        }

        let sram = fs::read(&path)?;
        if sram.len() != size {
            warn!(
                "SRAM size mismatch for {:?}: file={}, core={}",
                path,
                sram.len(),
                size
            );
        }
        let copy_len = sram.len().min(size);
        unsafe {
            ptr::copy_nonoverlapping(sram.as_ptr(), data, copy_len);
        }
        info!("Loaded SRAM from {:?}", path);
        Ok(())
    }

    fn save_sram(&self, core: &Core) -> Result<()> {
        let Some((data, size)) = core.memory_region(RETRO_MEMORY_SAVE_RAM) else {
            return Ok(());
        };
        fs::create_dir_all(&self.paths.save_dir)?;
        let path = self.paths.sram_path();
        let sram = unsafe { std::slice::from_raw_parts(data as *const u8, size) };
        fs::write(&path, sram)?;
        info!("Saved SRAM to {:?}", path);
        Ok(())
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    fn save_state(&self) -> Result<()> {
        self.save_state_slot(self.state_slot)
    }

    fn save_state_slot(&self, slot: i8) -> Result<()> {
        let core = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?;
        let size = core.serialize_size();
        if size == 0 {
            return Err(anyhow!("Core does not support save states"));
        }

        let mut data = vec![0; size];
        if !core.serialize(&mut data) {
            return Err(anyhow!("Core failed to save state"));
        }

        fs::create_dir_all(&self.paths.state_dir)?;
        let path = self.paths.state_path(slot)?;
        fs::write(&path, data)?;
        info!("Saved state slot {} to {:?}", slot, path);
        Ok(())
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    fn load_state(&self) -> Result<()> {
        self.load_state_slot(self.state_slot)
    }

    fn load_state_slot(&self, slot: i8) -> Result<()> {
        let core = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?;
        let path = self.paths.state_path(slot)?;
        let data = fs::read(&path)?;
        if !core.unserialize(&data) {
            return Err(anyhow!("Core failed to load state"));
        }

        info!("Loaded state slot {} from {:?}", slot, path);
        Ok(())
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    fn select_state_slot(&mut self, slot: i8) -> Result<()> {
        if !(-1..=9).contains(&slot) {
            return Err(anyhow!("Save state slot must be between 0 and 9"));
        }

        self.state_slot = slot;
        self.command_state.set_state_slot(slot);
        info!("Selected state slot {}", slot);
        Ok(())
    }

    fn cleanup_extracted_rom(&mut self) {
        if let Some(dir) = self.extracted_rom_dir.take()
            && let Err(err) = fs::remove_dir_all(&dir)
        {
            warn!("Failed to remove extracted ROM dir {:?}: {}", dir, err);
        }
    }

    #[cfg(feature = "simulator")]
    fn present_simulator_frame(&self, video: &mut SimulatorVideo) -> Result<bool> {
        let frame = match &self.captured_frame {
            Some(frame) => frame,
            None => return Ok(false),
        };
        let format = match self.pixel_format {
            Some(format) => format,
            None => return Ok(false),
        };

        let result = video.present(frame, format)?;
        Ok(result.should_quit)
    }

    #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
    fn present_miyoo_frame(&self, video: &mut MiyooVideo) -> Result<()> {
        let frame = match &self.captured_frame {
            Some(frame) => frame,
            None => return Ok(()),
        };
        let format = match self.pixel_format {
            Some(format) => format,
            None => return Ok(()),
        };

        video.present(frame, format)?;
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
        let ppm_data = match self.pixel_format {
            Some(VideoFrameFormat::Rgb565) => video::ppm::encode_rgb565(frame)?,
            Some(VideoFrameFormat::Xrgb8888) => video::ppm::encode_xrgb8888(frame)?,
            None => return Err(anyhow!("Frame dump requires a supported pixel format")),
        };

        fs::write(path, ppm_data)?;
        info!("Frame dumped to {:?}", path);
        Ok(())
    }

    #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
    async fn poll_platform_input(&mut self, platform: &mut DefaultPlatform) {
        while let Ok(key_event) =
            tokio::time::timeout(Duration::from_millis(1), platform.poll()).await
        {
            self.joypad_state.apply(key_event);
        }
    }

    #[cfg(feature = "simulator")]
    fn apply_simulator_input(&mut self, video: &mut SimulatorVideo) {
        for event in video.take_control_events() {
            if let Err(err) = self.apply_control_event(event) {
                warn!("Control event failed: {}", err);
            }
        }
        for key_event in video.take_key_events() {
            self.joypad_state.apply(key_event);
        }
    }

    fn apply_control_event(&mut self, event: ControlEvent) -> Result<()> {
        match event {
            ControlEvent::SaveState => self.save_state(),
            ControlEvent::LoadState => self.load_state(),
            ControlEvent::SaveStateSlot(slot) => {
                self.select_state_slot(slot)?;
                self.save_state()
            }
            ControlEvent::LoadStateSlot(slot) => {
                self.select_state_slot(slot)?;
                self.load_state()
            }
            ControlEvent::SelectStateSlot(slot) => self.select_state_slot(slot),
            ControlEvent::StateSlotPlus => self.select_state_slot((self.state_slot + 1).min(9)),
            ControlEvent::StateSlotMinus => self.select_state_slot((self.state_slot - 1).max(-1)),
            ControlEvent::SetPaused(paused) => {
                self.paused = paused;
                Ok(())
            }
            ControlEvent::TogglePaused => {
                self.paused = !self.paused;
                Ok(())
            }
            ControlEvent::ToggleFastForward => {
                self.fast_forwarding = !self.fast_forwarding;
                if let Some(producer) = &mut self.audio_producer {
                    producer.set_muted(self.fast_forwarding);
                }
                Ok(())
            }
            ControlEvent::SetFastForward(enabled) => {
                self.fast_forwarding = enabled;
                if let Some(producer) = &mut self.audio_producer {
                    producer.set_muted(enabled);
                }
                Ok(())
            }
            ControlEvent::Reset => {
                self.core
                    .as_ref()
                    .ok_or_else(|| anyhow!("Core not loaded"))?
                    .reset();
                Ok(())
            }
            ControlEvent::Quit => {
                self.should_quit = true;
                Ok(())
            }
            ControlEvent::CycleScale => {
                self.scale_mode = self.scale_mode.next();
                info!("Selected scale mode: {:?}", self.scale_mode);
                Ok(())
            }
        }
    }

    #[cfg(feature = "simulator")]
    fn apply_scale_to_simulator_video(&self, video: &mut SimulatorVideo) -> Result<()> {
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

    #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
    fn apply_scale_to_miyoo_video(&self, video: &mut MiyooVideo) -> Result<()> {
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
}

fn frame_interval(fps: f64) -> Result<Duration> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(anyhow!("Core reported invalid FPS: {}", fps));
    }

    Ok(Duration::from_secs_f64(1.0 / fps))
}

fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

fn find_zip_rom_index(
    archive: &mut zip::ZipArchive<fs::File>,
    valid_extensions: &str,
) -> Result<usize> {
    let valid_extensions: Vec<String> = valid_extensions
        .split('|')
        .filter(|extension| !extension.is_empty())
        .map(|extension| extension.to_ascii_lowercase())
        .collect();

    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let Some(extension) = Path::new(entry.name())
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
        else {
            continue;
        };
        if valid_extensions.is_empty()
            || valid_extensions
                .iter()
                .any(|valid_extension| valid_extension == &extension)
        {
            return Ok(index);
        }
    }

    Err(anyhow!("ZIP ROM contains no supported ROM file"))
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
            RETRO_ENVIRONMENT_GET_FASTFORWARDING => self.write_env_bool(data, self.fast_forwarding),
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
            scale: crate::scale::ScaleMode::Aspect,
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
