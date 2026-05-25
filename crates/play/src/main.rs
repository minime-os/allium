#[cfg(not(any(feature = "simulator", feature = "miyoo")))]
compile_error!("pick `simulator` or `miyoo` feature");

mod audio;
mod commands;
mod config;
mod content;
mod controls;
mod core;
mod core_options;
mod dump;
mod hud;
mod input;
mod paths;
mod platform;
mod save;
mod settings;
mod unzip;
mod video;

use anyhow::{Result, anyhow};
use config::Args;
use core::{ActiveSession, PlayContext};
use log::{info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use platform::{DefaultPlatform, init_logging};
use video::frame_interval;
use commands::ControlEvent;

/// RAII guard that writes a state marker file on creation and removes it on drop.
struct PlayStateGuard {
    path: std::path::PathBuf,
}

impl PlayStateGuard {
    fn new(path: &std::path::Path) -> Self {
        let core_id = std::env::var("ALLIUM_CORE_ID").unwrap_or_else(|_| "unknown".to_string());
        let content = format!("{{\"core_id\":\"{}\"}}\n", core_id);
        if let Err(e) = std::fs::write(path, content) {
            log::warn!("Failed to write play state file: {}", e);
        }
        Self { path: path.to_path_buf() }
    }
}

impl Drop for PlayStateGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Failed to remove play state file: {}", e);
            }
        }
    }
}

fn main() -> Result<()> {
    init_logging()?;

    let state_path = common::constants::ALLIUM_BASE_DIR.join("state").join("play.json");
    std::fs::create_dir_all(state_path.parent().unwrap_or_else(|| std::path::Path::new("")))?;
    let _guard = PlayStateGuard::new(&state_path);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let args = Args::from_env()?;
        let ctx = PlayContext::new(args);
        let (mut session, audio_consumer) = ActiveSession::new(ctx)?;

        info!("Initializing Play for core: {}", session.ctx.paths.core_id);
        info!("ROM path: {:?}", session.ctx.paths.rom);

        unsafe {
            core::set_handler(&mut session);
        }

        let result = execute_session(&mut session, audio_consumer).await;

        unsafe {
            core::clear_handler();
        }

        result
    })
}

async fn execute_session(
    session: &mut ActiveSession,
    audio_consumer: crate::audio::AudioConsumer,
) -> Result<()> {
    if session.ctx.args.dump_frame.is_some() {
        warm_up_and_dump(session)?;
    } else {
        start_main_loop(session, audio_consumer).await?;
    }
    Ok(())
}

fn warm_up_and_dump(session: &ActiveSession) -> Result<()> {
    const DUMP_WARMUP_FRAMES: usize = 60;
    info!("Running {} warmup frames for dump...", DUMP_WARMUP_FRAMES);
    for _ in 0..DUMP_WARMUP_FRAMES {
        session.core.run();
    }
    dump_captured_frame(session)?;
    Ok(())
}

fn dump_captured_frame(session: &ActiveSession) -> Result<()> {
    let Some(path) = &session.ctx.args.dump_frame else {
        return Ok(());
    };
    if session.captured_frame.width == 0 {
        return Err(anyhow!("No frame captured"));
    }
    dump::dump_frame(path, &session.captured_frame, Some(session.pixel_format))?;
    info!("Frame dumped to {:?}", path);
    Ok(())
}

// ---- Frame loop ----

async fn start_main_loop(
    session: &mut ActiveSession,
    audio_consumer: crate::audio::AudioConsumer,
) -> Result<()> {
    let av = &session.av_info;
    let fps = av.timing.fps;
    let frame_interval = frame_interval(fps)?;
    let sample_rate = av.timing.sample_rate.round() as u32;
    let base_width = av.geometry.base_width;
    let base_height = av.geometry.base_height;
    let aspect_ratio = av.geometry.aspect_ratio;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let state = Arc::clone(&session.command_state);
    let command_server = tokio::spawn(async move {
        if let Err(err) = commands::run_command_server(tx, state).await {
            warn!("Play UDP command server stopped: {}", err);
        }
    });

    let mut drv = DefaultPlatform::new(
        &session.ctx.paths.core_id,
        base_width,
        base_height,
        aspect_ratio,
        session.scale_mode,
        sample_rate,
        audio_consumer,
    )?;
    session.apply_frontend_settings(&mut drv)?;

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
    session: &mut ActiveSession,
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
        // Fast-forward only bypasses wait when cap > 1x.
        let ff_enabled = session.fast_forwarding && session.frontend_settings.max_ff_speed > 1;
        if ff_enabled {
            tokio::task::yield_now().await;
            continue;
        }
        if session.frontend_settings.tearing == settings::TearingMode::Off {
            continue;
        }
        let sleep = tokio::time::Instant::from_std(*next_at);
        if let Some(r) = wait_frame(session, sleep, ctrl_c, rx, drv).await? {
            return Ok(r);
        }
    }
}

fn run_loop_step(
    session: &mut ActiveSession,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    drv: &mut DefaultPlatform,
    frames: &mut u64,
    next_at: &mut Instant,
    interval: Duration,
) -> Result<Option<&'static str>> {
    if session.ctx.args.frames == Some(*frames) {
        return Ok(Some("frame cap reached"));
    }
    process_pending_control_events(session, rx, drv)?;
    if session.should_quit {
        return Ok(Some("quit command"));
    }
    poll_and_apply_platform_inputs(session, drv);
    session.host_cpu = drv.cpu_usage().unwrap_or(0.0);
    session.emulate_single_frame(frames);
    if session.present_captured_frame(drv)? {
        return Ok(Some("window closed"));
    }
    if session.frontend_settings.tearing == settings::TearingMode::Strict {
        *next_at += interval;
    } else {
        *next_at = (*next_at + interval).max(Instant::now());
    }
    Ok(None)
}

async fn wait_frame(
    session: &mut ActiveSession,
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
            session.apply_control_event(event, drv)?;
            Ok(None)
        }
    }
}

fn shutdown_loop(
    session: &mut ActiveSession,
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
    session.set_audio_muted(true);
}

fn process_pending_control_events(
    session: &mut ActiveSession,
    control_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ControlEvent>,
    drv: &mut DefaultPlatform,
) -> Result<()> {
    while let Ok(event) = control_rx.try_recv() {
        session.apply_control_event(event, drv)?;
    }
    Ok(())
}

fn poll_and_apply_platform_inputs(
    session: &mut ActiveSession,
    drv: &mut DefaultPlatform,
) {
    let platform_events = drv.poll_input(&mut session.joypad_state);

    // Check for shortcut combos before forwarding to core.
    let menu_held = session.joypad_state.is_pressed(common::platform::Key::Menu);
    let actions = session.shortcut_bindings.poll(session.joypad_state.raw_keys(), menu_held);
    for action in actions {
        let event = match action.as_str() {
            "toggle_fast_forward" => Some(ControlEvent::ToggleFastForward),
            "save_state" => Some(ControlEvent::SaveState),
            "load_state" => Some(ControlEvent::LoadState),
            "reset" => Some(ControlEvent::Reset),
            "toggle_menu" => Some(ControlEvent::SetPaused(true)),
            _ => {
                info!("Unknown shortcut action: {}", action);
                None
            }
        };
        if let Some(ev) = event {
            if let Err(err) = session.apply_control_event(ev, drv) {
                warn!("Shortcut event failed: {}", err);
            }
        }
    }

    for event in platform_events {
        if let Err(err) = session.apply_control_event(event, drv) {
            warn!("Control event failed: {}", err);
        }
    }
}

// ---- Frame timing ----

enum LoopWait {
    Frame,
    Signal,
    Control(ControlEvent),
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
    // When the core is already behind schedule, skip the async select machinery
    // to avoid tokio::sleep_until overhead on every late frame.
    if tokio::time::Instant::now() >= deadline {
        return LoopWait::Frame;
    }

    tokio::select! {
        _ = tokio::time::sleep_until(deadline) => LoopWait::Frame,
        _ = ctrl_c.as_mut() => LoopWait::Signal,
        _ = shutdown.as_mut() => LoopWait::Signal,
        event = control_rx.recv() => event.map_or(LoopWait::Signal, LoopWait::Control),
    }
}
