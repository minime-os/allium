#[cfg(not(any(feature = "simulator", feature = "miyoo")))]
compile_error!("pick `simulator` or `miyoo` feature");

mod audio;
mod callbacks;
mod config;
mod control;
mod core;
mod diagnostics;
mod input;
mod libretro_sys;
mod paths;
mod platform;
mod save;
mod video;
mod session;
mod unzip;

use anyhow::Result;
use config::Args;
use log::info;
use session::PlaySession;

fn main() -> Result<()> {
    platform::init_logging()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let args = Args::from_env()?;
        let mut session = PlaySession::new(args);

        info!("Initializing PlaySession for core: {}", session.paths.core_id);
        info!("ROM path: {:?}", session.paths.rom);

        unsafe {
            let ptr = &mut session as *mut PlaySession;
            callbacks::set_handler(ptr);
        }

        let result = execute_session(&mut session).await;

        unsafe {
            callbacks::clear_handler();
        }

        result
    })
}

async fn execute_session(session: &mut PlaySession) -> Result<()> {
    session.load_core()?;
    session.load_game()?;

    if session.args.dump_frame.is_some() {
        session.warm_up_and_dump()?;
    } else {
        session.start_main_loop().await?;
    }

    session.unload_game();
    Ok(())
}
