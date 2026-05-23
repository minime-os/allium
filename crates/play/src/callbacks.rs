//! Bridges libretro's C FFI callback architecture to the active Rust `PlaySession`.
//! Since libretro uses static C function pointers, this module maintains a safe global
//! reference (`CALLBACK_HANDLER`) to delegate events to the active emulation session.

// File Flow:
// 1. Static global callback handler reference (`CALLBACK_HANDLER`).
// 2. `LibretroCallbacks` trait defining handlers for all libretro events.
// 3. Setter and clearer functions to register/unregister the active handler.
// 4. External "C" callback entrypoints invoked directly by libretro.
// 5. Unsafe helper (`with_handler`) to locate the static handler and delegate events.

use crate::core::PlaySession;
use crate::libretro_sys::*;
use crate::video::{CapturedFrame, VideoFrameFormat};
use log::{info, warn, debug};
use std::ffi::CString;
use std::os::raw::{c_char, c_uint, c_void};
use std::ptr;

// libretro calls plain C function pointers, not Rust methods.
// This global gives those callbacks a way back to the active PlaySession.
static mut CALLBACK_HANDLER: Option<*mut PlaySession> = None;

/// Registers the active `PlaySession` callback handler globally.
/// This must be called before running a session so that core events find a valid destination.
pub unsafe fn set_handler(handler: *mut PlaySession) {
    unsafe {
        CALLBACK_HANDLER = Some(handler);
    }
}

/// Unregisters the globally active callback handler.
/// This must be called immediately when a session ends to avoid stale dangling pointers.
pub unsafe fn clear_handler() {
    unsafe {
        CALLBACK_HANDLER = None;
    }
}

/// Environment callback wrapper invoked by the libretro core to query frontend capabilities.
pub unsafe extern "C" fn environment_callback(cmd: c_uint, data: *mut c_void) -> bool {
    unsafe { with_session(|h| h.on_environment(cmd, data)).unwrap_or(false) }
}

/// Video frame callback wrapper invoked by the libretro core for every rendered frame.
pub unsafe extern "C" fn video_refresh_callback(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    unsafe { with_session(|h| h.on_video_refresh(data, width, height, pitch)) };
}

/// Audio mono/stereo single sample callback wrapper invoked by the libretro core.
pub unsafe extern "C" fn audio_sample_callback(left: i16, right: i16) {
    unsafe { with_session(|h| h.on_audio_sample(left, right)) };
}

/// Audio batch sample callback wrapper invoked by the libretro core for high-performance audio output.
pub unsafe extern "C" fn audio_sample_batch_callback(data: *const i16, frames: usize) -> usize {
    unsafe { with_session(|h| h.on_audio_sample_batch(data, frames)).unwrap_or(0) }
}

/// Input poll callback wrapper invoked by the libretro core to instruct Play to refresh inputs.
pub unsafe extern "C" fn input_poll_callback() {
    unsafe { with_session(|h| h.on_input_poll()) };
}

/// Input query callback wrapper invoked by the libretro core to read buttons or controller inputs.
pub unsafe extern "C" fn input_state_callback(
    port: c_uint,
    device: c_uint,
    index: c_uint,
    id: c_uint,
) -> i16 {
    unsafe { with_session(|h| h.on_input_state(port, device, index, id)).unwrap_or(0) }
}

/// Safely attempts to run a operation on the active callback handler if it is present.
/// If the handler is missing (e.g. during early initialization or teardown), it degrades to a harmless no-op.
unsafe fn with_session<T>(f: impl FnOnce(&mut PlaySession) -> T) -> Option<T> {
    unsafe { CALLBACK_HANDLER.and_then(|handler| handler.as_mut()).map(f) }
}

// Callback methods mutate session state instead of using scattered globals.
impl PlaySession {
    /// Handles requests from the core to query frontend environments (directories, pixel formats, etc.).
    pub(crate) fn on_environment(&mut self, cmd: c_uint, data: *mut c_void) -> bool {
        match cmd {
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => self.set_pixel_format(data),
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY => self.write_env_path(data, &self.system_dir),
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => self.write_env_path(data, &self.save_dir),
            RETRO_ENVIRONMENT_GET_FASTFORWARDING => self.write_env_bool(data, self.fast_forwarding),
            RETRO_ENVIRONMENT_GET_CAN_DUPE => self.write_env_bool(data, true),
            RETRO_ENVIRONMENT_SET_MESSAGE => self.set_message(data),
            _ => self.handle_unsupported_env(cmd, data),
        }
    }

    /// Captures a newly rendered frame from the core, copying it to session-owned memory, and draws HUD overlay if active.
    pub(crate) fn on_video_refresh(
        &mut self,
        data: *const c_void,
        width: c_uint,
        height: c_uint,
        pitch: usize,
    ) {
        if !data.is_null() {
            self.copy_refresh_frame(data, width, height, pitch);
            self.hud_state.tick_fps();
            self.draw_refresh_hud(width, height, pitch);
        }
    }

    /// Processes a single stereo audio sample directly into our audio synchronization channel.
    pub(crate) fn on_audio_sample(&mut self, left: i16, right: i16) {
        if let Some(producer) = &mut self.audio_producer {
            producer.push_frame(left, right);
        }
    }

    /// Processes a batch array of stereo audio frames, pushing them into the output channel.
    pub(crate) fn on_audio_sample_batch(&mut self, data: *const i16, frames: usize) -> usize {
        if let Some(producer) = &mut self.audio_producer {
            if !data.is_null() {
                let samples = unsafe { std::slice::from_raw_parts(data, frames * 2) };
                producer.push_frames(samples, frames);
            }
        }

        frames
    }

    /// Dispatches input controller polling triggers.
    pub(crate) fn on_input_poll(&mut self) {}

    /// Queries the current keyboard or joypad button states inside our joypad state tracker.
    pub(crate) fn on_input_state(&mut self, port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16 {
        self.joypad_state.input_state(port, device, index, id)
    }
}

impl PlaySession {
    fn set_pixel_format(&mut self, data: *mut c_void) -> bool {
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
            info!("Unsupported pixel format: {format}");
            false
        }
    }

    fn set_message(&self, data: *mut c_void) -> bool {
        if data.is_null() {
            return false;
        }
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

    fn copy_refresh_frame(&mut self, data: *const c_void, width: c_uint, height: c_uint, pitch: usize) {
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

    fn draw_refresh_hud(&mut self, width: c_uint, height: c_uint, pitch: usize) {
        if self.hud_state.is_enabled() && let Some(format) = self.pixel_format {
            self.hud_state.update(self.host_cpu);
            let aspect = self.av_info.as_ref().map(|av| av.geometry.aspect_ratio).unwrap_or(0.0);
            let frame = self.captured_frame.as_mut().unwrap();
            self.hud_state.draw(
                &mut frame.data,
                width,
                height,
                pitch,
                format,
                self.scale_mode,
                aspect,
            );
        }
    }

    /// Copies system config paths into raw env void pointers, ensuring C-compatibility.
    fn write_env_path(&self, data: *mut c_void, path: &CString) -> bool {
        if data.is_null() {
            return false;
        }

        unsafe {
            *(data as *mut *const c_char) = path.as_ptr();
        }
        true
    }

    /// Copies state boolean flags into raw env void pointers, ensuring C-compatibility.
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
