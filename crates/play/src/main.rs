#[cfg(not(any(feature = "simulator", feature = "miyoo")))]
compile_error!("pick `simulator` or `miyoo` feature");

mod args;
mod audio;
mod callbacks;
mod config;
mod control;
mod core;
mod input;
mod libretro_sys;
mod logs;
mod paths;
mod scale;
mod session;
mod udp;
mod video;

use anyhow::Result;
use args::Args;
use session::PlaySession;

// Tokio lets the main emulation loop react to external events such as low-battery autosave.
fn main() -> Result<()> {
    logs::init()?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let args = Args::from_env()?;
        let mut session = PlaySession::new(args);
        session.run().await?;
        Ok(())
    })
}
