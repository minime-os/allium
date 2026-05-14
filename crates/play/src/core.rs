use crate::libretro_sys::*;
use anyhow::{Context, Result, anyhow};
use libloading::Library;
use std::ffi::CStr;
use std::os::raw::c_uint;
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;

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
pub struct Core {
    _lib: Library, // Keep the dynamic library loaded while these function pointers exist.
    symbols: CoreSymbols,
}

// CoreSymbols is the raw libretro function table loaded from the dynamic library.
#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
struct CoreSymbols {
    retro_init: unsafe extern "C" fn(), // Start the core after callbacks are registered.
    retro_deinit: unsafe extern "C" fn(), // Let the core clean up before the library unloads.
    retro_api_version: unsafe extern "C" fn() -> c_uint, // Check that Play and the core speak the same API version.
    retro_get_system_info: unsafe extern "C" fn(info: *mut retro_system_info), // Ask what the core is and what content it accepts.
    retro_get_system_av_info: unsafe extern "C" fn(info: *mut retro_system_av_info), // Ask for video geometry and audio timing after loading content.
    retro_set_environment: unsafe extern "C" fn(cb: retro_environment_t), // Give the core a way to ask frontend questions.
    retro_set_video_refresh: unsafe extern "C" fn(cb: retro_video_refresh_t), // Give the core a callback for each video frame.
    retro_set_audio_sample: unsafe extern "C" fn(cb: retro_audio_sample_t), // Give the core a callback for one stereo audio sample.
    retro_set_audio_sample_batch: unsafe extern "C" fn(cb: retro_audio_sample_batch_t), // Give the core a callback for a batch of audio frames.
    retro_set_input_poll: unsafe extern "C" fn(cb: retro_input_poll_t), // Let the core ask Play to refresh input state.
    retro_set_input_state: unsafe extern "C" fn(cb: retro_input_state_t), // Let the core ask whether a button/input is active.
    retro_load_game: unsafe extern "C" fn(game: *const retro_game_info) -> bool, // Hand the selected ROM to the core.
    retro_unload_game: unsafe extern "C" fn(), // Release game-specific state while keeping the core loaded.
    retro_run: unsafe extern "C" fn(),         // Execute one emulation frame.
    retro_serialize_size: unsafe extern "C" fn() -> usize, // Ask how large a save state buffer must be.
    retro_serialize: unsafe extern "C" fn(data: *mut c_void, len: usize) -> bool, // Copy core state into a buffer.
    retro_unserialize: unsafe extern "C" fn(data: *const c_void, len: usize) -> bool, // Restore core state from a buffer.
    retro_get_memory_data: unsafe extern "C" fn(id: c_uint) -> *mut c_void, // Get core-owned memory such as SRAM.
    retro_get_memory_size: unsafe extern "C" fn(id: c_uint) -> usize, // Get core-owned memory size.
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

        Ok(Self { _lib: lib, symbols })
    }
}

impl CoreSymbols {
    // Step 2: after the library is open, load every libretro function Play needs.
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
                retro_load_game: load_symbol(lib, b"retro_load_game")?,
                retro_unload_game: load_symbol(lib, b"retro_unload_game")?,
                retro_run: load_symbol(lib, b"retro_run")?,
                retro_serialize_size: load_symbol(lib, b"retro_serialize_size")?,
                retro_serialize: load_symbol(lib, b"retro_serialize")?,
                retro_unserialize: load_symbol(lib, b"retro_unserialize")?,
                retro_get_memory_data: load_symbol(lib, b"retro_get_memory_data")?,
                retro_get_memory_size: load_symbol(lib, b"retro_get_memory_size")?,
            })
        }
    }

    // Step 3: reject cores using an API version Play does not understand.
    fn check_api_version(&self) -> Result<()> {
        let api_version = unsafe { (self.retro_api_version)() };
        if api_version != RETRO_API_VERSION {
            return Err(anyhow!("Unsupported libretro API version: {}", api_version));
        }

        Ok(())
    }

    // Step 4: give the core Play's callback functions before retro_init.
    fn install_callbacks(&self) {
        use crate::callbacks::*;
        unsafe {
            (self.retro_set_environment)(Some(environment_callback));
            (self.retro_set_video_refresh)(Some(video_refresh_callback));
            (self.retro_set_audio_sample)(Some(audio_sample_callback));
            (self.retro_set_audio_sample_batch)(Some(audio_sample_batch_callback));
            (self.retro_set_input_poll)(Some(input_poll_callback));
            (self.retro_set_input_state)(Some(input_state_callback));
        }
    }

    // Step 5: initialize the core after the frontend callbacks are installed.
    fn init(&self) {
        unsafe {
            (self.retro_init)();
        }
    }

    // Drop path: retro_deinit lets the core clean up while its code is still loaded.
    fn deinit(&self) {
        unsafe {
            (self.retro_deinit)();
        }
    }
}

impl Core {
    // Rust wrappers around libretro function pointers.
    pub fn load_game(&self, info: &retro_game_info) -> Result<()> {
        if unsafe { (self.symbols.retro_load_game)(info) } {
            Ok(())
        } else {
            Err(anyhow!("Failed to load game"))
        }
    }

    pub fn unload_game(&self) {
        unsafe { (self.symbols.retro_unload_game)() };
    }

    pub fn run(&self) {
        unsafe { (self.symbols.retro_run)() };
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub fn serialize_size(&self) -> usize {
        unsafe { (self.symbols.retro_serialize_size)() }
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub fn serialize(&self, data: &mut [u8]) -> bool {
        unsafe { (self.symbols.retro_serialize)(data.as_mut_ptr() as *mut c_void, data.len()) }
    }

    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub fn unserialize(&self, data: &[u8]) -> bool {
        unsafe { (self.symbols.retro_unserialize)(data.as_ptr() as *const c_void, data.len()) }
    }

    pub fn memory_region(&self, id: c_uint) -> Option<(*mut u8, usize)> {
        let size = unsafe { (self.symbols.retro_get_memory_size)(id) };
        if size == 0 {
            return None;
        }

        let data = unsafe { (self.symbols.retro_get_memory_data)(id) };
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
        unsafe { (self.symbols.retro_get_system_av_info)(&mut info) };
        info
    }

    pub fn get_system_info(&self) -> CoreInfo {
        let mut info = retro_system_info {
            library_name: ptr::null(),
            library_version: ptr::null(),
            valid_extensions: ptr::null(),
            need_fullpath: false,
            block_extract: false,
        };

        unsafe {
            (self.symbols.retro_get_system_info)(&mut info);

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
