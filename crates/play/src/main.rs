#[cfg(not(any(feature = "simulator", feature = "miyoo")))]
compile_error!("pick `simulator` or `miyoo` feature");

mod audio;
mod callbacks;
mod config;
mod control;
mod core;
mod input;
mod libretro_sys;
mod paths;
mod platform;
mod save;
mod hud;
mod scale;
mod session;
mod timing;
mod udp;
mod unzip;
mod video;


use anyhow::Result;
use config::Args;
use session::PlaySession;

// Tokio lets the main emulation loop react to external events such as low-battery autosave.
fn main() -> Result<()> {
    platform::init_logging()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let args = Args::from_env()?;
        let mut session = PlaySession::new(args);
        session.run().await?;
        Ok(())
    })
}
