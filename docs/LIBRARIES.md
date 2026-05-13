# Library primers

Reference for the Rust crates and external surfaces Play uses. Read the relevant section before the stage that introduces it. Each section is a starting point, not a complete API tour — official docs are linked at the bottom of each.

Sections roughly ordered by stage of first use.

---

## libretro

**What it is.** A C ABI for emulator cores. RetroArch is the best-known frontend; MinArch is another. Play is a third. Cores are shared libraries (`.so` on Linux, `.dylib` on macOS) implementing the libretro interface. They emulate consoles (SNES, Genesis, PSX, etc.) and report frames + audio + state to the frontend, which renders them and feeds back input.

**Mental model.** A core is a state machine. The frontend:
1. Loads the core (`dlopen`).
2. Calls `retro_set_*` to register five callbacks (environment, video_refresh, audio_sample, audio_sample_batch, input_poll, input_state — six really).
3. Calls `retro_init`.
4. Calls `retro_load_game` with ROM bytes.
5. Loops: `retro_run()` once per frame. The core synchronously calls the frontend's callbacks during this call to deliver pixels, audio, request input.
6. Eventually: `retro_unload_game`, `retro_deinit`, drop the library.

There is no per-instance state in the C ABI. All cores assume singleton ownership. A frontend can run only one core at a time per process.

Play routes callbacks through one process-global pointer to the active `PlaySession` / callback state. The invariant is explicit: one core per process, pointer set before callbacks can fire, pointer valid while callbacks can fire, pointer cleared before state drop. The raw `extern "C"` functions should only translate raw pointers/integers into Play-owned types and delegate to safe Rust methods. Do not use a global `Mutex<Option<State>>` as the architecture.

**Key callbacks.**
- `environment(cmd, data) -> bool`: cores ask the frontend for capabilities. Most commands (RETRO_ENVIRONMENT_GET_*) you can return false to. SET_PIXEL_FORMAT and SET_VARIABLES you'll handle eventually.
- `video_refresh(data, width, height, pitch)`: pixels arrive here. `data` is a pointer to a buffer in the core's pixel format (RGB565 by default, can be XRGB8888 if the core requests). `pitch` = bytes per row (often `width * 2`).
- `audio_sample_batch(data, frames) -> usize`: stereo i16 samples in interleaved L/R/L/R order. Return frames consumed.
- `input_state(port, device, index, id) -> i16`: the core asks "is button X on port 0 pressed?" return 0 or 1.

**Common gotchas.**
- Callbacks fire on the thread calling `retro_run` — your main loop. Don't lock anything that the audio thread also needs without thinking through deadlocks.
- The pixel format defaults to RGB565. If a core needs XRGB8888, it tells you via SET_PIXEL_FORMAT. If you ignore SET_PIXEL_FORMAT, some cores will emit XRGB8888 anyway and your output will look broken — handle the env command.
- `retro_get_memory_data(RETRO_MEMORY_SAVE_RAM)` returns a raw pointer into the core's address space. The buffer is valid until `retro_unload_game`. Treat it carefully.

**Docs.**
- `libretro.h` itself is the canonical doc. Read the comments — they're thorough.
- https://docs.libretro.com/development/libretro-overview/
- https://github.com/libretro/RetroArch (RetroArch's frontend code is the reference implementation)

---

## bindgen

**What it is.** A CLI/library that reads a C header and emits Rust `extern "C"` declarations + struct definitions. Used once (in Stage 2) to convert `libretro.h` into `libretro_sys.rs`.

**Mental model.** Run `bindgen header.h -o output.rs` with allowlist filters to keep the output small. Commit `output.rs`. Re-run only when you bump the header version. There is no `build.rs`; normal builds should not need libclang.

Isolation rule: only Play's FFI wrapper (`core.rs`) and callback bridge import `libretro_sys`. The rest of Play uses Play-owned types (`PixelFormat`, `FrameRef`, `AvInfo`, `SystemInfo`, etc.). This keeps unsafe ABI details narrow and makes a later handwritten ABI migration cheap.

**Key flags for libretro.**
- `--allowlist-type 'retro_.*'` — keep types prefixed with `retro_`
- `--allowlist-function 'retro_.*'` — keep matching functions
- `--allowlist-var 'RETRO_.*'` — keep matching constants
- `--no-layout-tests` — skip `_bindgen_test_layout_*` functions (they bloat output, only useful when changing C compilers)

**Common gotchas.**
- bindgen requires `libclang` installed. On macOS via brew: `brew install llvm`. Set `LIBCLANG_PATH` if it can't find it.
- Output uses `::std::os::raw::c_int` etc. — fine, but verbose. Some folks prefer `--use-core` for `#![no_std]`. We don't need that.

**Install.** `cargo install bindgen-cli` (CLI). Do not add `bindgen` to `[build-dependencies]`; Play uses manual committed bindings to keep build-time simple.

**Docs.** https://rust-lang.github.io/rust-bindgen/

---

## libloading

**What it is.** Cross-platform dynamic library loading (`dlopen` on Unix, `LoadLibrary` on Windows).

**Mental model.**

```rust
let lib = unsafe { libloading::Library::new(path)? };
let func: libloading::Symbol<unsafe extern "C" fn() -> u32> =
    unsafe { lib.get(b"retro_api_version")? };
let v = unsafe { func() };
```

The `Symbol` borrows from the `Library`. Keep the library alive as long as you use any function from it.

**Common gotchas.**
- All FFI calls are `unsafe`. Wrap in safe abstractions early (a `Core` struct that owns the library and exposes safe methods).
- Symbol names need exact byte string: `b"retro_init"` not `"retro_init"`.
- Drop order matters: drop symbols → drop library. Done automatically if you store both in one struct.

**Docs.** https://docs.rs/libloading/latest/libloading/

---

## winit

**What it is.** Cross-platform window creation and event loop. Play uses it in the simulator only — on hw, you write directly to the framebuffer.

**Mental model (winit 0.30).** You implement `ApplicationHandler` for your state struct. You hand the trait impl to `EventLoop::run_app(...)` (or `pump_app_events` on macOS — see below). winit calls your trait methods when the OS sends events.

```rust
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create your window here. Not in main.
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => { /* handle */ }
            WindowEvent::RedrawRequested => { /* present a frame */ }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Called repeatedly. Useful for driving your own frame loop.
    }
}
```

**macOS quirk (already in Allium's `simulator.rs`).** `EventLoop::run_app_on_demand` triggers NSApplication.run/stop on every poll → 60Hz activation cycles → visible flickering. Use `EventLoopExtPumpEvents::pump_app_events` instead, which drains events without triggering app lifecycle. The cherry-picked `f9b8c9cc` does this. Read it before stage 4.

**Common gotchas.**
- Don't create the window in `main` — create it in `resumed()`. winit 0.30 enforces this.
- The event loop is the loop. Don't try to nest your own `loop {}` outside it. Either use winit's loop and integrate your frame timing in `about_to_wait`, or use `pump_app_events` and write your own loop.
- Keyboard events use `PhysicalKey(KeyCode::*)` for layout-independent codes (preferred for game input) vs `Key` for logical codes (text-entry).

**Docs.** https://docs.rs/winit/0.30/winit/ — the changelog for 0.30 is essential because the API moved away from the 0.29 closure-based event loop.

---

## softbuffer

**What it is.** A "give me a u32 framebuffer for this winit window" library. Pure CPU rendering, no GPU. Perfect for emulator output (which is already CPU-rendered pixels).

**Mental model.**

```rust
let context = softbuffer::Context::new(window.clone())?;
let mut surface = softbuffer::Surface::new(&context, window.clone())?;
surface.resize(NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap())?;
let mut buffer = surface.buffer_mut()?;
for pixel in buffer.iter_mut() { *pixel = 0x00FF0000; } // red
buffer.present()?;
```

Pixel format: `0x00RRGGBB` packed in `u32`. So if the libretro core gives you RGB565, you convert per pixel. Naive: `((r << 16) | (g << 8) | b) as u32` — fine on a Mac at 240x160-ish core resolutions. For larger cores or if iteration matters, vectorize later.

**Common gotchas.**
- Resize the surface every time the window resizes, OR pin the window size. Otherwise you get garbage pixels.
- `buffer_mut()` returns a guard; drop it (or `present()`) before doing anything else with the window.

**Docs.** https://docs.rs/softbuffer/latest/softbuffer/

---

## framebuffer

**What it is.** Wrapper around the Linux framebuffer device (`/dev/fb0`). Used on hardware to push pixels directly to the screen. Play uses this as its own game-video hot path; emulator frames should not pass through Allium's `common::display::Display` / `tiny-skia` UI renderer.

**Mental model.**

```rust
let mut fb = framebuffer::Framebuffer::new("/dev/fb0")?;
let w = fb.var_screen_info.xres;
let h = fb.var_screen_info.yres;
let bpp = fb.var_screen_info.bits_per_pixel; // typically 16 (RGB565) on Miyoo
let pitch = fb.fix_screen_info.line_length;  // bytes per row
fb.frame.copy_from_slice(&pixels);           // write all pixels
```

`fb.frame` is the mmap'd region. Writes appear on screen on the next vblank (no vsync coordination here — you can tear).

**Miyoo specifics.** Resolution is 640x480 on Miyoo Mini Plus. RGB565 native. Line length sometimes != width × 2 if there's row padding — always use `fix_screen_info.line_length` for the pitch.

**Common gotchas.**
- `/dev/fb0` may need root or specific group access. On Miyoo, your `play` runs as root via the launch chain — fine.
- The framebuffer doesn't double-buffer for you. If you partial-write a frame, you'll see partial frames. Either write atomically (one big `copy_from_slice`) or implement your own double buffer.
- Always respect both source pitch from libretro and destination pitch from framebuffer metadata. `width * bytes_per_pixel` is not a safe row stride assumption.
- Preallocate conversion buffers. Per-frame allocation in this path will show up as stutter.

**Docs.** https://docs.rs/framebuffer/latest/framebuffer/ — also useful: `man fb`, the kernel framebuffer API.

---

## cpal

**What it is.** Cross-platform audio output (and input). On macOS uses CoreAudio, on Linux uses ALSA, on Windows uses WASAPI. Play uses it in the simulator only.

**Mental model.** Three concepts: Host, Device, Stream.

```rust
let host = cpal::default_host();
let device = host.default_output_device().expect("no output");
let config = device.default_output_config()?.into();
let stream = device.build_output_stream(
    &config,
    |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        for sample in data.iter_mut() { *sample = pull_from_ring(); }
    },
    |err| eprintln!("audio error: {err}"),
    None,
)?;
stream.play()?;
```

The callback runs on cpal's audio thread. It must not block. Use a lock-free ringbuffer (`ringbuf` crate) to deliver samples from your game-loop thread.

**Common gotchas.**
- Sample format negotiation matters. The device's default may be f32, or i16, or u16. Build a stream for whichever you get; don't assume f32. Or use `SampleFormat` to template.
- Stream stops when dropped. Keep `_stream` alive in your state.
- Underruns (callback runs faster than producer fills the ring) → output silence + log; don't panic.

**Docs.** https://docs.rs/cpal/latest/cpal/ — the examples in the cpal repo are the best way to learn.

---

## alsa

**What it is.** Direct ALSA bindings for Linux audio. Lower level than cpal. Used on hardware where we want minimal overhead and direct control of buffer/period sizes.

**Mental model.** ALSA terms:
- **Frame** = one stereo sample (L+R = 4 bytes for i16 stereo).
- **Period** = how often the hw interrupts to ask for more frames.
- **Buffer** = total ring of frames the hw reads from. Buffer = N periods.

You write:

```rust
let pcm = PCM::new("default", Direction::Playback, false)?;
{
    let hwp = HwParams::any(&pcm)?;
    hwp.set_channels(2)?;
    hwp.set_rate(48000, ValueOr::Nearest)?;
    hwp.set_format(Format::s16())?;
    hwp.set_access(Access::RWInterleaved)?;
    hwp.set_buffer_size(BUFFER_FRAMES)?;
    hwp.set_period_size(PERIOD_FRAMES, ValueOr::Nearest)?;
    pcm.hw_params(&hwp)?;
}
let io = pcm.io_i16()?;
io.writei(&samples_buffer)?;  // blocks until buffer has room
```

**Sizing on Miyoo.** Sample rate often 44100 or 48000 (core-dependent). Buffer ~ 4096 frames, period ~ 1024 frames is a reasonable starting point. Smaller = lower latency = more risk of underrun.

**Common gotchas.**
- `writei` blocks. Run it on a dedicated thread that pulls from a ringbuffer; don't call from your game loop.
- ALSA error messages are cryptic. Wrap them with context.
- `default` PCM may route through pulse/pipewire on a desktop Linux. On Miyoo there's no such layer — `default` goes straight to hw.

**Docs.** https://docs.rs/alsa/latest/alsa/ — also useful: `aplay -L` on the device shows available PCMs.

---

## evdev

**What it is.** Linux input event device (`/dev/input/event*`) reader. Reads raw key/button presses from the kernel.

**Mental model.**

```rust
let mut device = evdev::Device::open("/dev/input/event0")?;
println!("device: {}", device.name().unwrap_or("?"));
loop {
    for event in device.fetch_events()? {
        if event.event_type() == EventType::KEY {
            // event.code() = the key, event.value() = 0 (release) / 1 (press) / 2 (repeat)
        }
    }
}
```

`fetch_events` blocks until events arrive. Run it on its own thread. Update atomics or a lock-free snapshot that the libretro `input_state` callback can read without locking. If you prototype with a mutex, keep it out of the hot callback before calling the stage done.

**Miyoo specifics.** Find the gamepad device by inspecting `/proc/bus/input/devices` on the device — likely `event0` or `event1`. Key codes are standard Linux input codes (`KEY_UP`, `KEY_LEFTSHIFT`, etc., aliased to gamepad meanings by the device driver). Reference Allium's existing input code under `crates/common/src/platform/miyoo/` for the exact mapping.

**Common gotchas.**
- Event codes are `u16` keys from `linux/input-event-codes.h`. Use the constants from the evdev crate (`Key::KEY_UP`, etc.).
- A device may need root. On Miyoo your binary runs as root. On a desktop Linux, add yourself to the `input` group.
- `fetch_events` returning empty is fine — just loop.

**Docs.** https://docs.rs/evdev/latest/evdev/

---

## zip

**What it is.** ZIP archive reader/writer. Play uses it to extract ROMs that are packaged as `.zip`.

**Mental model.**

```rust
let file = std::fs::File::open(zip_path)?;
let mut archive = zip::ZipArchive::new(file)?;
for i in 0..archive.len() {
    let mut entry = archive.by_index(i)?;
    if entry.is_file() {
        let mut out = std::fs::File::create(extract_path.join(entry.name()))?;
        std::io::copy(&mut entry, &mut out)?;
    }
}
```

For Play: open the zip, find the first file with a "ROM-looking" extension (`.smc`, `.sfc`, `.gb`, `.gba`, `.md`, `.nes`, etc. — match against the core's `valid_extensions` from system info), extract it to a temp dir, pass that path to `retro_load_game`.

**Common gotchas.**
- Some old-school ROM zips contain readme files, BIOS dumps, etc. Filter by extension.
- Extract to `/tmp/play-<pid>` or similar — easy to clean up on exit.

**Docs.** https://docs.rs/zip/latest/zip/

---

## ringbuf (used in stage 5)

**What it is.** Lock-free single-producer single-consumer ring buffer. Used to ferry audio samples from the libretro callback thread to the audio output thread.

**Mental model.**

```rust
let rb = HeapRb::<f32>::new(8192);
let (mut producer, mut consumer) = rb.split();
// producer.push(sample) — from the audio_sample_batch callback
// consumer.pop() — from the cpal/ALSA callback
```

Both `push` and `pop` are non-blocking. They return `Option`/bool to indicate full/empty.

**Common gotchas.**
- Size matters: too small → frequent overruns/underruns; too big → audio latency. ~100ms worth of frames is a reasonable starting point.
- Producer in one thread, consumer in another — never share a half across threads.

**Docs.** https://docs.rs/ringbuf/latest/ringbuf/

---

## tokio (used in stage 8)

**Reference, not primer.** You already have `crates/common/src/retroarch.rs` showing how to bind a UDP socket and send/recv with `tokio::net::UdpSocket`. Mirror that pattern in Play for the listener side. Read that file before writing your listener.

Spawn the listener with `tokio::spawn` from a `tokio::main` runtime, or build a single-threaded runtime if Play stays mostly synchronous.

**Docs.** https://tokio.rs/tokio/tutorial — but you only need the UDP and `spawn` parts.

---

## Where official docs live

- All Rust crates: `https://docs.rs/<crate>/<version>/<crate>/`
- libretro: https://docs.libretro.com/ + libretro.h itself
- Linux fb / evdev / ALSA: kernel.org documentation, `man` pages on the device

When in doubt, read the docs of the version you're actually using (`cargo tree -p play | grep <crate>` to confirm the version).
