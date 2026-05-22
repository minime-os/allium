//! Handles emulator loop execution synchronization, frame-rate timing calculations,
//! and multiplexing of shutdown signals, frame deadlines, and control events.

use crate::control::ControlEvent;
use std::time::Duration;
use anyhow::{anyhow, Result};

/// Helper mapping target FPS rates to microsecond-resolution clock tick durations.
pub(crate) fn frame_interval(fps: f64) -> Result<Duration> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(anyhow!("Core reported invalid FPS: {}", fps));
    }

    Ok(Duration::from_secs_f64(1.0 / fps))
}

pub(crate) enum LoopWait {
    Frame,
    Signal,
    Control(ControlEvent),
}

pub(crate) async fn wait_for_next_frame_or_control<F, S>(
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
