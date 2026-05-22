// Desktop host video presenter using winit windowing and softbuffer pixels.
// It pumps OS/keyboard events and posts them via channels to the input component.

use crate::control::ControlEvent;
use crate::scale::{ScaleMode, ScaleRect, calculate_scale_rect};
use crate::video::convert::{scale_rgb565_to_xrgb8888, scale_xrgb8888_to_xrgb8888};
use crate::video::frame::{CapturedFrame, VideoFrameFormat};
use crate::video::{VideoPresentResult};
use crate::platform::VideoBackend;
use anyhow::{Context, Result, anyhow};
use common::platform::{Key, KeyEvent, simulator::SCREEN_HEIGHT, simulator::SCREEN_WIDTH};
use log::info;
use softbuffer::Surface;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent as WinitKeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window as WinitWindow, WindowId};

#[cfg(target_os = "macos")]
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
#[cfg(not(target_os = "macos"))]
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;

pub struct SimulatorVideo {
    event_loop: EventLoop<()>,
    app: SimulatorVideoApp,
    pixels: Vec<u32>,
    rect: ScaleRect,
}

struct SimulatorVideoApp {
    window: Option<Rc<WinitWindow>>,
    surface: Option<Surface<Rc<WinitWindow>, Rc<WinitWindow>>>,
    width: NonZeroU32,
    height: NonZeroU32,
    closed: bool,
    key_tx: Sender<KeyEvent>,
    control_tx: Sender<ControlEvent>,
}

impl VideoBackend for SimulatorVideo {
    fn present(
        &mut self,
        frame: &CapturedFrame,
        format: VideoFrameFormat,
    ) -> Result<VideoPresentResult> {
        self.pump_events()?;
        if self.app.closed {
            return Ok(VideoPresentResult { should_quit: true });
        }
        self.scale_frame(frame, format)?;
        self.present_pixels()?;
        Ok(VideoPresentResult::default())
    }

    fn set_scale(
        &mut self,
        mode: ScaleMode,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
    ) -> Result<()> {
        self.rect = calculate_scale_rect(
            mode,
            source_width,
            source_height,
            aspect_ratio,
            self.app.width.get(),
            self.app.height.get(),
        )?;
        Ok(())
    }
}

impl SimulatorVideo {
    pub fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
        key_tx: Sender<KeyEvent>,
        control_tx: Sender<ControlEvent>,
    ) -> Result<Self> {
        let (out_w, out_h) = (*SCREEN_WIDTH, *SCREEN_HEIGHT);
        let width = NonZeroU32::new(out_w).context("Width zero")?;
        let height = NonZeroU32::new(out_h).context("Height zero")?;
        let rect = calculate_scale_rect(scale, source_width, source_height, aspect_ratio, out_w, out_h)?;
        let pixels = vec![0; width.get() as usize * height.get() as usize];
        info!("Simulator video initialized at {}x{}", width.get(), height.get());
        Ok(Self {
            event_loop: EventLoop::new()?,
            app: SimulatorVideoApp::new(width, height, key_tx, control_tx),
            pixels,
            rect,
        })
    }

    fn pump_events(&mut self) -> Result<()> {
        #[cfg(not(target_os = "macos"))]
        {
            self.event_loop.run_app_on_demand(&mut self.app)?;
        }
        #[cfg(target_os = "macos")]
        {
            let status = self
                .event_loop
                .pump_app_events(Some(Duration::ZERO), &mut self.app);
            if matches!(status, PumpStatus::Exit(_)) {
                self.app.closed = true;
            }
        }
        Ok(())
    }

    fn scale_frame(&mut self, frame: &CapturedFrame, format: VideoFrameFormat) -> Result<()> {
        let w = self.app.width.get();
        let h = self.app.height.get();
        match format {
            VideoFrameFormat::Rgb565 => {
                scale_rgb565_to_xrgb8888(frame, &mut self.pixels, w, h, self.rect)
            }
            VideoFrameFormat::Xrgb8888 => {
                scale_xrgb8888_to_xrgb8888(frame, &mut self.pixels, w, h, self.rect)
            }
        }
    }

    fn present_pixels(&mut self) -> Result<()> {
        let surface = self.app.surface.as_mut().context("Simulator video surface not created")?;
        surface.resize(self.app.width, self.app.height)
            .map_err(|err| anyhow!("Failed to resize Play softbuffer surface: {}", err))?;
        let mut buffer = surface.buffer_mut()
            .map_err(|err| anyhow!("Failed to get Play softbuffer buffer: {}", err))?;
        buffer.copy_from_slice(&self.pixels);
        buffer.present()
            .map_err(|err| anyhow!("Failed to present Play softbuffer buffer: {}", err))
    }
}

impl SimulatorVideoApp {
    fn new(
        width: NonZeroU32,
        height: NonZeroU32,
        key_tx: Sender<KeyEvent>,
        control_tx: Sender<ControlEvent>,
    ) -> Self {
        Self {
            window: None,
            surface: None,
            width,
            height,
            closed: false,
            key_tx,
            control_tx,
        }
    }

    fn init_window_and_surface(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WinitWindow::default_attributes()
            .with_title("Play")
            .with_inner_size(PhysicalSize::new(self.width.get(), self.height.get()))
            .with_resizable(false);
        let window = Rc::new(event_loop.create_window(attrs).expect("Failed to create window"));
        let context = softbuffer::Context::new(window.clone()).expect("Failed context");
        let mut surface = softbuffer::Surface::new(&context, window.clone()).expect("Failed surface");
        surface.resize(self.width, self.height).expect("Failed resize");
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn handle_keyboard_input(&mut self, keycode: KeyCode, state: ElementState, repeat: bool) {
        if state == ElementState::Pressed {
            if let Some(event) = control_event_for_keycode(keycode) {
                let _ = self.control_tx.send(event);
                return;
            }
        }
        let key = Key::from(keycode);
        let key_event = match (state, repeat) {
            (ElementState::Pressed, true) => KeyEvent::Autorepeat(key),
            (ElementState::Pressed, false) => KeyEvent::Pressed(key),
            (ElementState::Released, _) => KeyEvent::Released(key),
        };
        let _ = self.key_tx.send(key_event);
    }
}

impl ApplicationHandler for SimulatorVideoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.init_window_and_surface(event_loop);
            #[cfg(not(target_os = "macos"))]
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.closed = true;
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event: WinitKeyEvent { physical_key: PhysicalKey::Code(kc), state, repeat, .. },
                ..
            } => self.handle_keyboard_input(kc, state, repeat),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + Duration::from_millis(1),
        ));
        #[cfg(not(target_os = "macos"))]
        event_loop.exit();
    }
}

fn control_event_for_keycode(keycode: KeyCode) -> Option<ControlEvent> {
    use KeyCode::*;
    match keycode {
        F5 => Some(ControlEvent::SaveState),
        F8 => Some(ControlEvent::LoadState),
        Digit0 => Some(ControlEvent::SelectStateSlot(0)),
        Digit1 => Some(ControlEvent::SelectStateSlot(1)),
        Digit2 => Some(ControlEvent::SelectStateSlot(2)),
        Digit3 => Some(ControlEvent::SelectStateSlot(3)),
        Digit4 => Some(ControlEvent::SelectStateSlot(4)),
        Digit5 => Some(ControlEvent::SelectStateSlot(5)),
        Digit6 => Some(ControlEvent::SelectStateSlot(6)),
        Digit7 => Some(ControlEvent::SelectStateSlot(7)),
        Digit8 => Some(ControlEvent::SelectStateSlot(8)),
        Digit9 => Some(ControlEvent::SelectStateSlot(9)),
        _ => None,
    }
}
