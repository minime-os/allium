# Building Play

Play is an Allium-native libretro runtime. It lives inside Allium, shares Allium's paths, platform layer, launcher handoff, and menu command protocol, and avoids reimplementing pieces that already belong in `common`.

This plan walks you through building Play from scratch, stage by stage, until it can become an opt-in daily-use runtime on a Miyoo Mini Plus. PicoArch and MinArch are references for behavior, edge cases, and pitfalls only. Do not copy their global-state C architecture, and do not treat MinArch/MiniUI save-state compatibility as a requirement.

Verified target: Miyoo Mini Plus. Design target: small backend seams that can support a future RG35xxSP port when Allium is ported there. Do not add RG35xxSP code now.

It is a learning plan, not a spec. Each stage has a clear goal, a small list of new things to learn, manual + automated checks, and a commit. Stages build on each other — don't skip ahead.

---

## How to read this plan

Each stage looks like:

> ### Stage N: Title
>
> **Goal.** One sentence. The visible result you should see when this stage closes.
>
> **What you'll learn.** New concepts and APIs introduced in this stage.
>
> **Prerequisites.** What must be true before you start.
>
> **Targets.** Sim, hw, or both. Pragmatic — not every stage exercises both.
>
> **Steps.** Numbered. With file paths and key APIs called out.
>
> **Smoke checks.** Concrete: command, expected outcome.
>
> **Done when.** A short checklist. All boxes ticked → stage closes.
>
> **Commit message suggestion.** Match Allium's existing conventional style.

### Conventions

- Cargo features: `simulator` (sim build) and `miyoo` (hw build). Mirror Allium.
- Direct commits to `main`. No PR ceremony. Tag completed stages: `git tag stage-N-done && git push origin --tags`.
- Conventional commit prefixes: `feat:`, `fix:`, `refactor:`, `chore:`, `build:`, `docs:`.
- No AI co-author trailers, ever.
- Companion docs: [LIBRARIES.md](./LIBRARIES.md) for crate primers, [PLAYBOOK.md](./PLAYBOOK.md) for daily commands.

### Architecture guardrails

These rules keep Play Allium-native while the stages stay incremental:

- Use Allium shared surfaces where they fit: `common::constants` for paths, `common::platform` for device/input concepts, `common::retroarch` for command protocol, and `GameInfo` for launcher handoff.
- Keep `main.rs` as the process boundary: logging, args, top-level error reporting. Put behavior in library modules.
- Grow modules by responsibility, not by forecast. Create a file only when it owns real behavior or tests. Start with flat files; split into directories only when real submodules exist. Avoid empty `mod.rs` files and empty backend folders.
- Good early files, when they become useful: `args.rs` for CLI parsing, `paths.rs` for Allium path resolution, `session.rs` for the `PlaySession` runtime shell.
- `PlaySession` owns runtime state. It is the thing callbacks, command handling, saves, audio, video, and input route into.
- `paths.rs` owns Play paths. Config/state live under `ALLIUM_BASE_DIR/config` and `ALLIUM_BASE_DIR/state`; saves/states live under Allium's save/profile area. ROM directories remain user content, not frontend config storage.
- `common::retroarch` owns both sending commands and parsing wire commands. Play should not grow its own private RetroArch UDP dialect.

### Performance guardrails

Tune later, but avoid early design choices that make performance hard:

- Do not send game frames through Allium's `Display` / `tiny-skia` UI path. Play has its own hot video path over `softbuffer` in the simulator and the Linux framebuffer on hardware.
- No locks in video/audio hot callbacks.
- No per-frame heap allocation in render/audio paths. Preallocate frame and audio buffers.
- Always respect source and destination pitch.
- Keep unsafe FFI narrow: raw libretro bindings stay behind a small wrapper.
- Use concrete backend types in hot loops. Traits are for setup/seams, not per-pixel/per-sample dispatch.
- Log frame timing and audio underruns. Do not block v1 on hard PSX/full-speed benchmarks; correctness and architecture come first.

---

## Stage 0: Bring-up

**Goal.** Prove the sim → build → deploy loop works on your Mac and Miyoo before writing a single line of Play code.

**What you'll learn.** The repo's macOS setup path, how Allium's Makefile builds the full distribution, where packaged binaries land, and how to deploy to an SD card mounted under `/Volumes` on macOS.

**Prerequisites.** macOS, Miyoo Mini Plus with an SD card already running Allium or an extracted Allium release.

**Targets.** Both. You want simulator and hardware working before Play work starts.

### Steps

1. Follow the macOS setup path from the repo README. From the repo root, run:
   ```bash
   ./scripts/setup-mac.sh
   ```
   This installs the normal Mac prerequisites for Allium development.
2. Verify your host tools are available:
   ```bash
   cargo --version
   make --version
   zig version
   cargo zigbuild --version
   patchelf --version
   ```
3. Start with the simulator, because it is the fastest feedback loop on Mac:
   ```bash
   make simulator bin=allium-launcher
   ```
   A window should open. Navigate the launcher. Quit cleanly.
4. Build the full Miyoo distribution:
   ```bash
   make all
   ```
   First run is slow because it builds Allium, RetroArch, helper binaries, themes, and the packaged `dist/` tree.
5. Inspect `dist/`:
   ```bash
   ls dist/.allium/bin/
   file dist/.allium/bin/alliumd
   ```
   You should see `alliumd`, `allium-launcher`, and the other packaged binaries. `file` should report an ARM ELF.
6. Insert the Miyoo SD card into your Mac. Find its mount point:
   ```bash
   ls /Volumes/
   ```
   Note the label, for example `/Volumes/MIYOO`.
7. For quick testing, create a git-ignored deploy config:
   ```bash
   cp local.mk.example local.mk
   ```
   Then set:
   ```make
   SDCARD_PATH=/Volumes/<YOUR_LABEL>
   ```
8. Deploy update files to the SD card:
   ```bash
   make deploy
   ```
   Eject the card properly from macOS, insert it into the Miyoo, and boot.
9. Confirm Allium starts and you can navigate the launcher on hardware. You are verifying the existing build/deploy loop, not testing Play yet.

### Smoke checks

- `./scripts/setup-mac.sh` finishes and the required tools are available.
- `make simulator bin=allium-launcher` opens the simulator window and it is usable.
- `make all` exits 0 and `dist/.allium/bin/alliumd` exists as an ARM ELF.
- `make deploy` copies files to `/Volumes/<LABEL>`.
- Miyoo boots Allium after deploy.

### Done when

- [x] macOS dev tools are installed via `./scripts/setup-mac.sh`
- [x] Sim launcher runs on Mac
- [x] `make all` produces a deployable `dist/` tree
- [x] You know your SD card's `/Volumes/<LABEL>` path
- [x] Miyoo boots from your build

No commits in this stage.

---

## Stage 1: Scaffold

**Goal.** A `play` binary that builds for both targets, ships in `make all`, parses CLI args, uses Allium path constants, and writes a log file on the device. It does nothing emulator-specific yet.

**What you'll learn.** Cargo workspace layout, `[features]` and optional deps, conditional compilation (`#[cfg]`), `simple_logger` setup, Allium's hand-rolled CLI parsing pattern, and how Play starts sharing `common` from day one.

**Prerequisites.** Stage 0 done.

**Targets.** Both.

### Steps

This stage is intentionally walked through in small substeps. Each substep should compile and be commit-worthy on its own. Do not change launcher behavior in this stage; RetroArch remains the runtime until Play can run games.

#### 1A. The thinnest crate that compiles

Create `crates/play/Cargo.toml` and `crates/play/src/main.rs` as the thinnest binary. Try `cargo build -p play`. It will fail until the workspace registers it.

#### 1B. Register the workspace member

Edit `Cargo.toml` (root), add `"crates/play"` to `[workspace] members`. Now `cargo build -p play` works.

#### 1C. Share Allium from the start

Add `common` as a dependency of Play. Use it early, even if only for path constants and feature consistency. Add a planned `ALLIUM_PLAY` constant beside `ALLIUM_RETROARCH` in `common::constants` when launcher handoff needs it later; for now, document the path shape and avoid changing launch behavior.

Use Allium paths instead of hardcoded strings. Logs should resolve through `ALLIUM_BASE_DIR.join("logs/play.log")`, which is normally `/mnt/SDCARD/.allium/logs/play.log` on hardware and the simulator `.allium` dir in sim.

#### 1D. Ship in `make all`

Open the Makefile. Find the `cargo zigbuild` lines (release and debug). Add `--bin=play`. Update `package-build` so `dist/.allium/bin/play` is copied. Then `make all` should produce a deployable `play` binary.

Verify: `file dist/.allium/bin/play` → ARM 32-bit ELF. Deploy only to confirm the binary lands on the SD card.

#### 1E. CLI parsing

Allium doesn't use `clap` for small binaries; it hand-rolls. Look at `crates/say/src/main.rs` or `crates/show/src/main.rs`.

Parse `--core <path> --rom <path> --core-id <id>`. Reject anything else with a clear error. Keep parsing small in this stage, but write tests now. If parsing starts to crowd `main.rs`, move it to `args.rs`; create that file because it owns real behavior and tests.

#### 1F. Logging to a file on hardware

Add `simple_logger` and `log` from workspace deps. On `simulator` builds, log to stderr. On `miyoo` builds, redirect stderr to `ALLIUM_BASE_DIR.join("logs/play.log")` and create the directory if missing.

Add the `simulator` and `miyoo` features now. Default empty — fail fast with a clear `compile_error\!` if neither is set.

### Smoke checks

- **Sim:** `cargo run -p play --features=simulator -- --core foo --rom bar --core-id baz` prints parsed args, exits 0. Stderr shows log lines.
- **Sim, missing args:** missing `--rom` produces a clear error, exits non-zero.
- **Tests:** `cargo test -p play args` covers valid args, missing values, duplicate args, and unknown flags.
- **Hw:** `make all`, deploy. Run `/mnt/SDCARD/.allium/bin/play --core foo --rom bar --core-id baz`. The log appears at `.allium/logs/play.log` under Allium's base dir.

### Done when

- [x] `cargo build -p play --features=simulator` succeeds on Mac
- [x] `make all` produces `dist/.allium/bin/play`
- [x] CLI parses `--core --rom --core-id`, with tests for good and bad inputs
- [x] Logging uses Allium path constants, not hardcoded SD paths
- [x] Log file appears on the SD card after running on hw
- [x] Launcher behavior is unchanged
- [x] Default-features build fails with a `compile_error\!` saying "pick `simulator` or `miyoo`"

### Commit message suggestion

`feat(play): scaffold crate with Allium paths and CLI parsing`

---

## Stage 1.5: Allium-native skeleton

**Goal.** Put the minimum architecture seams in place before unsafe FFI enters, without creating empty scaffolding.

**What you'll learn.** How to keep Rust modules small, when to split files, where Allium-owned paths and command protocols live, and how `PlaySession` will become the runtime owner.

**Prerequisites.** Stage 1 done.

**Targets.** Both.

### Steps

1. Keep `main.rs` thin: setup logging, parse args, call library code, print errors.
2. Create files only when they own behavior/tests:
   - `args.rs` if CLI parsing has moved out of `main.rs`.
   - `paths.rs` for `PlayPaths` / path resolver helpers.
   - `session.rs` when a `PlaySession` shell starts owning args, paths, and future runtime state.
3. Add path resolver tests. They should prove config/state/log paths come from Allium base dirs and that ROM dirs are not used for frontend config.
4. Add a `PlaySession` shell only if it owns real state already. It can be boring: args + paths + lifecycle methods that currently log what they would do.
5. Extend `common::retroarch` with wire parsing tests before the UDP server exists. The sender and parser should live in the same module so menu/alliumd/Play cannot drift.

### Smoke checks

- `cargo test -p play` runs CLI/path tests.
- `cargo test -p common retroarch` runs command parser tests.
- No empty backend dirs or empty `mod.rs` files exist.

### Done when

- [x] `main.rs` is a process boundary, not the runtime implementation
- [x] Any new module owns behavior and tests
- [x] `paths.rs` resolves Allium-owned paths
- [x] `common::retroarch` has a plan/test surface for parsing incoming wire commands

### Commit message suggestion

`refactor(play): add Allium-native runtime skeleton`

---

## Stage 2: Load a libretro core

**Goal.** `play --core <path>` loads a libretro core via `dlopen`, queries its system info, prints name + version + supported file extensions, and exits.

**What you'll learn.** What libretro is and what a "core" is, dynamic loading with `libloading`, the libretro C ABI, manual `bindgen` workflow, unsafe FFI boundaries in Rust.

**Prerequisites.** Stage 1.5 done. See [LIBRARIES.md → libretro](./LIBRARIES.md#libretro) for a primer.

**Targets.** Both.

### Steps

#### 2A. Vendor `libretro.h`

Download a stable `libretro.h` into `crates/play/vendor/libretro.h`. Pin to a specific commit if you want reproducibility. Vendor to the repo, not as a build-script download.

#### 2B. Generate Rust bindings by hand

Install bindgen if needed: `cargo install bindgen-cli`. Run it manually and commit the generated file:

```bash
cd crates/play
bindgen vendor/libretro.h \
    --allowlist-type 'retro_.*' \
    --allowlist-function 'retro_.*' \
    --allowlist-var 'RETRO_.*' \
    --no-layout-tests \
    -o src/libretro_sys.rs
```

No `build.rs`. Generated bindings are part of the repo so normal builds stay deterministic and do not require libclang.

Isolation rule: keep `libretro_sys` imports near the core loader, callback bridge, and session boundary. The rest of Play should use Play-owned frame/video types, `AvInfo`, and `SystemInfo`. This keeps unsafe ABI details narrow and makes a later handwritten ABI migration cheap.

#### 2C. Load the core dynamically

Add `libloading`. Create `core.rs`. Use `libloading::Library::new(core_path)` to load and resolve the symbols needed this stage:

- `retro_init`
- `retro_get_system_info`
- `retro_deinit`
- `retro_api_version`

Wrap them in a struct that owns the `Library` and function pointers. Drop impl calls `retro_deinit`.

Call `retro_init()`. Call `retro_get_system_info(&mut info)`. Convert raw strings into safe `SystemInfo` before returning to the rest of Play. Enforce `retro_api_version() == 1`.

#### 2D. Wire it into `main.rs`

`main.rs` parses args and calls library code. It does not reach into `libretro_sys`.

### Smoke checks

- **Sim:** Run with a macOS libretro core. It prints name, version, extensions, and `api_version=1`.
- **Hw:** Run with a `.so` core from the SD card. The log shows the same metadata.
- **Boundary:** `grep -R "libretro_sys" crates/play/src` shows imports only in the FFI wrapper/callback bridge.

### Done when

- [x] `vendor/libretro.h` and `src/libretro_sys.rs` committed
- [x] No `build.rs` exists for binding generation
- [x] `Core` loads + drops a core cleanly
- [x] Metadata is converted into Play-owned safe types
- [x] `retro_api_version() == 1` enforced
- [x] Raw binding imports are isolated

### Commit message suggestion

`feat(play): load libretro cores behind FFI wrapper`

---

## Stage 3: First frame

**Goal.** `play` loads a ROM into a core, runs enough unpaced frames to capture a useful framebuffer, and writes it to a PPM file. No window, no audio yet.

**What you'll learn.** libretro callbacks, `retro_load_game`, `retro_run`, the video refresh callback, pixel format conversion, and how to route C callbacks into one active `PlaySession` safely.

**Prerequisites.** Stage 2 done.

**Targets.** Both.

### Steps

#### 3A. Implement the callback bridge

libretro callbacks are process-global function pointers. Use one process-global pointer to the active session/callback state, not a global `Mutex<Option<State>>`.

Invariant:

- one core per process;
- the active-session pointer is set before callbacks can fire;
- the pointer remains valid while callbacks can fire;
- the pointer is cleared before session state is dropped.

The `extern "C"` callback functions should be tiny. They convert raw args into safe Play-owned types and delegate to methods on `PlaySession` / callback state. Do not lock in video/audio callbacks.

Define:

- `environment(cmd, data) -> bool`
- `video_refresh(data, width, height, pitch)`
- `audio_sample(left, right)`
- `audio_sample_batch(data, frames) -> usize`
- `input_poll()`
- `input_state(port, device, index, id) -> i16`

Hook them with `retro_set_*` before `retro_init`.

#### 3B. Load the ROM

Read the ROM into memory only if the core needs data. Construct `retro_game_info` with path/data/size per libretro expectations. Call `retro_load_game(&info)`. Query AV info and log geometry, fps, and pixel format.

Play's v1 video path is RGB565-first, matching MinArch's performance-oriented approach. Accept `RETRO_PIXEL_FORMAT_RGB565`; also accept `RETRO_PIXEL_FORMAT_XRGB8888` for frame dumps because common macOS cores like Nestopia request it before content loads.

#### 3C. Run one frame, capture, dump

Call `retro_run()` for a short dump-only warmup. In `video_refresh`, copy pixels into a frame buffer, respecting `pitch`. Convert RGB565 or XRGB8888 to RGB888 and write a PPM at `--dump-frame <path>`.

Then call `retro_unload_game`, clear the active-session pointer, drop state, and exit.

### Smoke checks

- **Sim/hw:** `--dump-frame /tmp/first.ppm` or an SD path creates a readable PPM with recognizable game graphics.
- Repeated runs do not segfault on exit.
- Frame copy handles `pitch \!= width * bytes_per_pixel`.

### Done when

- [x] All callbacks wired and delegated through the active session pointer
- [x] Active pointer lifetime invariant is documented near the unsafe code
- [x] PPM opens correctly and shows recognizable graphics
- [x] Sim produces a recognizable PPM from a real core/ROM; hw visual comparison is deferred until a Miyoo smoke-test path is available

### Commit message suggestion

`feat(play): load ROM and dump first frame`

---

## Stage 4A: Game loop and timing

**Goal.** Replace the single-frame run with a paced frame loop that can run for `--frames N` or forever, with clean shutdown.

**What you'll learn.** Core-reported FPS, simple frame pacing, timing logs, and deterministic smoke checks.

**Prerequisites.** Stage 3 done.

**Targets.** Both.

### Steps

1. Loop over `retro_run()` at the core's reported fps.
2. Add `--frames <N>` for capped runs. Default: forever.
3. Sleep between frames using a simple frame-time clock. No vsync yet.
4. Add clean shutdown for Ctrl-C / quit flag: unload game, clear callback pointer, drop core.
5. Log average frame time over capped runs.

### Smoke checks

- `cargo run -p play --features=simulator -- ... --frames 600` exits cleanly.
- `time ... --frames 600` is roughly 10 seconds for a 60 fps core.

### Done when

- [x] Capped frame loop works
- [x] Forever loop exits cleanly on quit
- [x] Frame timing is logged

### Commit message suggestion

`feat(play): add paced frame loop`

---

## Stage 4B: Native-size simulator video

**Goal.** A winit + softbuffer simulator window shows native-size game frames. No scaling yet.

**What you'll learn.** winit 0.30, softbuffer, frame conversion tests, and why game video avoids Allium's UI `Display` path.

**Prerequisites.** Stage 4A done. See [LIBRARIES.md → winit](./LIBRARIES.md#winit) and [softbuffer](./LIBRARIES.md#softbuffer).

**Targets.** Simulator.

### Steps

1. Add simulator video behind the `simulator` feature.
2. Create a window at the core's reported geometry.
3. Convert core pixels into softbuffer's `0x00RRGGBB` buffer. Respect source pitch.
4. Add focused conversion tests if the converter owns enough logic to test.
5. Pump winit events using the existing Allium simulator pattern.

This is a Play-specific video path. Do not route emulator frames through `common::display::Display` / `tiny-skia`.

### Smoke checks

- Simulator window shows native-size game frames.
- Window close exits cleanly.
- Conversion tests pass, if added.

### Done when

- [ ] Native-size sim video works
- [ ] No per-frame allocation in conversion/present path
- [ ] No use of Allium `Display` for game frames

### Commit message suggestion

`feat(play): render native video in simulator`

---

## Stage 4C: Native-size Miyoo framebuffer video

**Goal.** The Miyoo framebuffer at `/dev/fb0` shows native-size game frames. No scaling yet.

**What you'll learn.** Linux framebuffer geometry, mmap writes, RGB565, and pitch-safe copies on hardware.

**Prerequisites.** Stage 4A done. See [LIBRARIES.md → framebuffer](./LIBRARIES.md#framebuffer).

**Targets.** Miyoo.

### Steps

1. Open `/dev/fb0` with the framebuffer crate.
2. Read screen geometry and pitch from framebuffer metadata.
3. Allocate a reusable output buffer once.
4. Copy/convert each frame into the framebuffer mmap. Respect both source pitch and destination pitch.
5. Accept tearing as a known limitation for this stage; do not add double buffering yet.

### Smoke checks

- Launch manually on Miyoo. Game appears on screen.
- `--frames N` returns to shell/log after the expected duration.

### Done when

- [ ] Native-size hw video works
- [ ] Pitch-aware copies work
- [ ] No per-frame allocation in the present path

### Commit message suggestion

`feat(play): render native video to framebuffer`

---

## Stage 4D: Basic scaling

**Goal.** Add basic scaling modes after native video works on both targets.

**What you'll learn.** Separation between presentation and scaling, integer/native vs aspect vs fullscreen, and when a backend abstraction is earned.

**Prerequisites.** Stages 4B and 4C done.

**Targets.** Both.

### Steps

1. Add `Native`, `Aspect`, and `Fullscreen` scale modes.
2. Preallocate scaled buffers. Avoid per-frame allocation.
3. Use simple point-sampling first. `fast_image_resize` can come later if needed.
4. If two concrete video backends now share real behavior, extract a `VideoBackend` seam. Keep hot loops concrete where possible.

Scaling is separate from first visible video. Do not block 4B/4C on scale quality.

### Smoke checks

- Sim and Miyoo show all three modes visibly differently.
- Frame timing stays stable enough for daily use.

### Done when

- [ ] Basic scaling works on both targets
- [ ] Video abstraction exists only if it removes real duplication
- [ ] Hot path avoids trait dispatch where practical

### Commit message suggestion

`feat(play): add basic video scaling`

---

## Stage 5: Audio

**Goal.** Game audio plays through cpal in the simulator and ALSA on Miyoo. Fast-forward mutes audio until later.

**What you'll learn.** cpal streams, ALSA buffers/periods, libretro audio callbacks, and ring buffers between the game loop and audio output.

**Prerequisites.** Stage 4 done. See [LIBRARIES.md → cpal](./LIBRARIES.md#cpal), [alsa](./LIBRARIES.md#alsa).

**Targets.** Both.

### Steps

#### 5A. Shared audio queue

Use a ring buffer between the libretro audio callback and the audio output thread/callback. The libretro callback pushes interleaved i16 stereo frames. Output pulls. Underrun emits silence and logs a rate-limited warning.

Do not resample in the first pass. If the core sample rate does not match the selected device rate, warn clearly or fail clearly; choose one and document it in the log. Fast-forward mutes audio for v1 instead of speeding/resampling it.

#### 5B. Sim audio: cpal

Open the default cpal output device. Build a stream for the device's actual sample format. Keep the stream alive in session state.

#### 5C. Hw audio: ALSA

Open ALSA PCM (`default` first, fall back only if needed). Set channels, format, sample rate, buffer size, and period size. Run writes on an output thread that pulls from the ring buffer.

#### 5D. Optional seam

Extract an audio backend seam only after both implementations work and share enough behavior to justify it.

### Smoke checks

- **Sim:** Audio plays with no constant clicks/pops.
- **Hw:** Audio plays through Miyoo speakers.
- **Stress:** Five minutes under normal CPU load has no constant underrun spam.
- **Mismatch:** Sample-rate mismatch logs a clear warning/error.

### Done when

- [ ] Audio plays in both sim and hw
- [ ] Ring buffer separates libretro callback from output
- [ ] No resampling hidden in v1
- [ ] Fast-forward mutes audio

### Commit message suggestion

`feat(play): play libretro audio`

---

## Stage 6: Input

**Goal.** Game responds to controls using Allium's existing platform abstraction first.

**What you'll learn.** `common::platform::Platform`, `common::platform::Key`, libretro joypad IDs, and when to drop below an existing abstraction.

**Prerequisites.** Stage 5 done.

**Targets.** Both.

### Steps

1. Depend on `common::platform::Platform` and `Key` for both simulator and Miyoo input.
2. Poll or receive keys through the existing platform layer.
3. Map `Key` values to libretro joypad IDs.
4. Store joypad state in a small structure that `input_state` can read without hot-path locking.
5. Add lower-level winit/evdev input only if measured latency, missing keys, or lifecycle bugs prove the shared abstraction is not enough.

### Smoke checks

- **Sim:** Keyboard controls work through Allium's simulator platform layer.
- **Hw:** Miyoo buttons work through Allium's Miyoo platform layer.
- MENU is reserved for Allium integration; do not make it a game button.

### Done when

- [ ] Sim playable from keyboard
- [ ] Hw playable from physical controls
- [ ] `Key -> libretro joypad` mapping is tested or table-driven enough to inspect
- [ ] No custom evdev/winit path exists without a need

### Commit message suggestion

`feat(play): map Allium input to libretro joypad`

---

## Stage 7: Persistence primitives

**Goal.** SRAM plus explicit save/load state primitives work. Autosave/autoload polish comes after menu command integration.

**What you'll learn.** The libretro memory API, `retro_serialize` / `retro_unserialize`, and Allium-owned save/state paths.

**Prerequisites.** Stage 6 done.

**Targets.** Both.

### Steps

#### 7A. SRAM read/write

Resolve SRAM paths through Play's Allium path resolver. Do not store frontend state next to ROMs. On core load, copy SRAM into `RETRO_MEMORY_SAVE_RAM` if a save exists. On clean exit, write SRAM back.

#### 7B. Save/load state primitives

Implement functions that save/load the current slot using `retro_serialize_size`, `retro_serialize`, and `retro_unserialize`. Trigger them from simulator hotkeys or a test harness for now. Hardware menu triggers come next stage.

#### 7C. State slots

Track state slot 0-9. Save/load operate on the current slot. Keep autosave slot support ready, but do not polish autoload UX before commands can request save/load.

### Smoke checks

- **Sim/hw:** SRAM persists across clean restarts.
- Manual save/load to slots 1, 2, 3 works.
- Save/state files land under Allium save/profile paths, not ROM dirs.

### Done when

- [ ] SRAM persists across runs
- [ ] Save/load state works on at least 3 slots
- [ ] Paths are Allium-owned and tested

### Commit message suggestion

`feat(play): add SRAM and state primitives`

---

## Stage 8: Allium integration and v1 daily use

**Goal.** Play can be enabled through config, launched by `allium-launcher`, controlled by `allium-menu`, and used for a normal play session with ZIP ROMs, basic scaling, fast-forward mute, and autosave/autoload.

**What you'll learn.** Launcher handoff, Allium config, the RetroArch UDP command protocol in `common::retroarch`, tokio UDP, and integration without rewriting Allium around Play.

**Prerequisites.** Stages 1-7 done.

**Targets.** Both.

This stage is the largest. Split aggressively over multiple sessions.

### Required v1 daily-use scope

- launch from Allium
- video
- audio
- input
- SRAM
- save/load state
- pause/unpause
- quit
- autosave/autoload
- fast-forward mute
- basic scaling
- ZIP ROMs

Deferred: disk control, screenshots, effects, shaders, cheats, rewind, overlays, netplay, debug overlay, sharpness filters, overclock, fast-forward audio resampling.

### Steps

#### 8A. Opt-in launcher handoff

Add a Play config file at `ALLIUM_BASE_DIR/config/play.toml`:

```toml
[play]
enabled = false
```

Missing config means RetroArch. `enabled = false` means RetroArch. Only when enabled should `allium-launcher` use Play for RetroArch cores. Do not make Play the default until menu/save/quit flow works.

When enabled, the launcher passes `--core <core_path> --rom <rom_path> --core-id <core_id>` to `ALLIUM_PLAY`.

#### 8B. UDP listener

Add a UDP command server that binds the existing RetroArch socket constant. Incoming datagrams parse through `common::retroarch::RetroArchCommand`, using parser code added beside the existing sender.

Implement at minimum:

- `PAUSE_TOGGLE`, `PAUSE`, `UNPAUSE`
- `SAVE_STATE`, `LOAD_STATE`, `STATE_SLOT_PLUS`, `STATE_SLOT_MINUS`
- `FAST_FORWARD`, `FAST_FORWARD_HOLD`
- `RESET`
- `QUIT`
- `GET_INFO` reply with disk/state info in the existing wire shape

UDP commands should set command flags or enqueue control actions into `PlaySession`; they should not lock hot audio/video callbacks.

#### 8C. allium-menu integration

Run `allium-launcher`, pick a game with Play enabled, press MENU, and verify `allium-menu` commands work. The menu should not care whether RetroArch or Play is listening.

#### 8D. Autosave/autoload polish

After explicit save/load works through UDP/menu, add autosave on clean exit and autoload on launch. Default on unless config says otherwise.

#### 8E. ZIP support

Use the workspace `zip` dep. When `--rom` ends in `.zip`, extract to a temp dir, find the inner ROM, load that path, and clean up temp files on exit.

#### 8F. Fast-forward mute

For toggle and hold commands, skip frame sleep while active. Mute/drop audio during fast-forward. Cap speed in config later if needed.

#### 8G. Basic scaling command

Expose scale cycling through UDP after Stage 4D scaling exists.

### Smoke checks

- Missing `play.toml` → launcher still uses RetroArch.
- `enabled = false` → launcher still uses RetroArch.
- `enabled = true` → launcher launches Play.
- MENU Save State, Load State, Fast-Forward, Pause/Resume, and Quit work.
- ZIP ROM launches.
- Autosave/autoload restores a normal session.

### Done when

- [ ] Launcher → Play handoff works behind opt-in config on both targets
- [ ] Missing/disabled config keeps RetroArch behavior
- [ ] allium-menu UDP commands work for pause/save/load/FF/quit
- [ ] ZIP ROMs play
- [ ] Fast-forward toggle and hold mute audio
- [ ] Autosave/autoload works
- [ ] Basic scaling can be selected

### Commit message suggestion

Many small commits, one per substep. End with `git tag stage-8-done`.

---

## Done — v1 daily-use Play

At this point Play is an opt-in Allium-native libretro runtime on Miyoo Mini Plus for everyday use. It does not need MinArch/MiniUI save/state/path compatibility to be done.

Deferred parity and polish live in the bonus stages below. None of these are blockers for daily play.

---

## Bonus stages (deferred parity and polish)

### Bonus A: Fast-forward audio resample

Currently FF mutes audio. Implement libsamplerate-rs or `rubato` resampling so audio speeds up cleanly with video.

### Bonus B: Sharpness modes

MinArch has Soft / Sharp / etc. Pixel filter passes. Implement in the scaling step.

### Bonus C: Screen effects

MinArch has CRT-style scanline / phosphor effects. Implement as a post-scale pass.

### Bonus D: Threaded video

MinArch can run the video callback on a worker thread to overlap with `retro_run`. Worth ~5-15% perf on heavy cores. Adds threading complexity — only do this if you see specific cores struggle.

### Bonus E: Overclock control

MinArch can switch the Miyoo CPU governor to `performance` and adjust min freq. Sysfs poking. Useful for demanding cores.

### Bonus F: 8888 → 565 downsample

For cores that emit XRGB8888 instead of RGB565. Add this later as a non-v1 pixel conversion path; keep the main video path RGB565-first.

### Bonus G: Debug overlay

FPS, frame time, audio underrun count, cpu freq. Render as a small text overlay using rusttype (workspace dep). Toggle with a UDP command.

### Bonus H: Per-core controller mappings

Some cores want different button layouts. Store per-core overrides in a config file. Apply in the input_state mapper.

---

## Final notes

- This plan was written after grilling the design tree exhaustively. If you find a stage feels wrong as you start it, that's a signal — re-grill that stage's slice before pushing through.
- The archived old Play crate at `jheronimus/play` is a reference, not a target. Don't copy code from it; re-derive it. The point is the learning, not the destination.
- Every stage is reversible. If stage N drives you nuts, drop the commits, change the approach, re-commit. Tags are cheap.
