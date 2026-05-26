# Settings menu plan for Play

This document walks through implementing a Minarch-parity settings menu accessible from the Allium in-game menu. It is scoped after Play v1 (basic launch, video, audio, input, save/load, UDP commands) has landed.

Target: Miyoo Mini Plus.

---

## Assumptions already made

- `allium-menu` receives a new `PlayMenu` view in `play_menu.rs`. Changes are isolated so upstream `allium-menu` changes remain easy to merge.
- Play is detected by a state file (`.allium/state/play.json`), not just the legacy marker file. `allium-menu` falls back to `MENU_TOGGLE` (RetroArch path) when Play is not running.
- Settings changes are communicated from `allium-menu` to Play via **new UDP commands**.
- Settings are persisted in **TOML files** under `config/play/`.
- All 8 Minarch frontend settings are supported.
- Controls and shortcuts support **per-core button filtering** via `retro_input_descriptor` and **MENU+button modifiers**.
- Core options support **both** legacy `retro_variable` and modern `retro_core_option_definition`.
- Screen effects (`Grid`, `Line`) are implemented **procedurally** (ported from MinUI `scaler.c` RGB565 blending formulas) — no PNG assets in the hot path. Effect assets (bezels, overlays) will live in `crates/play/assets/` for future features.
- The menu follows Minarch's "Save for console / Save for game / Restore defaults" flow.

---

## Stage S1: Settings UDP plumbing

**Goal.** The Play UDP server recognises a new class of settings commands and dispatches them into the session without breaking existing save/load/pause/quit commands.

**What you'll learn.** How to extend the UDP command protocol beyond the RetroArch-compatible subset; how to route new commands through `PlaySession` without locking audio/video callbacks.

**Prerequisites.** Play v1 UDP server (Stage 8B in PLAN.md) is functional.

**Targets.** Both (sim for unit tests, hw for end-to-end). Sim tests can exercise UDP dispatch in-process.

### Steps

1. Define new settings commands. At minimum:
   - `SET_SCALE <mode>`
   - `SET_EFFECT <none|grid|line>`
   - `SET_SHARPNESS <sharp|crisp|soft>`
   - `SET_TEARING <off|lenient|strict>`
   - `SET_OVERCLOCK <powersave|normal|performance>`
   - `SET_THREAD_VIDEO <bool>`
   - `SET_DEBUG_HUD <bool>`
   - `SET_MAX_FF <0..7>`
   - `SET_CORE_OPTION <key> <value>`
   - `RELOAD_CONFIG`
2. Extend `ControlEvent` in `commands.rs` with variants for each setting. Keep the enum small — settings that share the same update pattern can share a variant (e.g. `ApplyFrontendSetting { key, value }`) where `key` is a string or small enum.
3. Parse the new wire format in the UDP server. Unknown commands must log a warning, not crash.
4. In `ActiveSession::apply_control_event`, add a handler arm that forwards settings to a new `FrontendSettings` component.
5. Apply settings immediately where safe (e.g. toggling HUD, changing scale mode). Defer settings that require core re-init or scaling rebuild until the next frame boundary.

### Smoke checks

- Sim: UDP client sends `SET_SCALE native`; Play logs the change and updates `session.scale_mode`.
- Sim: Unknown command `"FOO"` logs a warning; the server stays alive.
- Hw: Menu sends `SET_EFFECT grid`; Play applies the effect on the next frame.

### Done when

- [x] New UDP commands parse correctly and are rejected gracefully on malformed input.
- [x] Settings flow through `apply_control_event` without touching audio/video hot paths.
- [x] Existing save/load/pause/quit commands remain untouched.

### Commit message suggestion

`feat(play): add settings UDP commands`

---

## Stage S2: Config persistence and hierarchy

**Goal.** Play loads TOML config at startup, applies overrides per-core and per-game, and persists changes to disk when requested via UDP or menu.

**What you'll learn.** TOML serde patterns, config overlay hierarchy, atomic file writes on VFAT (write-then-rename), and how to keep the config surface small enough to reason about.

**Prerequisites.** Stage S1 done. `toml` and `serde` are already in the workspace.

**Targets.** Both. Sim unit tests load and merge TOML strings in memory. Hw tests verify files appear on the SD card.

### Steps

1. Define config structs:
   - `FrontendConfig` — the 8 settings fields, strongly typed enums.
   - `ControlConfig` — mapping from physical button names to libretro joypad IDs.
   - `ShortcutConfig` — mapping from physical button names to frontend actions.
   - `CoreOptionsConfig` — `HashMap<String, String>` of key-value pairs.
   - `PlayConfig` — container holding optional global overrides plus per-core and per-game overlays.
2. Implement loader that walks the hierarchy:
   - Start with built-in defaults.
   - Merge `config/play/frontend.toml` if present.
   - Merge `config/play/<core_id>/frontend.toml` if present.
   - Merge `config/play/<core_id>/<game_name>.toml` if present ( Frontend section only).
3. Separate the load path for `controls.toml` and `shortcuts.toml` with the same hierarchy.
4. Implement saver that writes back to the appropriate file given a target scope ("console" or "game").
5. On `RELOAD_CONFIG`, re-run the loader and diff-apply changed values.
6. Add unit tests for merging and precedence.

### Smoke checks

- Sim: Test loads `frontend.toml`, core override, and game override; the game override wins.
- Sim: Test saves config and re-loads; values round-trip.
- Hw: Delete all config files; Play starts with built-in defaults.
- Hw: Save for game; file appears at `config/play/<core_id>/<game_name>.toml`.

### Done when

- [x] Config loads from hierarchy with correct precedence.
- [x] Config saves atomically (no half-written files on crash).
- [x] Unit tests cover merge, precedence, and round-trip.
- [x] No config path is hardcoded to SD card root; uses `ALLIUM_BASE_DIR`.

### Commit message suggestion

`feat(play): add TOML config persistence with per-system and per-game overrides`

---

## Stage S3: Frontend settings live in Play

**Goal.** All 8 Minarch frontend settings are stored, applied, and re-applied from config at startup.

**What you'll learn.** Where each setting hooks into the existing Play runtime, and which settings affect the core callback contract (e.g. overclock changes CPU governor).

**Prerequisites.** Stage S2 done.

**Targets.** Both.

### Steps

1. Populate `ActiveSession` with a `FrontendSettings` struct holding the 8 fields.
2. At session startup, load the config hierarchy and apply values to the struct.
3. Wire the UDP handlers from S1 to mutate `FrontendSettings` and trigger side effects:
   - `ScaleMode` → call `apply_scale()` on the platform video backend.
   - `Effect` → set flag consumed by the video present path.
   - `Sharpness` → set flag consumed by the scaler selection.
   - `PreventTearing` → set flag consumed by the frame timing sleep path.
   - `Overclock` → write to `/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor` or call the governor utility.
   - `ThreadVideo` → set flag (deferred: actual threading is Bonus D).
   - `DebugHUD` → toggle `HudState::enabled`.
   - `MaxFFSpeed` → configure fast-forward cap.
4. When settings change, invalidate caches that depend on them (e.g. scaler selection).

### Smoke checks

- Hw: Launch game; send `SET_SCALE native` via UDP; scaling changes on next frame.
- Hw: Send `SET_OVERCLOCK performance`; governor switches.
- Sim: Cycle all 8 settings via test harness and verify struct values.

### Done when

- [x] All 8 frontend settings are loadable from TOML and mutable at runtime via UDP.
- [x] Settings change is visible within one frame.
- [x] No settings require a full session restart to apply.

### Commit message suggestion

`feat(play): wire frontend settings into session and video path`

---

## Stage S4: Controls and shortcuts mapping

**Goal.** Full parity with Minarch control mapping: per-core button filtering from `retro_input_descriptor`, MENU+button modifier support, and physical button → action binding.

**What you'll learn.** How `retro_input_descriptor` communicates which buttons a core cares about; how to store button bindings in TOML; how to handle MENU+button combos without stealing the MENU button from the frontend.

**Prerequisites.** Stage S2/S3 done.

**Targets.** Both.

### Steps

1. In the environment callback, accept `RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS` and store the descriptor list.
2. Build a filtered button list: only show buttons that the core declares as present (matching Minarch's `Input_init()` logic).
3. Define a `ButtonMapping` struct: physical button, optional `requires_menu` modifier, target action (for shortcuts) or libretro ID (for controls).
4. Extend `JoypadState` to accept configurable mappings instead of the current hardcoded table.
5. Implement the "awaiting input" mini-flow: when the user selects "Bind X", the menu enters a listening state where the next physical button press becomes the binding.
6. Store bindings in `controls.toml` and `shortcuts.toml` (separate from frontend settings).

### Smoke checks

- Sim: Mock `retro_input_descriptor` with only A and B; Controls menu shows only A and B.
- Sim: Bind A to `RETRO_DEVICE_ID_JOYPAD_L`; subsequent `input_state` calls return the mapped value.
- Hw: Bind MENU+A to Toggle Fast Forward; verify MENU alone still opens the menu.

### Done when

- [x] Controls menu filters buttons per-core descriptor.
- [x] Shortcuts menu supports single-button and MENU+button modifiers.
- [x] Bindings persist to TOML and restore on next launch.

### Commit message suggestion

`feat(play): add per-core controls and configurable shortcuts`

---

## Stage S5: Core options tab

**Goal.** The settings menu exposes the running core's options, supporting both legacy `retro_variable` and modern `retro_core_option_definition` APIs.

**What you'll learn.** The two libretro option APIs and how to normalise them into a single internal representation.

**Prerequisites.** Stage S2/S3 done.

**Targets.** Both.

### Steps

1. Accept `RETRO_ENVIRONMENT_SET_CORE_OPTIONS` in the environment callback. Parse the `retro_core_option_definition` array into an internal `CoreOptionDefinition` list.
2. Accept `RETRO_ENVIRONMENT_SET_VARIABLES` (legacy API). Parse the `retro_variable` array and normalise it to the same `CoreOptionDefinition` shape.
3. Store current values in `CoreOptionsConfig` (HashMap) and apply defaults from the definitions on first load.
4. When a core option changes value, call `retro_set_environment` with `RETRO_ENVIRONMENT_SET_VARIABLE` to notify the core.
5. Wire the Core tab in `PlayMenu` to enumerate definitions and send `SET_CORE_OPTION` UDP commands.

### Smoke checks

- Sim: Core sends `SET_CORE_OPTIONS` with 3 options; Play stores them and responds `true`.
- Sim: Core sends legacy `SET_VARIABLES` with `|`-separated values; Play parses into the same shape.
- Hw: Change an option in the Core tab; observe the core react (e.g. palette change in Gambatte).

### Done when

- [x] Both option APIs are handled.
- [x] Options are stored and applied to the core via `RETRO_ENVIRONMENT_SET_VARIABLE`.
- [x] Options persist per-core and per-game.

### Commit message suggestion

`feat(play): expose core options via legacy and modern libretro APIs`

---

## Stage S6: PlayMenu in allium-menu

**Goal.** Pressing "Settings" in the in-game menu opens a `PlayMenu` with four tabs (Frontend, Controls, Shortcuts, Core) and save/restore actions, communicating with Play via UDP.

**What you'll learn.** How `allium-menu` views are structured; how to keep `PlayMenu` changes minimal and isolated from upstream allium-menu logic.

**Prerequisites.** Stages S1–S5 done on the Play side.

**Targets.** Both (sim tests the view logic; hw tests UDP round-trip).

### Steps

1. Detect Play: check if `.allium/state/play.json` exists. If not, fall back to the existing RetroArch `MENU_TOGGLE` path.
2. Create `play_menu.rs` containing a `PlayMenu` struct and `PlayMenuState` (serialisable for view persistence).
3. Implement four tabs as sub-views:
   - **Frontend**: scrollable list of 8 settings with left/right value cycling.
   - **Controls**: scrollable list of core-relevant buttons with A-to-bind, X-to-clear.
   - **Shortcuts**: same binding UI for frontend actions.
   - **Core**: scrollable list of core option key-value pairs.
4. Add a bottom action bar: "Save for console" (`PAUSE_TOGGLE` + write global TOML), "Save for game" (write per-game TOML), "Restore defaults" (delete override + `RELOAD_CONFIG`).
5. Send UDP commands on every value change for immediate feedback.
6. Style the menu to match existing Allium menu aesthetics (pill buttons, button hints, status bar).

### Smoke checks

- Sim: `PlayMenu::new()` renders four tab labels and the bottom actions.
- Sim: Changing a Frontend value sends the correct UDP string to a mock socket.
- Hw: Navigate Settings → Frontend → change Effect → see effect change live.
- Hw: Controls tab shows only buttons the core descriptor reports.

### Done when

- [x] `PlayMenu` exists as a separate file with minimal changes to upstream views.
- [x] All four tabs enumerate their items correctly.
- [x] UDP commands are sent on every interactive change.
- [x] Save/ Restore flow writes and reads TOML files correctly.

### Commit message suggestion

`feat(allium-menu): add PlayMenu with settings, controls, shortcuts, and core options`

---

## Stage S7: Procedural screen effects

**Goal.** Port MinUI's procedural `Grid` and `Line` effects from `scaler.c` into Play's video present path. Effects only apply at integer/native scaling.

**What you'll learn.** MinUI's `Weight3_1` and `Weight2_3` RGB565 blending formulas; how to insert a lightweight post-scale pass without allocating per-frame.

**Prerequisites.** Stage S3 (Effect setting exists and toggles a flag).

**Targets.** Both.

### Steps

1. Port the RGB565 blending macros:
   - `Weight3_1(a, b)` = blend 75% `a` + 25% `b`
   - `Weight2_3(a, b)` = blend 40% `a` + 60% `b`
2. Port the effect scaler functions:
   - `scaleNx_grid` (2x, 3x)
   - `scaleNx_line` (2x, 3x, 4x)
3. In the video present path, after scaling but before writing to the framebuffer, check:
   - Is `Effect` != `None`?
   - Is `ScaleMode` == `Native` (integer scaling)?
   If yes, dispatch to the appropriate effect scaler.
4. Precompute the "black" blend target as `0x0000` (RGB565 black). The GB lightest-palette-color feature (MinUI commit 5051ace) is deferred.
5. Add simulator tests: render a known source pattern with Grid and Line, assert specific output pixels.

### Smoke checks

- Sim: Native 2× scale with `Effect::Grid` produces expected blended pixels in a test buffer.
- Hw: Launch GB at native scale; toggle Grid; observe LCD-like grid overlay.
- Hw: Toggle Line at native scale; observe horizontal scanlines.
- Hw: Switch to Aspect scaling; effect is silently skipped (no artifacts).

### Done when

- [x] Grid and Line effects produce correct output at integer scales.
- [x] Effects are skipped safely at non-integer scales.
- [x] No per-frame heap allocation in the effect path.

### Commit message suggestion

`feat(play): add procedural grid and scanline effects`

---

## Done — Settings Menu

At this point Play has Minarch-parity settings accessible from the Allium in-game menu: Frontend, Controls, Shortcuts, Core, and procedural screen effects.

Deferred features (not in scope for this plan):
- GB lightest-palette grid color (requires core patch + `/tmp` IPC).
- Screen bezels and overlay assets.
- Threaded video (Bonus D in PLAN.md).
- Fast-forward audio resampling (Bonus A in PLAN.md).
