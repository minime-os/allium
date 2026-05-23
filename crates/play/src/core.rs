use crate::libretro_sys::*;
use crate::config::Args;
use crate::audio::AudioProducer;
use crate::config::PlayConfig;
use crate::diagnostics;
use crate::input::JoypadState;
use crate::paths::PlayPaths;
use crate::save;
use crate::video::ScaleMode;
use crate::video::{CapturedFrame, VideoFrameFormat};
use crate::platform::{DefaultPlatform, EmulationPlatform, VideoBackend};
use crate::unzip;
use crate::content;
use anyhow::{Context, Result, anyhow};
use libloading::Library;
use log::{debug, info, warn};
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::c_uint;
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;
use std::sync::Arc;

// libretro gives borrowed C data for core properties; Play copies it so it can log/use it safely later.
pub struct CoreInfo {
    pub library_name: String,     // Example: "Snes9x", from snes9x_libretro.so.
    pub library_version: String,  // Example: "1.62.3".
    pub valid_extensions: String, // ROM extensions this core can load, like "sfc|smc|zip".
    pub need_fullpath: bool,      // true: pass a ROM path; false: read ROM bytes into memory.
    #[allow(dead_code)]
    pub block_extract: bool, // true: pass archives through instead of extracting first.
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

// CoreSymbols is the raw libretro function table loaded from the dynamic library.
struct CoreSymbols {
    lifecycle: CoreLifecycleSymbols,
    gameplay: CoreGameplaySymbols,
}

impl Core {
    // Load flow: open library -> load symbols -> check API -> install callbacks -> init core.
    pub unsafe fn load(path: &Path) -> Result<Self> {
        // Step 1: open the dynamic library file that contains the libretro core.
        let lib = unsafe { Library::new(path) }
            .with_context(|| format!("Failed to load core: {}", path.display()))?;
        let symbols = unsafe { CoreSymbols::load(&lib)? };

        symbols.check_api_version()?;
        symbols.install_callbacks();
        symbols.init();

        Ok(Self { lib, symbols })
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
    // Step 2: after the library is open, load every libretro function Play needs.
    unsafe fn load(lib: &Library) -> Result<Self> {
        let lifecycle = unsafe { CoreLifecycleSymbols::load(lib)? };
        let gameplay = unsafe { CoreGameplaySymbols::load(lib)? };
        Ok(Self { lifecycle, gameplay })
    }

    // Step 3: reject cores using an API version Play does not understand.
    fn check_api_version(&self) -> Result<()> {
        let api_version = unsafe { (self.lifecycle.retro_api_version)() };
        if api_version != RETRO_API_VERSION {
            return Err(anyhow!("Unsupported libretro API version: {}", api_version));
        }

        Ok(())
    }

    // Step 4: give the core Play's callback functions before retro_init.
    fn install_callbacks(&self) {
        use crate::callbacks::*;
        unsafe {
            (self.lifecycle.retro_set_environment)(Some(environment_callback));
            (self.lifecycle.retro_set_video_refresh)(Some(video_refresh_callback));
            (self.lifecycle.retro_set_audio_sample)(Some(audio_sample_callback));
            (self.lifecycle.retro_set_audio_sample_batch)(Some(audio_sample_batch_callback));
            (self.lifecycle.retro_set_input_poll)(Some(input_poll_callback));
            (self.lifecycle.retro_set_input_state)(Some(input_state_callback));
        }
    }

    // Step 5: initialize the core after the frontend callbacks are installed.
    fn init(&self) {
        unsafe {
            (self.lifecycle.retro_init)();
        }
    }

    // Drop path: retro_deinit lets the core clean up while its code is still loaded.
    fn deinit(&self) {
        unsafe {
            (self.lifecycle.retro_deinit)();
        }
    }
}

impl Core {
    // Rust wrappers around libretro function pointers.
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
        if size == 0 {
            return None;
        }

        let data = unsafe { (self.symbols.gameplay.retro_get_memory_data)(id) };
        if data.is_null() {
            return None;
        }

        Some((data as *mut u8, size))
    }

    // AV info can depend on loaded content, so query it after retro_load_game.
    pub fn get_system_av_info(&self) -> retro_system_av_info {
        let mut info = retro_system_av_info {
            geometry: retro_game_geometry {
                base_width: 0,
                base_height: 0,
                max_width: 0,
                max_height: 0,
                aspect_ratio: 0.0,
            },
            timing: retro_system_timing {
                fps: 0.0,
                sample_rate: 0.0,
            },
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

// Missing symbols are common FFI failures; naming the symbol makes them diagnosable.
unsafe fn load_symbol<T>(lib: &Library, name: &[u8]) -> Result<T>
where
    T: Copy,
{
    unsafe { lib.get::<T>(name) }
        .map(|symbol| *symbol)
        .with_context(|| format!("Missing libretro symbol: {}", String::from_utf8_lossy(name)))
}

// Null strings should not crash logging or metadata display.
unsafe fn c_string(value: *const std::os::raw::c_char) -> String {
    if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.symbols.deinit();
    }
}

// ---- PlaySession: state struct + lifecycle ----

// UDP command state shared between the session and the async command server.
pub struct CommandState {
    pub(crate) state_slot: std::sync::atomic::AtomicI8,
}

impl CommandState {
    pub fn new(state_slot: i8) -> Arc<Self> {
        Arc::new(Self {
            state_slot: std::sync::atomic::AtomicI8::new(state_slot),
        })
    }

    pub fn set_state_slot(&self, state_slot: i8) {
        self.state_slot.store(state_slot, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn state_slot(&self) -> i8 {
        self.state_slot.load(std::sync::atomic::Ordering::Relaxed)
    }
}

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
    pub(crate) hud_state: diagnostics::HudState,
    pub(crate) host_cpu: f64,
}

fn to_cstring(path: &std::path::Path) -> CString {
    CString::new(path.to_string_lossy().into_owned()).expect("Path must not contain NUL")
}

impl PlaySession {
    /// Constructs a new `PlaySession` from parsed CLI arguments.
    pub fn new(args: Args) -> Self {
        let paths = PlayPaths::from_args(&args);
        let sys_dir = to_cstring(&paths.config_dir);
        let sav_dir = to_cstring(&paths.save_dir);
        let config = PlayConfig::load().unwrap_or_else(|_| PlayConfig::default());
        let hud = args.hud || config.hud || std::env::var("ALLIUM_HUD").is_ok();
        let scale = args.scale;
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
            scale_mode: scale,
            state_slot: 0,
            command_state: CommandState::new(0),
            system_dir: sys_dir,
            save_dir: sav_dir,
            hud_state: diagnostics::HudState::new(hud),
            host_cpu: 0.0,
        }
    }

    pub(crate) fn core(&self) -> Result<&Core> {
        self.core.as_ref().ok_or_else(|| anyhow!("Core not loaded"))
    }

    /// Dynamically loads the libretro shared library core.
    pub(crate) fn load_core(&mut self) -> Result<()> {
        info!("Loading core from {:?}", self.paths.core_path);
        let core = unsafe { Core::load(&self.paths.core_path)? };
        let info = core.get_system_info();
        info!("Core loaded: {} ({})", info.library_name, info.library_version);
        info!("Extensions: {}", info.valid_extensions);
        self.core = Some(core);
        Ok(())
    }

    /// Loads the active ROM and resolves AV metadata.
    pub(crate) fn load_game(&mut self) -> Result<()> {
        fs::create_dir_all(&self.paths.config_dir)?;
        fs::create_dir_all(&self.paths.save_dir)?;
        let sys_info = self
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("Core not loaded"))?
            .get_system_info();

        let (game_info, resolved, rom_data, rom_path) =
            content::resolve_and_prepare_rom(&self.paths, &sys_info)?;
        self.rom_path_cstring = rom_path;
        self.rom_data = rom_data;
        self.resolved_rom = resolved;
        self.core.as_ref().unwrap().load_game(&game_info)?;

        // Autoload
        let core = self.core.as_ref().unwrap();
        save::load_sram(core, &self.paths)?;
        if self.config.autoload {
            if let Err(err) = save::load_state_slot(core, &self.paths, -1) {
                debug!("Autosave autoload skipped: {}", err);
            }
        }
        self.store_av_info()?;
        Ok(())
    }

    /// Unloads the core game cleanly.
    pub(crate) fn unload_game(&mut self) {
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

    /// Extracts and caches active video/audio geometry from the core.
    pub(crate) fn store_av_info(&mut self) -> Result<()> {
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

    /// Advances the emulator state by one tick.
    pub(crate) fn emulate_single_frame(&mut self,
        frames_run: &mut u64,
    ) -> Result<()> {
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

    /// Displays the current frame buffer on the screen.
    pub(crate) fn present_captured_frame(
        &self,
        driver: &mut DefaultPlatform,
    ) -> Result<bool> {
        if self.paused && driver.skip_presentation_when_paused() {
            return Ok(false);
        }
        if let Some(frame) = &self.captured_frame {
            let format = self
                .pixel_format
                .ok_or_else(|| anyhow!("No pixel format set"))?;
            if driver.video().present(frame, format)?.should_quit {
                return Ok(true);
            }
        }
        Ok(false)
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
    pub(crate) fn apply_scale(
        &self,
        video: &mut impl VideoBackend,
    ) -> Result<()> {
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

    pub(crate) fn set_audio_muted(&mut self, muted: bool) {
        if let Some(producer) = &mut self.audio_producer {
            producer.set_muted(muted);
        }
    }
}

impl Drop for PlaySession {
    fn drop(&mut self) {
        self.unload_game();
    }
}
