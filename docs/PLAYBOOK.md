# Daily playbook

Routine commands. The plan ([PLAN.md](./PLAN.md)) tells you what to build; this tells you how to do the build/test/deploy/commit chores around it.

---

## One-time setup

These ran in Stage 0 of the plan. Repeat only if you nuke your machine.

```bash
brew install --cask orbstack       # if not installed
docker pull mholdg16/miyoomini-toolchain:latest
cargo install bindgen-cli          # for Stage 2
brew install llvm                  # for bindgen's libclang
```

Find your SD card label once: insert SD, run `ls /Volumes/`. Note the name (e.g., `MIYOO`). Throughout this doc, `/Volumes/MIYOO` is a stand-in — substitute yours.

---

## Pre-flight before any build

```bash
docker info >/dev/null || open -a OrbStack && sleep 5
```

OrbStack must be running before any `make build`.

---

## Simulator workflow (fast iteration)

Run the launcher in the simulator (a winit window opens):

```bash
cargo run -p allium-launcher --features=simulator
```

Run Play standalone in the simulator (after Stage 1):

```bash
cargo run -p play --features=simulator -- \
    --core /path/to/core.dylib \
    --rom /path/to/game.rom \
    --core-id snes9x
```

After Stage 4A, use capped runs for timing smoke checks:

```bash
time cargo run -p play --features=simulator -- \
    --core /path/to/core.dylib \
    --rom /path/to/game.rom \
    --core-id snes9x \
    --frames 600
```

For a 60 fps core, 600 frames should take about 10 seconds. Treat large drift as a pacing bug, not as a performance benchmark.

For libretro core builds for macOS: get them from the libretro buildbot (search "libretro buildbot mac"). Pin one core per system you test (snes9x, gambatte, picodrive, etc.).

Tighten the dev loop with `cargo watch` if you want auto-rebuild:

```bash
cargo install cargo-watch          # one-time
cargo watch -x 'run -p play --features=simulator -- ...'
```

---

## Hardware build + deploy loop

Build the ARM binaries:

```bash
make build
```

First build is slow (~5-10 min cross-compile). Incremental rebuilds are fast (~30s).

Insert the Miyoo's SD card into your Mac. Copy the build over:

```bash
cp -r dist/* /Volumes/MIYOO/
```

Eject:

```bash
diskutil eject /Volumes/MIYOO
```

Reinsert into the Miyoo. Boot. Test.

After running on the device, eject the card again to read the log:

```bash
less /Volumes/MIYOO/.allium/logs/play.log
```

If you only changed Play and don't want to recopy everything:

```bash
cp dist/.allium/bin/play /Volumes/MIYOO/.allium/bin/play
```

---

## Play opt-in config

Play is not the default runtime while it is under construction. Missing config means the launcher keeps using RetroArch. Disabled config also means RetroArch.

When Stage 8 reaches launcher handoff, enable Play with:

```toml
# /mnt/SDCARD/.allium/config/play.toml on Miyoo
# simulator: <sim ALLIUM_BASE_DIR>/config/play.toml
[play]
enabled = true
```

Set `enabled = false` or remove the file to return to RetroArch. Do not leave Play enabled for normal launcher testing until menu save/load/quit works.

---

## Quality gates (before each commit)

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p play
```

If clippy fires on existing Allium code (not yours), don't fix the existing code — that's scope creep. Just make sure your changes don't add new warnings.

For a faster pre-commit, run only on Play:

```bash
cargo fmt --all -- --check
cargo clippy -p play --all-targets -- -D warnings
cargo test -p play
```

---

## Git hygiene

Conventional commits, lowercase prefixes, scoped to `play` when relevant:

```
feat(play): add continuous game loop
fix(play): handle RGB565 pitch padding correctly
refactor(play): extract VideoBackend trait
chore(play): bump libretro.h to 2026-04 snapshot
build: add play to make build target list
docs: update playbook with new deploy step
```

Tag completed stages and push the tag:

```bash
git tag stage-N-done
git push origin --tags
```

NEVER add `Co-Authored-By: Claude` (or any AI tool) to commits.

When syncing with upstream Allium:

```bash
git fetch upstream
git log main..upstream/main --oneline   # see what's new upstream
git rebase upstream/main                 # apply your work on top
# resolve conflicts (likely in launcher code if upstream changed it)
git push origin main --force-with-lease  # only after rebase
```

---

## Logs

Play logs under Allium paths. On hardware that is normally `/mnt/SDCARD/.allium/logs/play.log`; on simulator builds, use the simulator Allium base dir or stderr depending on the stage.

**Sim:** stderr. Pipe to a file if a session is going to be long:

```bash
cargo run -p play --features=simulator -- ... 2> /tmp/play.log
```

`RUST_LOG=debug cargo run ...` for verbose logs.

**Hw:** Play writes to `ALLIUM_BASE_DIR/logs/play.log` (normally `/mnt/SDCARD/.allium/logs/play.log`). To read: eject SD, open the file from Mac. There is no live tail — that requires telnet/ssh on the device.

If you need live tailing later: enable telnet via Allium's `telnet-on.sh` script (`static/.allium/scripts/`). Then `telnet <miyoo-ip>` and `tail -f /mnt/SDCARD/.allium/logs/play.log`.

---

## Rare ops

### Regenerate libretro bindings

When you bump `crates/play/vendor/libretro.h`:

```bash
cd crates/play
bindgen vendor/libretro.h \
    --allowlist-type 'retro_.*' \
    --allowlist-function 'retro_.*' \
    --allowlist-var 'RETRO_.*' \
    --no-layout-tests \
    -o src/libretro_sys.rs
```

Diff `src/libretro_sys.rs` against the previous version. New types/functions are usually safe; removals or signature changes will break compilation — fix call sites then.

### Pull a fresh toolchain image

If `make build` errors mention missing tools:

```bash
docker pull mholdg16/miyoomini-toolchain:latest
docker images | grep miyoomini   # confirm
```

### Clean cargo cache

If incremental cache gets confused:

```bash
cargo clean -p play
```

Full nuke (slow rebuild after):

```bash
cargo clean
```

### Inspect a built binary

```bash
file dist/.allium/bin/play         # confirm ARM ELF
ls -lh dist/.allium/bin/play       # size sanity check (~5-15 MB stripped)
arm-linux-gnueabihf-objdump -p dist/.allium/bin/play | grep NEEDED   # listed dynamic deps
```

### Rebuild only Play (skip the rest)

```bash
make -B build CARGO_BUILD_TARGET_LIST="--bin=play"
```

(If your Makefile factors out the binary list — check the actual var name. If not, edit the Makefile temporarily.)

---

## Troubleshooting

**`make build` errors with "no such image":** OrbStack isn't running or the toolchain image isn't pulled. Run the pre-flight steps.

**Cargo says `no matching package found` for `play`:** workspace `members` doesn't include `crates/play`. Check the root `Cargo.toml`.

**Sim window opens then immediately closes:** check stderr — usually a panic from a missing libretro symbol, bad pixel format conversion, or a winit lifecycle mistake.

**Video is black in sim:** confirm the video callback fires and that the core pixel format matches your converter. Dump one PPM from the same run path to separate core output from window presentation.

**Hw screen stays black:** check `ALLIUM_BASE_DIR/logs/play.log` after eject. Likely a framebuffer open error, wrong core path, or pitch/format mismatch. Remember the Play video path writes framebuffer directly; it should not go through Allium `Display`.

**`--frames N` exits too early/late:** log core fps and measured frame time. Fix pacing before blaming core performance.

**Audio crackling in sim:** ring buffer size too small, or sample rate mismatch between core and cpal config. Log both rates and compare.

**Audio underrun on hw:** ALSA period/buffer too small for the CPU. Increase period_size in increments of 256 and watch underrun logs.

**Fast-forward audio sounds wrong:** v1 fast-forward should mute/drop audio. If you hear stretched garbage, mute path is incomplete.

**Game accepts input in sim but not on hw:** first verify Allium `Platform` / `Key` events work. Only drop to raw evdev if the shared platform layer misses keys or adds measurable latency.

**MENU does nothing:** confirm Play's UDP listener bound the RetroArch socket and `common::retroarch` parses incoming wire commands. Check for another process already bound to the same socket.

**allium-menu sends commands but Play ignores them:** log parsed `RetroArchCommand` values. If parsing code lives outside `common::retroarch`, move it back before adding more commands.
