// Minime host audio output using dynamically loaded ALSA (libasound.so.2).
// This avoids compile-time dependencies on alsa-sys and makes cross-compiling on Mac fully functional.

use crate::audio::AudioConsumer;
use anyhow::{Context, Result, anyhow};
use log::{info, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

const CHANNELS: usize = 2;
const PERIOD_FRAMES: usize = 1024;

pub struct MinimeAudio {
    running: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

type SndPcmOpen = unsafe extern "C" fn(
    pcm: *mut *mut std::os::raw::c_void,
    name: *const std::os::raw::c_char,
    stream: std::os::raw::c_int,
    mode: std::os::raw::c_int,
) -> std::os::raw::c_int;

type SndPcmSetParams = unsafe extern "C" fn(
    pcm: *mut std::os::raw::c_void,
    format: std::os::raw::c_int,
    access: std::os::raw::c_int,
    channels: std::os::raw::c_uint,
    rate: std::os::raw::c_uint,
    soft_resample: std::os::raw::c_int,
    latency: std::os::raw::c_uint,
) -> std::os::raw::c_int;

type SndPcmWritei = unsafe extern "C" fn(
    pcm: *mut std::os::raw::c_void,
    buffer: *const std::os::raw::c_void,
    size: usize,
) -> isize;

type SndPcmRecover = unsafe extern "C" fn(
    pcm: *mut std::os::raw::c_void,
    err: std::os::raw::c_int,
    silent: std::os::raw::c_int,
) -> std::os::raw::c_int;

type SndPcmClose = unsafe extern "C" fn(pcm: *mut std::os::raw::c_void) -> std::os::raw::c_int;

impl MinimeAudio {
    pub fn new(sample_rate: u32, consumer: AudioConsumer) -> Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = running.clone();

        let thread = thread::Builder::new()
            .name("play-minime-audio".to_string())
            .spawn(move || {
                if let Err(err) = run_alsa_thread(sample_rate, consumer, thread_running) {
                    warn!("Minime dynamic ALSA audio thread stopped: {:#}", err);
                }
            })
            .context("Failed to spawn Minime dynamic ALSA audio thread")?;

        Ok(Self {
            running,
            thread: Some(thread),
        })
    }
}

impl Drop for MinimeAudio {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_alsa_thread(
    sample_rate: u32,
    mut consumer: AudioConsumer,
    running: Arc<AtomicBool>,
) -> Result<()> {
    // Dynamically load libasound.so.2 or fallback
    let lib = unsafe {
        libloading::Library::new("libasound.so.2")
            .or_else(|_| libloading::Library::new("libasound.so"))
            .context("Failed to load libasound.so")?
    };

    // Get symbols
    let snd_pcm_open: libloading::Symbol<SndPcmOpen> = unsafe { lib.get(b"snd_pcm_open")? };
    let snd_pcm_set_params: libloading::Symbol<SndPcmSetParams> =
        unsafe { lib.get(b"snd_pcm_set_params")? };
    let snd_pcm_writei: libloading::Symbol<SndPcmWritei> = unsafe { lib.get(b"snd_pcm_writei")? };
    let snd_pcm_recover: libloading::Symbol<SndPcmRecover> =
        unsafe { lib.get(b"snd_pcm_recover")? };
    let snd_pcm_close: libloading::Symbol<SndPcmClose> = unsafe { lib.get(b"snd_pcm_close")? };

    let mut pcm: *mut std::os::raw::c_void = std::ptr::null_mut();
    let name = std::ffi::CString::new("default")?;

    // Open in blocking mode (0)
    let ret = unsafe { snd_pcm_open(&mut pcm, name.as_ptr(), 0, 0) };
    if ret < 0 {
        return Err(anyhow!("Failed to open PCM default device, err={}", ret));
    }

    // Set parameters:
    // format = 2 (SND_PCM_FORMAT_S16_LE)
    // access = 3 (SND_PCM_ACCESS_RW_INTERLEAVED)
    // channels = CHANNELS
    // rate = sample_rate
    // soft_resample = 1
    // latency = 100000 (100ms in microseconds)
    let ret = unsafe { snd_pcm_set_params(pcm, 2, 3, CHANNELS as u32, sample_rate, 1, 100000) };
    if ret < 0 {
        unsafe {
            snd_pcm_close(pcm);
        }
        return Err(anyhow!("Failed to set PCM parameters, err={}", ret));
    }

    info!(
        "Dynamic ALSA player initialized successfully: dev=default, sample_rate={} Hz, latency=100ms",
        sample_rate
    );

    let mut buffer = vec![0i16; PERIOD_FRAMES * CHANNELS];
    let mut buffering = true;

    while running.load(Ordering::Relaxed) {
        let occupied = consumer.occupied_len();
        if buffering {
            if occupied >= PERIOD_FRAMES * CHANNELS * 2 {
                buffering = false;
            } else {
                buffer.fill(0);
            }
        }
        if !buffering {
            let stats = consumer.fill_i16(&mut buffer);
            if stats.frames_filled < PERIOD_FRAMES {
                buffering = true;
            }
        }

        unsafe {
            let mut written = snd_pcm_writei(pcm, buffer.as_ptr() as *const _, PERIOD_FRAMES);
            if written < 0 {
                // Recover from underrun or suspend
                let recovered = snd_pcm_recover(pcm, written as std::os::raw::c_int, 1);
                if recovered < 0 {
                    warn!("snd_pcm_recover failed: {}", recovered);
                    break;
                }
                // Try writing again
                written = snd_pcm_writei(pcm, buffer.as_ptr() as *const _, PERIOD_FRAMES);
                if written < 0 {
                    warn!("snd_pcm_writei failed after recovery: {}", written);
                }
            }
        }
    }

    unsafe {
        let _ = snd_pcm_close(pcm);
    }

    Ok(())
}
