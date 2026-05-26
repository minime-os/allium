// Miyoo-specific audio player utilizing MI_AO kernel APIs.
// This module spawns a dedicated audio output thread feeding raw sound samples.

use crate::audio::AudioConsumer;
use anyhow::{Context, Result};
use log::{info, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

const CHANNELS: usize = 2;
const AO_DEV: ::ffi::MI_AUDIO_DEV = 0;
const AO_CHN: ::ffi::MI_AO_CHN = 0;
const PERIOD_FRAMES: usize = 1024;

pub struct MiyooAudio {
    running: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MiyooAudio {
    pub fn new(sample_rate: u32, consumer: AudioConsumer) -> Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = running.clone();
        let thread = thread::Builder::new()
            .name("play-miyoo-audio".to_string())
            .spawn(move || {
                if let Err(err) = run_miyoo_thread(sample_rate, consumer, thread_running) {
                    warn!("Miyoo MI_AO audio thread stopped: {:#}", err);
                }
            })
            .context("Failed to spawn Miyoo MI_AO audio thread")?;

        Ok(Self {
            running,
            thread: Some(thread),
        })
    }
}

impl Drop for MiyooAudio {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn select_miyoo_sample_rate(sample_rate: u32) -> ::ffi::MI_AUDIO_SampleRate_e {
    use ::ffi::*;
    match sample_rate {
        r if r <= 8500 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_8000,
        r if r <= 11500 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_11025,
        r if r <= 14000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_12000,
        r if r <= 19000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_16000,
        r if r <= 23000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_22050,
        r if r <= 28000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_24000,
        r if r <= 38000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_32000,
        r if r <= 46000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_44100,
        r if r <= 72000 => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_48000,
        _ => MI_AUDIO_SampleRate_e_E_MI_AUDIO_SAMPLE_RATE_96000,
    }
}

fn init_miyoo_audio_attr(mi_rate: ::ffi::MI_AUDIO_SampleRate_e) -> ::ffi::MI_AUDIO_Attr_s {
    use ::ffi::*;
    MI_AUDIO_Attr_s {
        eSamplerate: mi_rate,
        eBitwidth: MI_AUDIO_BitWidth_e_E_MI_AUDIO_BIT_WIDTH_16,
        eWorkmode: MI_AUDIO_Mode_e_E_MI_AUDIO_MODE_I2S_MASTER,
        eSoundmode: MI_AUDIO_SoundMode_e_E_MI_AUDIO_SOUND_MODE_STEREO,
        u32FrmNum: 6,
        u32PtNumPerFrm: PERIOD_FRAMES as MI_U32,
        u32CodecChnCnt: CHANNELS as MI_U32,
        u32ChnCnt: CHANNELS as MI_U32,
        WorkModeSetting: MI_AUDIO_Attr_s__bindgen_ty_1 {
            stI2sConfig: MI_AUDIO_I2sConfig_t {
                eFmt: MI_AUDIO_I2sFmt_e_E_MI_AUDIO_I2S_FMT_I2S_MSB,
                eMclk: MI_AUDIO_I2sMclk_e_E_MI_AUDIO_I2S_MCLK_0,
                bSyncClock: 0,
            },
        },
    }
}

fn enable_miyoo_audio(attr: &mut ::ffi::MI_AUDIO_Attr_s) {
    use ::ffi::*;
    unsafe {
        let ret = MI_AO_SetPubAttr(AO_DEV, attr);
        if ret != 0 {
            warn!("MI_AO_SetPubAttr returned {ret}");
        }
        let ret = MI_AO_Enable(AO_DEV);
        if ret != 0 {
            warn!("MI_AO_Enable returned {ret}");
        }
        let ret = MI_AO_EnableChn(AO_DEV, AO_CHN);
        if ret != 0 {
            warn!("MI_AO_EnableChn returned {ret}");
        }
    }
}

fn disable_miyoo_audio() {
    use ::ffi::*;
    unsafe {
        let _ = MI_AO_DisableChn(AO_DEV, AO_CHN);
        let _ = MI_AO_Disable(AO_DEV);
    }
}

fn init_miyoo_audio(sample_rate: u32) -> u32 {
    let mi_rate = select_miyoo_sample_rate(sample_rate);
    let mi_rate_val = mi_rate as u32;
    if mi_rate_val != sample_rate {
        warn!("MI_AO rounding core sample rate {sample_rate} Hz to {mi_rate_val} Hz");
    }
    let mut attr = init_miyoo_audio_attr(mi_rate);
    enable_miyoo_audio(&mut attr);
    mi_rate_val
}

fn process_miyoo_audio_period(
    consumer: &mut AudioConsumer,
    buffer: &mut [i16],
    buffering: &mut bool,
) {
    let occupied = consumer.occupied_len();
    if *buffering {
        if occupied >= PERIOD_FRAMES * CHANNELS * 2 {
            *buffering = false;
        } else {
            buffer.fill(0);
        }
    }
    if !*buffering {
        let stats = consumer.fill_i16(buffer);
        if stats.frames_filled < PERIOD_FRAMES {
            *buffering = true;
        }
    }
}

fn send_miyoo_frame(buffer: &mut [i16], seq: u32) {
    use ::ffi::*;
    let mut frame = MI_AUDIO_Frame_s {
        eBitwidth: MI_AUDIO_BitWidth_e_E_MI_AUDIO_BIT_WIDTH_16,
        eSoundmode: MI_AUDIO_SoundMode_e_E_MI_AUDIO_SOUND_MODE_STEREO,
        apVirAddr: [std::ptr::null_mut(); 16],
        u64TimeStamp: 0,
        u32Seq: seq,
        u32Len: (PERIOD_FRAMES * CHANNELS * std::mem::size_of::<i16>()) as MI_U32,
        au32PoolId: [0; 2],
        apSrcPcmVirAddr: [std::ptr::null_mut(); 16],
        u32SrcPcmLen: 0,
    };
    frame.apVirAddr[0] = buffer.as_mut_ptr() as *mut std::os::raw::c_void;
    unsafe {
        if MI_AO_SendFrame(AO_DEV, AO_CHN, &mut frame, -1) != 0 {
            warn!("MI_AO_SendFrame failed");
        }
    }
}

fn run_miyoo_thread(
    sample_rate: u32,
    mut consumer: AudioConsumer,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let mi_rate = init_miyoo_audio(sample_rate);
    info!(
        "Starting Miyoo MI_AO audio: dev={AO_DEV}, chn={AO_CHN}, sample_rate={mi_rate} Hz, period_frames={PERIOD_FRAMES}"
    );
    let (mut buffer, mut seq, mut buffering) = (vec![0i16; PERIOD_FRAMES * CHANNELS], 0u32, true);
    while running.load(Ordering::Relaxed) {
        process_miyoo_audio_period(&mut consumer, &mut buffer, &mut buffering);
        send_miyoo_frame(&mut buffer, seq);
        seq = seq.wrapping_add(1);
    }
    disable_miyoo_audio();
    Ok(())
}
