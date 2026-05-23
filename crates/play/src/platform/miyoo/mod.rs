// Miyoo platform bootstrapper coordinating video, audio, and physical input.

pub mod audio;
pub mod stats;
pub mod video;

use crate::commands::ControlEvent;
use crate::input::JoypadState;
use crate::video::ScaleMode;
use anyhow::Result;
use audio::MiyooAudio;
use common::platform::{DefaultPlatform as CommonPlatform, Platform};
use video::MiyooVideo;

pub struct MiyooPlatform {
    pub video: MiyooVideo,
    _audio: MiyooAudio,
    platform: CommonPlatform,
    stats: stats::MiyooStats,
    signal: Option<tokio::signal::unix::Signal>,
    swap_enabled: bool,
}

impl MiyooPlatform {
    pub fn new(
        core_id: &str,
        source_width: u32,
        source_height: u32,
        aspect_ratio: f32,
        scale: ScaleMode,
        sample_rate: u32,
        audio_consumer: crate::audio::AudioConsumer,
    ) -> Result<Self> {
        let swap_enabled = check_swap_needed(core_id);
        if swap_enabled {
            log::info!("Enabling swap for core {}", core_id);
            run_script(&common::constants::ALLIUM_SCRIPTS_DIR.join("swap-on.sh"));
        }

        log::info!("Stopping audioserver");
        run_script(&common::constants::ALLIUM_SD_ROOT.join(".tmp_update/script/stop_audioserver.sh"));
        block_libpadsp_preload();
        set_governor("performance");

        let video = MiyooVideo::new(source_width, source_height, aspect_ratio, scale)?;
        let platform = CommonPlatform::new()?;
        let _audio = MiyooAudio::new(sample_rate, audio_consumer)?;
        let stats = stats::MiyooStats::new();
        let signal = None;
        Ok(Self {
            video,
            _audio,
            platform,
            stats,
            signal,
            swap_enabled,
        })
    }

    pub fn poll_input(&mut self, joypad: &mut JoypadState) -> Vec<ControlEvent> {
        while let Some(key_event) = self.platform.try_poll() {
            joypad.apply(key_event);
        }
        Vec::new()
    }

    pub fn cpu_usage(&mut self) -> Option<f64> {
        self.stats.cpu_usage()
    }

    pub fn skip_presentation_when_paused(&self) -> bool {
        true
    }

    pub async fn wait_for_shutdown(&mut self) {
        let signal = self.signal.get_or_insert_with(|| {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
        });
        signal.recv().await;
    }
}

impl Drop for MiyooPlatform {
    fn drop(&mut self) {
        log::info!("Starting audioserver");
        run_script(&common::constants::ALLIUM_SD_ROOT.join(".tmp_update/script/start_audioserver.sh"));
        if self.swap_enabled {
            log::info!("Disabling swap");
            run_script(&common::constants::ALLIUM_SCRIPTS_DIR.join("swap-off.sh"));
        }
        set_governor("ondemand");
    }
}

pub fn init_logging() -> Result<()> {
    use std::fs;
    use log::LevelFilter;
    use simple_logger::SimpleLogger;
    use common::constants::ALLIUM_PLAY_LOG;

    let _ = fs::write("/mnt/SDCARD/.allium/logs/play_started.marker", "started");
    let _ = common::log::init_hardware_log(&*ALLIUM_PLAY_LOG);
    println!("--- Play starting at {} ---", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));

    SimpleLogger::new().with_level(LevelFilter::Info).init()?;
    Ok(())
}

fn set_governor(governor: &str) {
    let path = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor";
    if !std::path::Path::new(path).exists() {
        return;
    }
    if let Err(err) = std::fs::write(path, governor) {
        log::warn!("Failed to set CPU governor to {}: {}", governor, err);
    } else {
        log::info!("Successfully set CPU governor to {}", governor);
    }
}

fn check_swap_needed(core_id: &str) -> bool {
    let toml_path = &*common::constants::ALLIUM_CONFIG_CORES;
    let contents = std::fs::read_to_string(toml_path).unwrap_or_default();
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

fn run_script(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    if let Err(err) = std::process::Command::new(path).status() {
        log::warn!("Failed to execute script {}: {}", path.display(), err);
    }
}

fn block_libpadsp_preload() {
    let wpa_pid = find_pid_by_name("wpa_supplicant");
    let udhcpc_pid = find_pid_by_name("udhcpc");
    let has_padsp = wpa_pid.map(check_preload_padsp).unwrap_or(false)
        || udhcpc_pid.map(check_preload_padsp).unwrap_or(false);
    if has_padsp {
        log::info!("libpadsp.so detected in network processes, restarting cleanly");
        restart_network_cleanly();
    }
}

fn find_pid_by_name(name: &str) -> Option<u32> {
    let proc_dir = std::fs::read_dir("/proc").ok()?;
    proc_dir
        .flatten()
        .find_map(|entry| get_pid_if_name_matches(&entry.path(), name))
}

fn get_pid_if_name_matches(path: &std::path::Path, name: &str) -> Option<u32> {
    let file_name = path.file_name()?.to_str()?;
    let pid = file_name.parse::<u32>().ok()?;
    let comm = std::fs::read_to_string(path.join("comm")).ok()?;
    if comm.trim() == name {
        Some(pid)
    } else {
        None
    }
}

fn check_preload_padsp(pid: u32) -> bool {
    let maps_path = format!("/proc/{}/maps", pid);
    let Ok(maps) = std::fs::read_to_string(maps_path) else {
        return false;
    };
    maps.contains("libpadsp.so")
}

fn restart_network_cleanly() {
    let _ = std::process::Command::new("killall").args(["-9", "wpa_supplicant", "udhcpc"]).status();

    let wpa_path = common::constants::ALLIUM_SD_ROOT.join("miyoo/app/wpa_supplicant");
    let mut wpa_cmd = std::process::Command::new(wpa_path);
    wpa_cmd.args(["-B", "-D", "nl80211", "-iwlan0", "-c", "/appconfigs/wpa_supplicant.conf"])
           .env_remove("LD_PRELOAD");
    if let Err(e) = wpa_cmd.status() {
        log::warn!("Failed to restart wpa_supplicant: {}", e);
    }

    let mut udhcpc_cmd = std::process::Command::new("udhcpc");
    udhcpc_cmd.args(["-i", "wlan0", "-s", "/etc/init.d/udhcpc.script"])
              .env_remove("LD_PRELOAD");
    if let Err(e) = udhcpc_cmd.status() {
        log::warn!("Failed to restart udhcpc: {}", e);
    }
}
