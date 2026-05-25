use libretro::*;
use crate::config::{Args, PlayConfig};
use crate::settings::FrontendSettings;
use crate::audio::{AudioProducer, AudioQueue, validate_sample_rate};
use crate::commands::{CommandState, ControlEvent};
use crate::hud::HudState;
use crate::input::JoypadState;
use crate::paths::PlayPaths;
use crate::save;
use crate::video::{ScaleMode, CapturedFrame, VideoFrameFormat, FrameData};
use crate::unzip;
use crate::content;
use crate::settings;
use anyhow::{Context, Result, anyhow};
use libloading::Library;
use log::{debug, info, warn};
use std::ffi::{CStr, CString};
use std::os::raw::{c_uint, c_void};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

// libretro gives borrowed C data for core properties; Play copies it so it can log/use it safely later.
pub struct CoreInfo {
    pub library_name: String,
    pub library_version: String,
    pub valid_extensions: String,
    pub need_fullpath: bool,
    #[allow(dead_code)]
    pub block_extract: bool,
}

// Core is the loaded libretro library. It keeps the library alive and exposes Rust methods.
#[allow(dead_code)]
pub struct Core {
    lib: Library,
    symbols: CoreSymbols,
}

struct CoreLifecycleSymbols {
    retro_init: unsafe extern "C" fn(),
    retro_deinit: unsafe extern "C" fn(),
    retro_api_version: unsafe extern "C" fn() -> c_uint,
    retro_get_system_info: unsafe extern "C" fn(info: *mut retro_system_info),
    retro_get_system_av_info: unsafe extern "C" fn(info: *mut retro_system_av_info),
    retro_set_environment: unsafe extern "C" fn(cb: retro_environment_t),
    retro_set_video_refresh: unsafe extern "C" fn(cb: retro_video_refresh_t),
    retro_set_audio_sample: unsafe extern "C" fn(cb: retro_audio_sample_t),
    retro_set_audio_sample_batch: unsafe extern "C" fn(cb: retro_audio_sample_batch_t),
    retro_set_input_poll: unsafe extern "C" fn(cb: retro_input_poll_t),
    retro_set_input_state: unsafe extern "C" fn(cb: retro_input_state_t),
}

struct CoreGameplaySymbols {
    retro_load_game: unsafe extern "C" fn(game: *const retro_game_info) -> bool,
    retro_unload_game: unsafe extern "C" fn(),
    retro_run: unsafe extern "C" fn(),
    retro_reset: unsafe extern "C" fn(),
    retro_serialize_size: unsafe extern "C" fn() -> usize,
    retro_serialize: unsafe extern "C" fn(data: *mut c_void, len: usize) -> bool,
    retro_unserialize: unsafe extern "C" fn(data: *const c_void, len: usize) -> bool,
    retro_get_memory_data: unsafe extern "C" fn(id: c_uint) -> *mut c_void,
    retro_get_memory_size: unsafe extern "C" fn(id: c_uint) -> usize,
}

struct CoreSymbols {
    lifecycle: CoreLifecycleSymbols,
    gameplay: CoreGameplaySymbols,
}

impl Core {
    pub unsafe fn load(path: &Path) -> Result<Self> {
        let lib = unsafe { Library::new(path) }
            .with_context(|| format!("Failed to load core: {}", path.display()))?;
        let symbols = unsafe { CoreSymbols::load(&lib)? };
        symbols.check_api_version()?;
        symbols.install_callbacks();
        Ok(Self { lib, symbols })
    }

    pub fn init(&self) {
        self.symbols.init();
    }
}

impl CoreLifecycleSymbols {
    unsafe fn load(lib: &Library) -> Result<Self> {
        unsafe {
            Ok(Self {
                retro_init: load_symbol(lib, b"retro_init")?,
                retro_deinit: load_symbol(lib, b"retro_deinit")?,
                retro_api_version: load_symbol(lib, b"retro_api_version")?,
                retro_get_system_info: load_symbol(lib, b"retro_get_system_info")?,
                retro_get_system_av_info: load_symbol(lib, b"retro_get_system_av_info")?,
                retro_set_environment: load_symbol(lib, b"retro_set_environment")?,
                retro_set_video_refresh: load_symbol(lib, b"retro_set_video_refresh")?,
                retro_set_audio_sample: load_symbol(lib, b"retro_set_audio_sample")?,
                retro_set_audio_sample_batch: load_symbol(lib, b"retro_set_audio_sample_batch")?,
                retro_set_input_poll: load_symbol(lib, b"retro_set_input_poll")?,
                retro_set_input_state: load_symbol(lib, b"retro_set_input_state")?,
            })
        }
    }
}

impl CoreGameplaySymbols {
    unsafe fn load(lib: &Library) -> Result<Self> {
        unsafe {
            Ok(Self {
                retro_load_game: load_symbol(lib, b"retro_load_game")?,
                retro_unload_game: load_symbol(lib, b"retro_unload_game")?,
                retro_run: load_symbol(lib, b"retro_run")?,
                retro_reset: load_symbol(lib, b"retro_reset")?,
                retro_serialize_size: load_symbol(lib, b"retro_serialize_size")?,
                retro_serialize: load_symbol(lib, b"retro_serialize")?,
                retro_unserialize: load_symbol(lib, b"retro_unserialize")?,
                retro_get_memory_data: load_symbol(lib, b"retro_get_memory_data")?,
                retro_get_memory_size: load_symbol(lib, b"retro_get_memory_size")?,
            })
        }
    }
}

impl CoreSymbols {
    unsafe fn load(lib: &Library) -> Result<Self> {
        let lifecycle = unsafe { CoreLifecycleSymbols::load(lib)? };
        let gameplay = unsafe { CoreGameplaySymbols::load(lib)? };
        Ok(Self { lifecycle, gameplay })
    }

    fn check_api_version(&self) -> Result<()> {
        let api_version = unsafe { (self.lifecycle.retro_api_version)() };
        if api_version != RETRO_API_VERSION {
            return Err(anyhow!("Unsupported libretro API version: {}", api_version));
        }
        Ok(())
    }

    fn install_callbacks(&self) {
        unsafe {
            (self.lifecycle.retro_set_environment)(Some(environment_callback));
            (self.lifecycle.retro_set_video_refresh)(Some(video_refresh_callback));
            (self.lifecycle.retro_set_audio_sample)(Some(audio_sample_callback));
            (self.lifecycle.retro_set_audio_sample_batch)(Some(audio_sample_batch_callback));
            (self.lifecycle.retro_set_input_poll)(Some(input_poll_callback));
            (self.lifecycle.retro_set_input_state)(Some(input_state_callback));
        }
    }

    fn init(&self) {
        unsafe { (self.lifecycle.retro_init)(); }
    }

    fn deinit(&self) {
        unsafe { (self.lifecycle.retro_deinit)(); }
    }
}

impl Core {
    pub fn load_game(&self, info: &retro_game_info) -> Result<()> {
        if unsafe { (self.symbols.gameplay.retro_load_game)(info) } {
            Ok(())
        } else {
            Err(anyhow!("Failed to load game"))
        }
    }

    pub fn unload_game(&self) {
        unsafe { (self.symbols.gameplay.retro_unload_game)() };
    }

    pub fn run(&self) {
        unsafe { (self.symbols.gameplay.retro_run)() };
    }

    pub fn reset(&self) {
        unsafe { (self.symbols.gameplay.retro_reset)() };
    }

    pub fn serialize_size(&self) -> usize {
        unsafe { (self.symbols.gameplay.retro_serialize_size)() }
    }

    pub fn serialize(&self, data: &mut [u8]) -> bool {
        unsafe { (self.symbols.gameplay.retro_serialize)(data.as_mut_ptr() as *mut c_void, data.len()) }
    }

    pub fn unserialize(&self, data: &[u8]) -> bool {
        unsafe { (self.symbols.gameplay.retro_unserialize)(data.as_ptr() as *const c_void, data.len()) }
    }

    pub fn memory_region(&self, id: c_uint) -> Option<(*mut u8, usize)> {
        let size = unsafe { (self.symbols.gameplay.retro_get_memory_size)(id) };
        if size == 0 { return None; }
        let data = unsafe { (self.symbols.gameplay.retro_get_memory_data)(id) };
        if data.is_null() { return None; }
        Some((data as *mut u8, size))
    }

    pub fn get_system_av_info(&self) -> retro_system_av_info {
        let mut info = retro_system_av_info {
            geometry: retro_game_geometry {
                base_width: 0, base_height: 0, max_width: 0, max_height: 0, aspect_ratio: 0.0,
            },
            timing: retro_system_timing { fps: 0.0, sample_rate: 0.0 },
        };
        unsafe { (self.symbols.lifecycle.retro_get_system_av_info)(&mut info) };
        info
    }

    pub fn get_system_info(&self) -> CoreInfo {
        let mut info = retro_system_info {
            library_name: ptr::null(), library_version: ptr::null(),
            valid_extensions: ptr::null(), need_fullpath: false, block_extract: false,
        };
        unsafe {
            (self.symbols.lifecycle.retro_get_system_info)(&mut info);
            CoreInfo {
                library_name: c_string(info.library_name),
                library_version: c_string(info.library_version),
                valid_extensions: c_string(info.valid_extensions),
                need_fullpath: info.need_fullpath,
                block_extract: info.block_extract,
            }
        }
    }
}

unsafe fn load_symbol<T>(lib: &Library, name: &[u8]) -> Result<T>
where T: Copy,
{
    unsafe { lib.get::<T>(name) }
        .map(|symbol| *symbol)
        .with_context(|| format!("Missing libretro symbol: {}", String::from_utf8_lossy(name)))
}

unsafe fn c_string(value: *const std::os::raw::c_char) -> String {
    if value.is_null() { String::new() } else {
        unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.symbols.deinit();
    }
}

// ---- PlayContext: immutable session setup ----

fn to_cstring(path: &std::path::Path) -> CString {
    CString::new(path.to_string_lossy().into_owned()).expect("Path must not contain NUL")
}

pub struct PlayContext {
    pub args: Args,
    pub paths: PlayPaths,
    pub config: PlayConfig,
    pub system_dir: CString,
    pub save_dir: CString,
}

impl PlayContext {
    pub fn new(args: Args) -> Self {
        let paths = PlayPaths::from_args(&args);
        let config = PlayConfig::load().unwrap_or_default();
        Self {
            system_dir: to_cstring(&paths.config_dir),
            save_dir: to_cstring(&paths.save_dir),
            args,
            paths,
            config,
        }
    }
}

// ---- ActiveSession: mutable runtime state (no Option soup) ----

pub struct ActiveSession {
    pub ctx: PlayContext,
    pub core: Core,
    pub av_info: retro_system_av_info,
    pub pixel_format: VideoFrameFormat,
    pub audio_producer: AudioProducer,
    pub joypad_state: JoypadState,
    pub fast_forwarding: bool,
    pub paused: bool,
    pub should_quit: bool,
    pub scale_mode: ScaleMode,
    pub state_slot: i8,
    pub command_state: Arc<CommandState>,
    pub hud_state: HudState,
    pub host_cpu: f64,
    pub captured_frame: CapturedFrame,
    pub frontend_settings: FrontendSettings,
    /// Raw pointer from the most recent libretro video_refresh callback.
    /// Only valid between the callback and the next retro_run() call.
    pub(crate) last_raw_frame: Option<(*const u8, u32, u32, usize)>,
    pub resolved_rom: unzip::ResolvedRom,
}

impl ActiveSession {
    pub fn new_bare(ctx: PlayContext) -> Result<(Self, crate::audio::AudioConsumer)> {
        std::fs::create_dir_all(&ctx.paths.config_dir)?;
        std::fs::create_dir_all(&ctx.paths.save_dir)?;

        let core = unsafe { Core::load(&ctx.paths.core_path)? };
        let info = core.get_system_info();
        info!("Core loaded: {} ({})", info.library_name, info.library_version);
        info!("Extensions: {}", info.valid_extensions);
        info!("need_fullpath={}, block_extract={}", info.need_fullpath, info.block_extract);

        let (game_info, resolved, _rom_data, _rom_path) =
            content::resolve_and_prepare_rom(&ctx.paths, &info)?;

        let hud = ctx.args.hud || ctx.config.hud || std::env::var("ALLIUM_HUD").is_ok();
        let frontend_settings = settings::load_frontend_settings(&ctx.paths);
        info!(
            "Frontend settings loaded: scale={:?}, effect={:?}, sharpness={:?}",
            frontend_settings.scale_mode,
            frontend_settings.effect,
            frontend_settings.sharpness,
        );
        let resolved_rom = resolved.unwrap_or_else(|| unzip::ResolvedRom {
            active_path: ctx.paths.rom.clone(),
            extracted_dir: None,
        });

        let (temp_prod, _) = AudioQueue::for_sample_rate(48000);

        let mut session = Self {
            ctx,
            core,
            av_info: retro_system_av_info {
                geometry: retro_game_geometry { base_width: 0, base_height: 0, max_width: 0, max_height: 0, aspect_ratio: 0.0 },
                timing: retro_system_timing { fps: 0.0, sample_rate: 0.0 },
            },
            pixel_format: VideoFrameFormat::Rgb565,
            audio_producer: temp_prod,
            joypad_state: JoypadState::new(),
            fast_forwarding: false,
            paused: false,
            should_quit: false,
            scale_mode: frontend_settings.scale_mode,
            state_slot: 0,
            command_state: CommandState::new(0),
            hud_state: HudState::new(hud),
            host_cpu: 0.0,
            captured_frame: CapturedFrame::new_empty(),
            frontend_settings,
            last_raw_frame: None,
            resolved_rom,
        };

        unsafe {
            crate::core::set_handler(&mut session);
        }

        session.core.init();
        session.core.load_game(&game_info)?;

        save::load_sram(&session.core, &session.ctx.paths)?;
        if session.ctx.config.autoload {
            if let Err(err) = save::load_state_slot(&session.core, &session.ctx.paths, -1) {
                debug!("Autosave autoload skipped: {}", err);
            }
        }

        let av_info = session.core.get_system_av_info();
        info!(
            "AV Info: {}x{} @ {} fps, sample_rate: {}",
            av_info.geometry.base_width, av_info.geometry.base_height,
            av_info.timing.fps, av_info.timing.sample_rate
        );

        let sample_rate = validate_sample_rate(av_info.timing.sample_rate)?;
        let (prod, cons) = AudioQueue::for_sample_rate(sample_rate);

        session.av_info = av_info;
        session.audio_producer = prod;

        Ok((session, cons))
    }
    pub fn new(_ctx: PlayContext) -> Result<(Self, crate::audio::AudioConsumer)> {
        // Kept for backward compat during refactor; delegates to new_bare.
        Self::new_bare(_ctx)
    }

    pub fn emulate_single_frame(&mut self,
        frames_run: &mut u64,
    ) {
        if !self.paused {
            self.core.run();
            *frames_run += 1;
            self.hud_state.tick_cpu();
        }
    }

    pub fn present_captured_frame(
        &mut self,
        drv: &mut crate::platform::DefaultPlatform,
    ) -> Result<bool> {
        if self.paused && drv.skip_presentation_when_paused() {
            return Ok(false);
        }

        // When HUD is off, present directly from the libretro callback buffer
        // to avoid the ~0.1–0.2 ms per-frame copy into captured_frame.data.
        if let Some((ptr, width, height, pitch)) = self.last_raw_frame.take() {
            let borrowed_data = unsafe {
                std::slice::from_raw_parts(ptr, height as usize * pitch)
            };
            let view = CapturedFrame::new(
                FrameData::borrowed(borrowed_data.as_ptr(), borrowed_data.len()),
                width,
                height,
                pitch,
            );
            return Ok(drv.video.present(&view, self.pixel_format)?);
        }

        if self.captured_frame.width == 0 {
            return Ok(false);
        }
        Ok(drv.video.present(&self.captured_frame, self.pixel_format)?)
    }

    pub fn apply_scale(
        &self,
        drv: &mut crate::platform::DefaultPlatform,
    ) -> Result<()> {
        drv.video.set_scale(
            self.scale_mode,
            self.av_info.geometry.base_width,
            self.av_info.geometry.base_height,
            self.av_info.geometry.aspect_ratio,
        )
    }

    // Apply all current frontend_settings to the running session and platform.
    // Called after platform creation and on RELOAD_CONFIG.
    pub fn apply_frontend_settings(&mut self, drv: &mut crate::platform::DefaultPlatform) -> Result<()> {
        self.scale_mode = self.frontend_settings.scale_mode;
        self.apply_scale(drv)?;
        drv.video.set_effect(self.frontend_settings.effect);
        drv.video.set_sharpness(self.frontend_settings.sharpness);
        self.hud_state.set_enabled(self.frontend_settings.debug_hud);

        #[cfg(feature = "miyoo")]
        {
            let governor = match self.frontend_settings.cpu_speed {
                settings::CpuSpeed::Powersave => "powersave",
                settings::CpuSpeed::Normal => "ondemand",
                settings::CpuSpeed::Performance => "performance",
            };
            crate::platform::miyoo::set_governor(governor);
        }

        info!(
            "Applied frontend settings: scale={:?} effect={:?} sharpness={:?} tearing={:?} cpu={:?} thread_video={:?} debug_hud={:?} max_ff={}",
            self.frontend_settings.scale_mode,
            self.frontend_settings.effect,
            self.frontend_settings.sharpness,
            self.frontend_settings.tearing,
            self.frontend_settings.cpu_speed,
            self.frontend_settings.thread_video,
            self.frontend_settings.debug_hud,
            self.frontend_settings.max_ff_speed,
        );
        Ok(())
    }

    pub fn apply_control_event(&mut self,
        event: ControlEvent,
        drv: &mut crate::platform::DefaultPlatform,
    ) -> Result<()> {
        match event {
            ControlEvent::SaveState => {
                save::save_state_slot(&self.core, &self.ctx.paths, self.state_slot)?;
            }
            ControlEvent::LoadState => {
                save::load_state_slot(&self.core, &self.ctx.paths, self.state_slot)?;
            }
            ControlEvent::SaveStateSlot(slot) => {
                self.select_state_slot(slot)?;
                save::save_state_slot(&self.core, &self.ctx.paths, slot)?;
            }
            ControlEvent::LoadStateSlot(slot) => {
                self.select_state_slot(slot)?;
                save::load_state_slot(&self.core, &self.ctx.paths, slot)?;
            }
            ControlEvent::SelectStateSlot(slot) => self.select_state_slot(slot)?,
            ControlEvent::StateSlotPlus => self.select_state_slot((self.state_slot + 1).min(9))?,
            ControlEvent::StateSlotMinus => self.select_state_slot((self.state_slot - 1).max(-1))?,
            ControlEvent::SetPaused(paused) => self.paused = paused,
            ControlEvent::TogglePaused => self.paused = !self.paused,
            ControlEvent::ToggleFastForward => {
                self.fast_forwarding = !self.fast_forwarding;
                self.set_audio_muted(self.fast_forwarding);
            }
            ControlEvent::SetFastForward(enabled) => {
                self.fast_forwarding = enabled;
                self.set_audio_muted(enabled);
            }
            ControlEvent::Reset => self.core.reset(),
            ControlEvent::Quit => self.should_quit = true,
            ControlEvent::CycleScale => {
                self.scale_mode = self.scale_mode.next();
                info!("Selected scale mode: {:?}", self.scale_mode);
                self.apply_scale(drv)?;
            }
            // Settings events (Stage S3 — stored and applied immediately)
            ControlEvent::SetScale(mode) => {
                if let Ok(v) = mode.parse() {
                    self.frontend_settings.scale_mode = v;
                    info!("SET_SCALE: {:?}", v);
                } else {
                    warn!("Invalid SET_SCALE: {}", mode);
                }
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetEffect(mode) => {
                if let Ok(v) = mode.parse() {
                    self.frontend_settings.effect = v;
                    info!("SET_EFFECT: {:?}", v);
                } else {
                    warn!("Invalid SET_EFFECT: {}", mode);
                }
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetSharpness(mode) => {
                if let Ok(v) = mode.parse() {
                    self.frontend_settings.sharpness = v;
                    info!("SET_SHARPNESS: {:?}", v);
                } else {
                    warn!("Invalid SET_SHARPNESS: {}", mode);
                }
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetTearing(mode) => {
                if let Ok(v) = mode.parse() {
                    self.frontend_settings.tearing = v;
                    info!("SET_TEARING: {:?}", v);
                } else {
                    warn!("Invalid SET_TEARING: {}", mode);
                }
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetOverclock(mode) => {
                if let Ok(v) = mode.parse() {
                    self.frontend_settings.cpu_speed = v;
                    info!("SET_OVERCLOCK: {:?}", v);
                } else {
                    warn!("Invalid SET_OVERCLOCK: {}", mode);
                }
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetThreadVideo(enabled) => {
                self.frontend_settings.thread_video = enabled;
                info!("SET_THREAD_VIDEO: {}", enabled);
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetDebugHUD(enabled) => {
                self.frontend_settings.debug_hud = enabled;
                info!("SET_DEBUG_HUD: {}", enabled);
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetMaxFF(speed) => {
                self.frontend_settings.max_ff_speed = speed.min(8).max(1);
                info!("SET_MAX_FF: {}", self.frontend_settings.max_ff_speed);
                self.apply_frontend_settings(drv)?;
            }
            ControlEvent::SetCoreOption { key, value } => {
                info!("TODO apply SET_CORE_OPTION: {} = {}", key, value);
            }
            ControlEvent::ReloadConfig => {
                info!("RELOAD_CONFIG");
                self.frontend_settings = settings::load_frontend_settings(&self.ctx.paths);
                self.apply_frontend_settings(drv)?;
            }
        }
        Ok(())
    }

    pub fn select_state_slot(&mut self, slot: i8) -> Result<()> {
        if !(-1..=9).contains(&slot) {
            return Err(anyhow!("Save state slot must be between 0 and 9"));
        }
        self.state_slot = slot;
        self.command_state.set_state_slot(slot);
        info!("Selected state slot {}", slot);
        Ok(())
    }

    pub fn set_audio_muted(&mut self, muted: bool) {
        self.audio_producer.set_muted(muted);
    }
}

impl Drop for ActiveSession {
    fn drop(&mut self) {
        if self.ctx.config.autosave {
            if let Err(err) = save::save_state_slot(&self.core, &self.ctx.paths, -1) {
                warn!("Failed to autosave state: {}", err);
            }
        }
        if let Err(err) = save::save_sram(&self.core, &self.ctx.paths) {
            warn!("Failed to save SRAM: {}", err);
        }
        self.core.unload_game();
    }
}

// ---- Callback methods (called from callbacks.rs via global handler) ----

impl ActiveSession {
    pub(crate) fn on_environment(&mut self, cmd: c_uint, data: *mut c_void,
    ) -> bool {
        let result = match cmd {
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => self.set_pixel_format(data),
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY => self.write_env_path(data, &self.ctx.system_dir),
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => self.write_env_path(data, &self.ctx.save_dir),
            RETRO_ENVIRONMENT_GET_FASTFORWARDING => self.write_env_bool(data, self.fast_forwarding),
            RETRO_ENVIRONMENT_GET_CAN_DUPE => self.write_env_bool(data, true),
            RETRO_ENVIRONMENT_SET_MESSAGE => self.set_message(data),
            _ => self.handle_unsupported_env(cmd, data),
        };
        debug!("env cmd={cmd} result={result}");
        result
    }

    pub(crate) fn on_video_refresh(
        &mut self,
        data: *const c_void,
        width: c_uint,
        height: c_uint,
        pitch: usize,
    ) {
        if data.is_null() {
            return;
        }

        // Store the raw pointer so we can present directly from it when HUD is off.
        self.last_raw_frame = Some((data as *const u8, width, height, pitch));
        self.hud_state.tick_fps();

        // Only copy and draw HUD when the overlay is actually enabled.
        if self.hud_state.is_enabled() {
            self.copy_refresh_frame(data, width, height, pitch);
            self.draw_refresh_hud(width, height, pitch);
        }
    }

    pub(crate) fn on_audio_sample(&mut self, left: i16, right: i16) {
        self.audio_producer.push_frame(left, right);
    }

    pub(crate) fn on_audio_sample_batch(&mut self, data: *const i16, frames: usize) -> usize {
        if !data.is_null() {
            let samples = unsafe { std::slice::from_raw_parts(data, frames * 2) };
            self.audio_producer.push_frames(samples, frames);
        }
        frames
    }

    pub(crate) fn on_input_poll(&mut self) {}

    pub(crate) fn on_input_state(&self, port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16 {
        self.joypad_state.input_state(port, device, index, id)
    }

    fn set_pixel_format(&mut self, data: *mut c_void) -> bool {
        if data.is_null() {
            warn!("Core requested SET_PIXEL_FORMAT with null data");
            return false;
        }
        let format = unsafe { *(data as *const retro_pixel_format) };
        if format == retro_pixel_format_RETRO_PIXEL_FORMAT_RGB565 {
            self.pixel_format = VideoFrameFormat::Rgb565;
            info!("Core set pixel format: RGB565");
            true
        } else if format == retro_pixel_format_RETRO_PIXEL_FORMAT_XRGB8888 {
            self.pixel_format = VideoFrameFormat::Xrgb8888;
            info!("Core set pixel format: XRGB8888");
            true
        } else {
            info!("Unsupported pixel format: {format}");
            false
        }
    }

    fn set_message(&self, data: *mut c_void) -> bool {
        if data.is_null() { return false; }
        info!("Core sent SET_MESSAGE: handled=false (not displayed)");
        false
    }

    fn handle_unsupported_env(&self, cmd: c_uint, data: *mut c_void) -> bool {
        if data.is_null() {
            debug!("Unsupported env cmd={cmd} with null data");
        } else {
            debug!("Unsupported env cmd={cmd}");
        }
        false
    }

    fn copy_refresh_frame(
        &mut self,
        data: *const c_void,
        width: c_uint,
        height: c_uint,
        pitch: usize,
    ) {
        let size = pitch * height as usize;
        let frame = &mut self.captured_frame;
        frame.width = width;
        frame.height = height;
        frame.pitch = pitch;

        // Ensure we have an owned buffer of the right size.
        let needs_new = match &frame.data {
            FrameData::Owned(v) => v.len() != size,
            FrameData::Borrowed { .. } => true,
        };
        if needs_new {
            frame.data = FrameData::Owned(vec![0; size]);
        }

        if let FrameData::Owned(v) = &mut frame.data {
            unsafe {
                ptr::copy_nonoverlapping(data as *const u8, v.as_mut_ptr(), size);
            }
        } else {
            unreachable!("copy_refresh_frame: data should be Owned after sizing");
        }
    }

    fn draw_refresh_hud(&mut self,
        width: c_uint,
        height: c_uint,
        pitch: usize,
    ) {
        if self.hud_state.is_enabled() {
            self.hud_state.update(self.host_cpu);
            let aspect = self.av_info.geometry.aspect_ratio;
            self.hud_state.draw(
                &mut self.captured_frame.data,
                width,
                height,
                pitch,
                self.pixel_format,
                self.scale_mode,
                aspect,
            );
        }
    }

    fn write_env_path(&self, data: *mut c_void, path: &CString,
    ) -> bool {
        if data.is_null() { return false; }
        unsafe { *(data as *mut *const std::os::raw::c_char) = path.as_ptr(); }
        true
    }

    fn write_env_bool(&self, data: *mut c_void, value: bool) -> bool {
        if data.is_null() { return false; }
        unsafe { *(data as *mut bool) = value; }
        true
    }
}

// ---- C callback bridge ----
// libretro cores call plain C function pointers, not Rust methods.
// This module provides the static global and shim functions that route
// core callbacks back into the active ActiveSession.

static mut CALLBACK_HANDLER: Option<*mut ActiveSession> = None;

/// Registers the active callback handler globally.
/// This must be called before running a session so core events find a valid destination.
pub unsafe fn set_handler(handler: *mut ActiveSession) {
    unsafe { CALLBACK_HANDLER = Some(handler); }
}

/// Unregisters the globally active callback handler.
/// This must be called immediately when a session ends to avoid stale dangling pointers.
pub unsafe fn clear_handler() {
    unsafe { CALLBACK_HANDLER = None; }
}

pub unsafe extern "C" fn environment_callback(cmd: c_uint, data: *mut c_void) -> bool {
    unsafe { with_session(|h| h.on_environment(cmd, data)).unwrap_or(false) }
}

pub unsafe extern "C" fn video_refresh_callback(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    unsafe { with_session(|h| h.on_video_refresh(data, width, height, pitch)) };
}

pub unsafe extern "C" fn audio_sample_callback(left: i16, right: i16) {
    unsafe { with_session(|h| h.on_audio_sample(left, right)) };
}

pub unsafe extern "C" fn audio_sample_batch_callback(data: *const i16, frames: usize) -> usize {
    unsafe { with_session(|h| h.on_audio_sample_batch(data, frames)).unwrap_or(0) }
}

pub unsafe extern "C" fn input_poll_callback() {
    unsafe { with_session(|h| h.on_input_poll()) };
}

pub unsafe extern "C" fn input_state_callback(
    port: c_uint,
    device: c_uint,
    index: c_uint,
    id: c_uint,
) -> i16 {
    unsafe { with_session(|h| h.on_input_state(port, device, index, id)).unwrap_or(0) }
}

unsafe fn with_session<T>(f: impl FnOnce(&mut ActiveSession) -> T) -> Option<T> {
    unsafe { CALLBACK_HANDLER.and_then(|handler| handler.as_mut()).map(f) }
}

