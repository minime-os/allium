use anyhow::Result;
use log::LevelFilter;
use simple_logger::SimpleLogger;

// On hardware there is no terminal, so stderr must become a file before logging starts.
pub fn init() -> Result<()> {
    #[cfg(feature = "miyoo")]
    init_miyoo()?;

    SimpleLogger::new().with_level(LevelFilter::Info).init()?;

    Ok(())
}

#[cfg(feature = "miyoo")]
// dup2 redirects simple_logger output without teaching the logger about files.
fn init_miyoo() -> Result<()> {
    use common::constants::ALLIUM_PLAY_LOG;
    use std::fs;
    use std::os::unix::io::AsRawFd;

    if let Some(parent) = ALLIUM_PLAY_LOG.parent() {
        fs::create_dir_all(parent)?;
    }

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&*ALLIUM_PLAY_LOG)?;

    let fd = log_file.as_raw_fd();
    unsafe {
        nix::libc::dup2(fd, nix::libc::STDERR_FILENO);
    }

    Ok(())
}
