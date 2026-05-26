#![deny(clippy::all)]
#![warn(rust_2018_idioms)]

mod alliumd;

use anyhow::Result;
use simple_logger::SimpleLogger;

use crate::alliumd::AlliumD;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "miyoo")]
    {
        use common::constants::ALLIUM_LOG_DIR;
        let _ = common::log::init_hardware_log(&ALLIUM_LOG_DIR.join("alliumd.log"));
        println!(
            "--- alliumd starting at {} ---",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
    }

    SimpleLogger::new().env().init().unwrap();

    #[cfg(feature = "console")]
    {
        log::info!("Starting tokio console at :6669");
        console_subscriber::init();
    }

    let mut app = AlliumD::new().await?;
    app.run_event_loop().await?;
    Ok(())
}
