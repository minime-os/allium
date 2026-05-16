use anyhow::Result;
use std::path::Path;

pub fn init_hardware_log(log_path: &Path) -> Result<()> {
    #[cfg(any(feature = "miyoo", feature = "rg35xxsp"))]
    {
        use std::fs;
        use std::os::unix::io::AsRawFd;

        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            Ok(log_file) => {
                let fd = log_file.as_raw_fd();
                unsafe {
                    nix::libc::dup2(fd, nix::libc::STDOUT_FILENO);
                    nix::libc::dup2(fd, nix::libc::STDERR_FILENO);
                }
            }
            Err(e) => {
                eprintln!("Failed to open log file {:?}: {}", log_path, e);
            }
        }
    }
    Ok(())
}
