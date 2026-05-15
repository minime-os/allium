use anyhow::Result;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use common::constants::ALLIUM_PLAY_LOG;

// On hardware there is no terminal, so stderr must become a file before logging starts.
pub fn init() -> Result<()> {
    #[cfg(feature = "miyoo")]
    {
        use std::fs;
        // Immediate marker to prove execution
        let _ = fs::write("/mnt/SDCARD/.allium/logs/play_started.marker", "started");
        
        // Attempt log redirection but don't crash if it fails
        let _ = common::log::init_hardware_log(&*ALLIUM_PLAY_LOG);
        
        println!("--- Play starting at {} ---", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }

    SimpleLogger::new().with_level(LevelFilter::Info).init()?;

    Ok(())
}
