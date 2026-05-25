# Miyoo Mini+ Benchmark Results

**Date:** 2026-05-25  
**Device:** Miyoo Mini+ (Allium custom firmware)  
**Build:** `play` v0.1.0 (release, armv7-unknown-linux-gnueabihf, miyoo feature)  
**Optimizations applied:** RGB565→BGRA8888 LUT, skip intermediate frame copy (HUD off), tokio deadline short-circuit  
**Test method:** 1800 frames per core (~30 s at 60 fps). Actual runtime measured from the frame-loop summary log.  
**Stability threshold:** Actual FPS ≥ 95 % of target FPS.

## Results — Play (optimized)

| Platform       | Core                  | Test ROM                                               | Target FPS | Avg Frame Time | Actual FPS | Stable 60 fps |
|----------------|-----------------------|--------------------------------------------------------|------------|----------------|------------|---------------|
| NES            | fceumm                | Contra (USA).zip                                       | 60.10      | 16.68 ms       | 60.00      | **YES**       |
| SNES (supafaust) | mednafen_supafaust  | ActRaiser (USA) [FastROM hack].zip                     | 60.10      | 17.17 ms       | 58.2       | **NO**        |
| SNES (snes9x2005) | snes9x2005         | ActRaiser (USA) [FastROM hack].zip                     | 59.92      | 16.70 ms       | 59.9       | **YES**       |
| Game Boy       | gambatte              | Batman - The Video Game (World).zip                    | 59.73      | 16.75 ms       | 59.69      | **YES**       |
| Game Boy Color | gambatte              | Pokemon - Crystal Version (USA, Europe) (Rev 1).zip    | 59.73      | 16.76 ms       | 59.67      | **YES**       |
| Game Boy Advance | mgba                | Astro Boy - Omega Factor (USA).zip                     | 59.73      | 16.70 ms       | 59.9       | **YES**       |
| Game Gear      | genesis_plus_gx       | Sonic Chaos (USA, Europe, Brazil) (En).zip             | 59.92      | 16.70 ms       | 59.9       | **YES**       |
| Master System  | picodrive             | Alex Kidd in Miracle World (USA, Europe, Brazil).zip | 60.00      | 16.67 ms       | 59.99      | **YES**       |
| Genesis        | picodrive             | Zero Tolerance (USA).zip                               | 60.00      | 16.70 ms       | 59.9       | **YES**       |

## Play vs RetroArch Comparison

| Platform | Core | Play FPS | RetroArch FPS | Difference |
|----------|------|----------|---------------|------------|
| SNES (supafaust) | mednafen_supafaust | **58.2** | 57.3 | Play +1.6 % faster |
| SNES (snes9x2005) | snes9x2005 | **59.9** | 59.3 | Play +1.0 % faster |
| Genesis | picodrive | 59.9 | — | — |

## Platforms Not Stable at 60 fps

| Platform | Issue | Fix |
|----------|-------|-----|
| SNES (supafaust) | mednafen_supafaust runs at **58.2 fps**. Core emulation is too heavy for 600 MHz Cortex-A7. | Switch default core to `snes9x2005` (59.9 fps, stable). |

## Optimization Impact

| Optimization | SNES (supafaust) | Genesis | Notes |
|-------------|-------------------|---------|-------|
| Baseline | 49.0 fps | 54.5 fps | Before any optimizations |
| + LUT only | 52.1 fps | 60.0 fps | RGB565→BGRA8888 lookup table |
| + skip-copy + tokio | **58.2 fps** | **59.9 fps** | Skip `captured_frame` memcpy when HUD off; tokio deadline fast-path |

Total savings: **3.25 ms/frame** for SNES, **1.64 ms/frame** for Genesis.

## Notes

- **FBNeo** still OOMs without swap. With 128 MB swap it loads but performance is untested.
- All cores except `mednafen_supafaust` run at stable 60 fps on the Miyoo Mini+.
- The remaining SNES gap is in the core itself, not Play's overhead.
