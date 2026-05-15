use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow};

pub fn get_brightness() -> Result<u8> {
    let dir = backlight_dir()?;
    let brightness: u32 = read_number(dir.join("brightness"))?;
    let max: u32 = read_number(dir.join("max_brightness"))?;
    Ok(((brightness * 100) / max.max(1)) as u8)
}

pub fn set_brightness(brightness: u8) -> Result<()> {
    let dir = backlight_dir()?;
    let max: u32 = read_number(dir.join("max_brightness"))?;
    let scaled = (u32::from(brightness.max(1)) * max.max(1)) / 100;
    fs::write(dir.join("brightness"), scaled.max(1).to_string())?;
    Ok(())
}

pub fn blank(enabled: bool) -> Result<()> {
    let value = if enabled { "1" } else { "0" };
    fs::write("/sys/class/graphics/fb0/blank", value)?;
    Ok(())
}

fn backlight_dir() -> Result<PathBuf> {
    std::fs::read_dir("/sys/class/backlight")?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .next()
        .ok_or_else(|| anyhow!("No backlight device found"))
}

fn read_number(path: PathBuf) -> Result<u32> {
    Ok(fs::read_to_string(path)?.trim().parse()?)
}
