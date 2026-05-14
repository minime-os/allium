#[cfg(not(any(feature = "simulator", feature = "miyoo")))]
compile_error!("pick `simulator` or `miyoo` feature");

mod args;
mod audio;
mod callbacks;
mod config;
mod control;
mod core;
mod frame;
mod input;
mod libretro_sys;
mod logs;
#[cfg(feature = "miyoo")]
mod miyoo_video;
mod paths;
mod scale;
mod session;
#[cfg(feature = "simulator")]
mod simulator_video;
mod udp;

use anyhow::Result;
use args::Args;
use session::PlaySession;

// Tokio lets the main emulation loop react to external events such as low-battery autosave.
#[tokio::main]
async fn main() -> Result<()> {
    logs::init()?;

    let args = Args::from_env()?;
    let mut session = PlaySession::new(args);
    session.run().await?;

    Ok(())
}
