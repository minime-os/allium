// This module manages loading and saving persistent state for emulator games.
// It handles emulated SRAM (battery-backed memory) and save state slots.
// All operations ensure safe serialization/deserialization against the local filesystem.

use std::fs;
use std::ptr;
use crate::core::Core;
use crate::paths::PlayPaths;
use crate::libretro_sys::RETRO_MEMORY_SAVE_RAM;
use anyhow::{Result, anyhow};
use log::{info, warn};

pub fn load_sram(core: &Core, paths: &PlayPaths) -> Result<()> {
    let Some((data, size)) = core.memory_region(RETRO_MEMORY_SAVE_RAM) else {
        return Ok(());
    };
    let path = paths.sram_path();
    if !path.exists() {
        return Ok(());
    }

    let sram = fs::read(&path)?;
    if sram.len() != size {
        warn!(
            "SRAM size mismatch for {:?}: file={}, core={}",
            path,
            sram.len(),
            size
        );
    }
    let copy_len = sram.len().min(size);
    unsafe {
        ptr::copy_nonoverlapping(sram.as_ptr(), data, copy_len);
    }
    info!("Loaded SRAM from {:?}", path);
    Ok(())
}

pub fn save_sram(core: &Core, paths: &PlayPaths) -> Result<()> {
    let Some((data, size)) = core.memory_region(RETRO_MEMORY_SAVE_RAM) else {
        return Ok(());
    };
    fs::create_dir_all(&paths.save_dir)?;
    let path = paths.sram_path();
    let sram = unsafe { std::slice::from_raw_parts(data as *const u8, size) };
    fs::write(&path, sram)?;
    info!("Saved SRAM to {:?}", path);
    Ok(())
}

pub fn save_state_slot(core: &Core, paths: &PlayPaths, slot: i8) -> Result<()> {
    let size = core.serialize_size();
    if size == 0 {
        return Err(anyhow!("Core does not support save states"));
    }

    let mut data = vec![0; size];
    if !core.serialize(&mut data) {
        return Err(anyhow!("Core failed to save state"));
    }

    fs::create_dir_all(&paths.state_dir)?;
    let path = paths.state_path(slot)?;
    fs::write(&path, data)?;
    info!("Saved state slot {} to {:?}", slot, path);
    Ok(())
}

pub fn load_state_slot(core: &Core, paths: &PlayPaths, slot: i8) -> Result<()> {
    let path = paths.state_path(slot)?;
    let data = fs::read(&path)?;
    if !core.unserialize(&data) {
        return Err(anyhow!("Core failed to load state"));
    }

    info!("Loaded state slot {} from {:?}", slot, path);
    Ok(())
}
