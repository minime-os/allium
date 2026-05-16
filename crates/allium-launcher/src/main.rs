#![deny(clippy::all)]
#![warn(rust_2018_idioms)]

mod allium_launcher;
mod consoles;
mod entry;
mod ota;
mod view;

use anyhow::Result;

use allium_launcher::AlliumLauncher;
use common::platform::{DefaultPlatform, Platform};
use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
    {
        use common::constants::ALLIUM_LOG_DIR;
        let _ = common::log::init_hardware_log(&ALLIUM_LOG_DIR.join("allium-launcher.log"));
        println!("--- allium-launcher starting at {} ---", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }

    SimpleLogger::new().env().init().unwrap();

    #[cfg(feature = "miyoo")]
    common::platform::miyoo::try_fix_resolution().await?;

    let platform = DefaultPlatform::new()?;
    let mut app = AlliumLauncher::new(platform)?;
    app.run_event_loop().await?;
    Ok(())
}
