use super::*;
use clap::Parser;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::time::Duration;

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
