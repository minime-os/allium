// Bridges libretro's C FFI callback architecture to the active Rust ActiveSession.
// Since libretro uses static C function pointers, this module maintains a safe global
// reference (CALLBACK_HANDLER) to delegate events to the active emulation session.

use crate::core::ActiveSession;
use std::os::raw::{c_uint, c_void};

// libretro calls plain C function pointers, not Rust methods.
// This global gives those callbacks a way back to the active ActiveSession.
static mut CALLBACK_HANDLER: Option<*mut ActiveSession> = None;

/// Registers the active callback handler globally.
pub unsafe fn set_handler(handler: *mut ActiveSession) {
    unsafe { CALLBACK_HANDLER = Some(handler); }
}

/// Unregisters the globally active callback handler.
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
