use crate::frame::{CapturedFrame, scale_rgb565_to_xrgb8888, scale_xrgb8888_to_xrgb8888};
use crate::scale::{ScaleMode, ScaleRect, calculate_scale_rect};
use anyhow::{Context, Result, anyhow};
use log::info;
use softbuffer::Surface;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
#[cfg(target_os = "macos")]
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
#[cfg(not(target_os = "macos"))]
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::{Window as WinitWindow, WindowId};

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
}

#[derive(Clone, Copy)]
pub enum SimulatorPixelFormat {
    Rgb565,
    Xrgb8888,
}

impl SimulatorVideo {
    pub fn new(
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
    ) -> Result<Self> {
        let output_width = simulator_output_width();
        let output_height = simulator_output_height();
        let width =
            NonZeroU32::new(output_width).context("Simulator video width must be non-zero")?;
        let height =
            NonZeroU32::new(output_height).context("Simulator video height must be non-zero")?;
        let rect = calculate_scale_rect(
            scale,
            source_width,
            source_height,
            aspect_ratio,
            output_width,
            output_height,
        )?;
        let event_loop = EventLoop::new()?;
        let pixels = vec![0; width.get() as usize * height.get() as usize];
        let mut video = Self {
            event_loop,
            app: SimulatorVideoApp {
                window: None,
                surface: None,
                width,
                height,
                closed: false,
            },
            pixels,
            rect,
        };

        video.pump_events()?;
        info!(
            "Simulator video initialized at {}x{}",
            width.get(),
            height.get()
        );
        Ok(video)
    }

    pub fn present(&mut self, frame: &CapturedFrame, format: SimulatorPixelFormat) -> Result<bool> {
        self.pump_events()?;
        if self.app.closed {
            return Ok(true);
        }

        match format {
            SimulatorPixelFormat::Rgb565 => scale_rgb565_to_xrgb8888(
                frame,
                &mut self.pixels,
                self.app.width.get(),
                self.app.height.get(),
                self.rect,
            )?,
            SimulatorPixelFormat::Xrgb8888 => scale_xrgb8888_to_xrgb8888(
                frame,
                &mut self.pixels,
                self.app.width.get(),
                self.app.height.get(),
                self.rect,
            )?,
        }

        let surface = self
            .app
            .surface
            .as_mut()
            .context("Simulator video surface was not created")?;
        surface
            .resize(self.app.width, self.app.height)
            .map_err(|err| anyhow!("Failed to resize Play softbuffer surface: {}", err))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|err| anyhow!("Failed to get Play softbuffer buffer: {}", err))?;
        buffer.copy_from_slice(&self.pixels);
        buffer
            .present()
            .map_err(|err| anyhow!("Failed to present Play softbuffer buffer: {}", err))?;

        Ok(false)
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
}

fn simulator_output_width() -> u32 {
    std::env::var("WIDTH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(752)
}

fn simulator_output_height() -> u32 {
    std::env::var("HEIGHT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(560)
}

impl ApplicationHandler for SimulatorVideoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = WinitWindow::default_attributes()
            .with_title("Play")
            .with_inner_size(PhysicalSize::new(self.width.get(), self.height.get()))
            .with_resizable(false);
        let window = event_loop
            .create_window(window_attrs)
            .expect("Failed to create Play simulator window");
        let window = Rc::new(window);
        let context = softbuffer::Context::new(window.clone())
            .expect("Failed to create Play softbuffer context");
        let mut surface = softbuffer::Surface::new(&context, window.clone())
            .expect("Failed to create Play softbuffer surface");
        surface
            .resize(self.width, self.height)
            .expect("Failed to resize Play softbuffer surface");

        self.window = Some(window);
        self.surface = Some(surface);
        #[cfg(not(target_os = "macos"))]
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            self.closed = true;
            event_loop.exit();
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
