use anyhow::Result;

const MIN_VOLUME: i32 = 0;
const MAX_VOLUME: i32 = 20;

pub fn set_volume(volume: i32) -> Result<()> {
    let percent = volume.clamp(MIN_VOLUME, MAX_VOLUME) * 100 / MAX_VOLUME;
    std::process::Command::new("amixer")
        .args(["sset", "Master", &format!("{percent}%")])
        .spawn()?
        .wait()?;
    Ok(())
}
