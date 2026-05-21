// MiyooSystemGuard manages target-specific device environment states.
// It ensures that hardware resources like audio channels and memory swap are cleanly
// acquired during the game emulation session and gracefully restored upon completion.

use anyhow::Result;
use log::{info, warn};
use std::fs;
use std::path::Path;
use std::process::Command;

pub struct MiyooSystemGuard {
    swap_enabled: bool,
}

impl MiyooSystemGuard {
    // Acquire system-wide control of hardware audio and virtual memory.
    pub fn new(core_id: &str) -> Self {
        let swap_enabled = check_swap_needed(core_id);
        if swap_enabled {
            info!("Enabling swap for core {}", core_id);
            let swap_on_path = common::constants::ALLIUM_SCRIPTS_DIR.join("swap-on.sh");
            run_script(&swap_on_path);
        }

        info!("Stopping audioserver");
        let stop_audio_path = common::constants::ALLIUM_SD_ROOT.join(".tmp_update/script/stop_audioserver.sh");
        run_script(&stop_audio_path);

        block_libpadsp_preload();

        Self { swap_enabled }
    }
}

impl Drop for MiyooSystemGuard {
    // Release system locks and return the device to its normal background state.
    fn drop(&mut self) {
        info!("Starting audioserver");
        let start_audio_path = common::constants::ALLIUM_SD_ROOT.join(".tmp_update/script/start_audioserver.sh");
        run_script(&start_audio_path);

        if self.swap_enabled {
            info!("Disabling swap");
            let swap_off_path = common::constants::ALLIUM_SCRIPTS_DIR.join("swap-off.sh");
            run_script(&swap_off_path);
        }
    }
}

// Emulators like PCSX ReARMed require virtual memory to fit large ROM assets/textures.
fn check_swap_needed(core_id: &str) -> bool {
    let toml_path = &*common::constants::ALLIUM_CONFIG_CORES;
    let contents = fs::read_to_string(toml_path).unwrap_or_default();
    let Ok(parsed): Result<toml::Value, _> = toml::from_str(&contents) else {
        return false;
    };
    parsed
        .get("cores")
        .and_then(|c| c.get(core_id))
        .and_then(|c| c.get("swap"))
        .and_then(|s| s.as_bool())
        .unwrap_or(false)
}

// Spawn helper scripts with proper process logging.
fn run_script(path: &Path) {
    if !path.exists() {
        return;
    }
    if let Err(err) = Command::new(path).status() {
        warn!("Failed to execute script {}: {}", path.display(), err);
    }
}

// Background network processes preloaded with libpadsp.so can keep the raw
// audio hardware descriptors open, blocking our FFI thread from getting a lock.
fn block_libpadsp_preload() {
    let wpa_pid = find_pid_by_name("wpa_supplicant");
    let udhcpc_pid = find_pid_by_name("udhcpc");
    let has_padsp = wpa_pid.map(check_preload_padsp).unwrap_or(false)
        || udhcpc_pid.map(check_preload_padsp).unwrap_or(false);
    if has_padsp {
        info!("libpadsp.so detected in network processes, restarting cleanly");
        restart_network_cleanly();
    }
}

// Locate process IDs to examine their active memory map bindings.
fn find_pid_by_name(name: &str) -> Option<u32> {
    let proc_dir = fs::read_dir("/proc").ok()?;
    for entry in proc_dir.flatten() {
        if let Some(pid) = get_pid_if_name_matches(&entry.path(), name) {
            return Some(pid);
        }
    }
    None
}

// Parse the system executable command identifier to identify targeted processes.
fn get_pid_if_name_matches(path: &Path, name: &str) -> Option<u32> {
    let file_name = path.file_name()?.to_str()?;
    let pid = file_name.parse::<u32>().ok()?;
    let comm = fs::read_to_string(path.join("comm")).ok()?;
    if comm.trim() == name {
        Some(pid)
    } else {
        None
    }
}

// Identify processes holding padsp hooks by reading their mapped library tables.
fn check_preload_padsp(pid: u32) -> bool {
    let maps_path = format!("/proc/{}/maps", pid);
    let Ok(maps) = fs::read_to_string(maps_path) else {
        return false;
    };
    maps.contains("libpadsp.so")
}

// Terminate map-polluted processes and restart them with environment hooks cleared.
fn restart_network_cleanly() {
    let _ = Command::new("killall").args(["-9", "wpa_supplicant", "udhcpc"]).status();

    let wpa_path = common::constants::ALLIUM_SD_ROOT.join("miyoo/app/wpa_supplicant");
    let mut wpa_cmd = Command::new(wpa_path);
    wpa_cmd.args(["-B", "-D", "nl80211", "-iwlan0", "-c", "/appconfigs/wpa_supplicant.conf"])
           .env_remove("LD_PRELOAD");
    if let Err(e) = wpa_cmd.status() {
        warn!("Failed to restart wpa_supplicant: {}", e);
    }

    let mut udhcpc_cmd = Command::new("udhcpc");
    udhcpc_cmd.args(["-i", "wlan0", "-s", "/etc/init.d/udhcpc.script"])
              .env_remove("LD_PRELOAD");
    if let Err(e) = udhcpc_cmd.status() {
        warn!("Failed to restart udhcpc: {}", e);
    }
}
