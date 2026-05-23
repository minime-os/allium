#[cfg(not(any(feature = "simulator", feature = "miyoo")))]
compile_error!("pick `simulator` or `miyoo` feature");

mod audio;
mod callbacks;
mod config;
mod content;
mod core;
mod diagnostics;
mod input;
mod libretro_sys;
mod paths;
mod platform;
mod save;
mod shortcuts;
mod unzip;
mod video;

use anyhow::{Result, anyhow};
use config::Args;
use core::{CommandState, PlaySession};
use log::{debug, info, warn};
use shortcuts::ControlEvent;
use std::sync::Arc;
use std::time::{Duration, Instant};
use platform::{DefaultPlatform, EmulationPlatform, InputBackend};
use audio::{AudioQueue, validate_sample_rate};

fn main() -> Result<()> {
    platform::init_logging()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let args = Args::from_env()?;
        let mut session = PlaySession::new(args);

        info!("Initializing PlaySession for core: {}", session.paths.core_id);
        info!("ROM path: {:?}", session.paths.rom);

        unsafe {
            let ptr = &mut session as *mut PlaySession;
            callbacks::set_handler(ptr);
        }

        let result = execute_session(&mut session).await;

        unsafe {
            callbacks::clear_handler();
        }

        result
    })
}

async fn execute_session(session: &mut PlaySession) -> Result<()> {
    session.load_core()?;
    session.load_game()?;

    if session.args.dump_frame.is_some() {
        warm_up_and_dump(session)?;
    } else {
        start_main_loop(session).await?;
    }

    session.unload_game();
    Ok(())
}

fn warm_up_and_dump(session: &PlaySession) -> Result<()> {
    const DUMP_WARMUP_FRAMES: usize = 60;
    info!("Running {} warmup frames for dump...", DUMP_WARMUP_FRAMES);
    if let Some(core) = &session.core {
        for _ in 0..DUMP_WARMUP_FRAMES {
            core.run();
        }
    }
    dump_captured_frame(session)?;
    Ok(())
}

fn dump_captured_frame(session: &PlaySession) -> Result<()> {
    let Some(path) = &session.args.dump_frame else {
        return Ok(());
    };
    let frame = session
        .captured_frame
        .as_ref()
        .ok_or_else(|| anyhow!("No frame captured"))?;
    diagnostics::dump_frame(path, frame, session.pixel_format)?;
    info!("Frame dumped to {:?}", path);
    Ok(())
}

// ---- Frame loop ----

async fn start_main_loop(session: &mut PlaySession) -> Result<()> {
    let av = session.av_info.as_ref().unwrap();
    let fps = av.timing.fps;
    let frame_interval = frame_interval(fps)?;
    let sample_rate = validate_sample_rate(av.timing.sample_rate)?;
    let base_width = av.geometry.base_width;
    let base_height = av.geometry.base_height;
    let aspect_ratio = av.geometry.aspect_ratio;
    let (cons, mut rx, command_server) = setup_audio_and_command_server(session, sample_rate)?;
    let mut drv = DefaultPlatform::initialize(
        base_width,
        base_height,
        aspect_ratio,
        session.args.scale,
        sample_rate,
        cons,
    )?;
    let mut frames = 0u64;
    let started_at = Instant::now();
    let mut next_at = started_at;
    let mut ctrl_c = std::pin::pin!(tokio::signal::ctrl_c());
    let reason = run_emulation_loop(
        session, &mut rx, &mut drv, &mut frames, &mut next_at,
        frame_interval, &mut ctrl_c,
    ).await?;
    shutdown_loop(session, reason, frames, started_at, fps);
    command_server.abort();
    Ok(())
}

async fn run_emulation_loop(
    session: &mut PlaySession,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    drv: &mut DefaultPlatform,
    frames: &mut u64,
    next_at: &mut Instant,
    interval: Duration,
    ctrl_c: &mut std::pin::Pin<&mut impl std::future::Future<Output = std::io::Result<()>>>,
) -> Result<&'static str> {
    loop {
        if let Some(reason) = run_loop_step(session, rx, drv, frames, next_at, interval)? {
            return Ok(reason);
        }
        if session.fast_forwarding {
            tokio::task::yield_now().await;
            continue;
        }
        let sleep = tokio::time::Instant::from_std(*next_at);
        if let Some(r) = wait_frame(session, sleep, ctrl_c, rx, drv).await? {
            return Ok(r);
        }
    }
}

fn run_loop_step(
    session: &mut PlaySession,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    drv: &mut DefaultPlatform,
    frames: &mut u64,
    next_at: &mut Instant,
    interval: Duration,
) -> Result<Option<&'static str>> {
    if session.args.frames == Some(*frames) {
        return Ok(Some("frame cap reached"));
    }
    process_pending_control_events(session, rx, drv)?;
    if session.should_quit {
        return Ok(Some("quit command"));
    }
    poll_and_apply_platform_inputs(session, drv);
    session.host_cpu = drv.stats().cpu_usage().unwrap_or(0.0);
    session.emulate_single_frame(frames)?;
    if session.present_captured_frame(drv)? {
        return Ok(Some("window closed"));
    }
    *next_at = (*next_at + interval).max(Instant::now());
    Ok(None)
}

async fn wait_frame(
    session: &mut PlaySession,
    deadline: tokio::time::Instant,
    ctrl_c: &mut std::pin::Pin<&mut impl std::future::Future<Output = std::io::Result<()>>>,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    drv: &mut DefaultPlatform,
) -> Result<Option<&'static str>> {
    let result = {
        let mut shutdown = std::pin::pin!(drv.wait_for_shutdown());
        wait_for_next_frame_or_control(deadline, ctrl_c, &mut shutdown, rx).await
    };
    match result {
        LoopWait::Frame => Ok(None),
        LoopWait::Signal => Ok(Some("signal received")),
        LoopWait::Control(event) => {
            apply_control_event(session, event)?;
            session.apply_scale(drv.video())?;
            Ok(None)
        }
    }
}

fn shutdown_loop(
    session: &mut PlaySession,
    reason: &str,
    frames: u64,
    started_at: Instant,
    target_fps: f64,
) {
    let elapsed = started_at.elapsed();
    let avg = if frames == 0 {
        Duration::ZERO
    } else {
        elapsed.div_f64(frames as f64)
    };
    info!(
        "Frame loop stopped: reason={}, frames={}, elapsed={:?}, avg_frame_time={:?}, target_fps={}",
        reason, frames, elapsed, avg, target_fps
    );
    session.audio_producer = None;
}

fn setup_audio_and_command_server(
    session: &mut PlaySession,
    rate: u32,
) -> Result<(
    audio::AudioConsumer,
    tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    tokio::task::JoinHandle<()>,
)> {
    let (mut prod, cons) = AudioQueue::for_sample_rate(rate);
    prod.set_muted(session.fast_forwarding);
    session.audio_producer = Some(prod);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let state = Arc::clone(&session.command_state);
    let srv = tokio::spawn(async move {
        if let Err(err) = run_command_server(tx, state).await {
            warn!("Play UDP command server stopped: {}", err);
        }
    });
    Ok((cons, rx, srv))
}

// ---- Action dispatch ----

fn process_pending_control_events(
    session: &mut PlaySession,
    control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    driver: &mut DefaultPlatform,
) -> Result<()> {
    while let Ok(event) = control_rx.try_recv() {
        apply_control_event(session, event)?;
        session.apply_scale(driver.video())?;
    }
    Ok(())
}

fn poll_and_apply_platform_inputs(
    session: &mut PlaySession,
    driver: &mut DefaultPlatform,
) {
    let platform_events = driver.input().poll(&mut session.joypad_state);
    for event in platform_events {
        if let Err(err) = apply_control_event(session, event) {
            warn!("Control event failed: {}", err);
        }
    }
}

fn apply_control_event(session: &mut PlaySession, event: ControlEvent) -> Result<()> {
    match event {
        ControlEvent::SaveState => save::save_state_slot(session.core()?, &session.paths, session.state_slot),
        ControlEvent::LoadState => save::load_state_slot(session.core()?, &session.paths, session.state_slot),
        ControlEvent::SaveStateSlot(slot) => {
            session.select_state_slot(slot)?;
            save::save_state_slot(session.core()?, &session.paths, slot)
        }
        ControlEvent::LoadStateSlot(slot) => {
            session.select_state_slot(slot)?;
            save::load_state_slot(session.core()?, &session.paths, slot)
        }
        ControlEvent::SelectStateSlot(slot) => session.select_state_slot(slot),
        ControlEvent::StateSlotPlus => session.select_state_slot((session.state_slot + 1).min(9)),
        ControlEvent::StateSlotMinus => session.select_state_slot((session.state_slot - 1).max(-1)),
        ControlEvent::SetPaused(paused) => { session.paused = paused; Ok(()) }
        ControlEvent::TogglePaused => { session.paused = !session.paused; Ok(()) }
        ControlEvent::ToggleFastForward => {
            session.fast_forwarding = !session.fast_forwarding;
            session.set_audio_muted(session.fast_forwarding);
            Ok(())
        }
        ControlEvent::SetFastForward(enabled) => {
            session.fast_forwarding = enabled;
            session.set_audio_muted(enabled);
            Ok(())
        }
        ControlEvent::Reset => { session.core()?.reset(); Ok(()) }
        ControlEvent::Quit => { session.should_quit = true; Ok(()) }
        ControlEvent::CycleScale => {
            session.scale_mode = session.scale_mode.next();
            info!("Selected scale mode: {:?}", session.scale_mode);
            Ok(())
        }
    }
}

// ---- Frame timing ----

enum LoopWait {
    Frame,
    Signal,
    Control(ControlEvent),
}

fn frame_interval(fps: f64) -> Result<Duration> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(anyhow!("Core reported invalid FPS: {}", fps));
    }
    Ok(Duration::from_secs_f64(1.0 / fps))
}

async fn wait_for_next_frame_or_control<F, S>(
    deadline: tokio::time::Instant,
    ctrl_c: &mut std::pin::Pin<&mut F>,
    shutdown: &mut std::pin::Pin<&mut S>,
    control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
) -> LoopWait
where
    F: std::future::Future<Output = std::io::Result<()>>,
    S: std::future::Future<Output = ()>,
{
    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => LoopWait::Frame,
        _ = ctrl_c.as_mut() => LoopWait::Signal,
        _ = shutdown.as_mut() => LoopWait::Signal,
        event = control_rx.recv() => event.map_or(LoopWait::Signal, LoopWait::Control),
    }
}

// ---- UDP command server ----

async fn run_command_server(
    tx: tokio::sync::mpsc::UnboundedSender<ControlEvent>,
    state: Arc<CommandState>,
) -> Result<()> {
    let socket = tokio::net::UdpSocket::bind(common::constants::RETROARCH_UDP_SOCKET).await?;
    let mut buf = [0u8; 256];
    debug!("Play UDP command server bound at {}", common::constants::RETROARCH_UDP_SOCKET);
    while process_next_datagram(&socket, &mut buf, &tx, &state).await? {}
    Ok(())
}

fn parse_udp_command(raw: &str) -> Option<common::retroarch::RetroArchCommand> {
    match std::str::FromStr::from_str(raw.trim()) {
        Ok(command) => Some(command),
        Err(err) => {
            warn!("Ignoring invalid UDP command {:?}: {}", raw, err);
            None
        }
    }
}

async fn process_next_datagram(
    socket: &tokio::net::UdpSocket,
    buf: &mut [u8; 256],
    tx: &tokio::sync::mpsc::UnboundedSender<ControlEvent>,
    state: &CommandState,
) -> Result<bool> {
    let (len, peer) = socket.recv_from(buf).await?;
    let raw = String::from_utf8_lossy(&buf[..len]);
    let Some(cmd) = parse_udp_command(&raw) else { return Ok(true); };
    if let Some(reply) = reply_for_command(&cmd, state) {
        socket.send_to(reply.as_bytes(), peer).await?;
    } else if let Some(ev) = ControlEvent::from_retroarch_command(cmd) {
        return Ok(tx.send(ev).is_ok());
    }
    Ok(true)
}

fn reply_for_command(command: &common::retroarch::RetroArchCommand, state: &CommandState) -> Option<String> {
    use common::retroarch::RetroArchCommand;
    match command {
        RetroArchCommand::GetInfo => Some(format!("GET_INFO 0 0 {}", state.state_slot())),
        RetroArchCommand::GetDiskCount => Some("GET_DISK_COUNT 0".to_string()),
        RetroArchCommand::GetDiskSlot => Some("GET_DISK_SLOT 0".to_string()),
        RetroArchCommand::GetStateSlot => Some(format!("GET_STATE_SLOT {}", state.state_slot())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libretro_sys::*;
    use clap::Parser;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};
    use std::ptr;

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
    fn get_info_reply_matches_menu_parser_shape() {
        let state = CommandState::new(-1);
        assert_eq!(
            reply_for_command(&common::retroarch::RetroArchCommand::GetInfo, &state),
            Some("GET_INFO 0 0 -1".to_string())
        );
    }
}
