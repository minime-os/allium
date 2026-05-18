use log::info;
use std::os::raw::{c_uint, c_void};

// libretro calls plain C function pointers, not Rust methods.
// This global gives those callbacks a way back to the active PlaySession.
static mut CALLBACK_HANDLER: Option<*mut dyn LibretroCallbacks> = None;

// The trait keeps the unsafe C entry points separate from session behavior.
pub trait LibretroCallbacks {
    fn on_environment(&mut self, cmd: c_uint, data: *mut c_void) -> bool;
    fn on_video_refresh(
        &mut self,
        data: *const c_void,
        width: c_uint,
        height: c_uint,
        pitch: usize,
    );
    fn on_audio_sample(&mut self, left: i16, right: i16);
    fn on_audio_sample_batch(&mut self, data: *const i16, frames: usize) -> usize;
    fn on_input_poll(&mut self);
    fn on_input_state(&mut self, port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16;
}

// The handler is only valid while the session is alive; stale callback pointers would be UB.
pub unsafe fn set_handler(handler: *mut dyn LibretroCallbacks) {
    unsafe {
        CALLBACK_HANDLER = Some(handler);
    }
}

pub unsafe fn clear_handler() {
    unsafe {
        CALLBACK_HANDLER = None;
    }
}

// These functions match libretro's ABI exactly, then immediately return to Rust code.
pub unsafe extern "C" fn environment_callback(cmd: c_uint, data: *mut c_void) -> bool {
    info!("C: environment_callback cmd={}", cmd);
    let result = unsafe { with_handler(|handler| handler.on_environment(cmd, data)).unwrap_or(false) };
    info!("C: environment_callback done");
    result
}

pub unsafe extern "C" fn video_refresh_callback(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    info!("C: video_refresh_callback");
    unsafe { with_handler(|handler| handler.on_video_refresh(data, width, height, pitch)) };
}

pub unsafe extern "C" fn audio_sample_callback(left: i16, right: i16) {
    unsafe { with_handler(|handler| handler.on_audio_sample(left, right)) };
}

pub unsafe extern "C" fn audio_sample_batch_callback(data: *const i16, frames: usize) -> usize {
    let result = unsafe { with_handler(|handler| handler.on_audio_sample_batch(data, frames)).unwrap_or(0) };
    result
}

pub unsafe extern "C" fn input_poll_callback() {
    unsafe { with_handler(|handler| handler.on_input_poll()) };
}

pub unsafe extern "C" fn input_state_callback(
    port: c_uint,
    device: c_uint,
    index: c_uint,
    id: c_uint,
) -> i16 {
    let result = unsafe { with_handler(|handler| handler.on_input_state(port, device, index, id)).unwrap_or(0) };
    result
}

// Missing handlers can happen during startup/shutdown, so callbacks degrade to no-op values.
unsafe fn with_handler<T>(f: impl FnOnce(&mut dyn LibretroCallbacks) -> T) -> Option<T> {
    unsafe { CALLBACK_HANDLER.and_then(|handler| handler.as_mut()).map(f) }
}
