use anyhow::{Context, Result, anyhow};
use log::{info, warn};
use ringbuf::{HeapCons, HeapProd, HeapRb, traits::*};
use std::time::{Duration, Instant};

const CHANNELS: usize = 2;
const QUEUE_MS: usize = 100;
const UNDERRUN_LOG_INTERVAL: Duration = Duration::from_secs(1);

pub struct AudioQueue;

pub struct AudioProducer {
    producer: HeapProd<i16>,
    muted: bool,
    dropped_frames: u64,
}

pub struct AudioConsumer {
    consumer: HeapCons<i16>,
    last_underrun_log: Option<Instant>,
    underrun_frames: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FillStats {
    pub frames_filled: usize,
    pub underrun_frames: usize,
}

impl AudioQueue {
    pub fn new(capacity_frames: usize, sample_rate: f64) -> (AudioProducer, AudioConsumer) {
        let capacity_frames = capacity_frames.max(1);
        let capacity_samples = capacity_frames * CHANNELS;
        let rb = HeapRb::<i16>::new(capacity_samples);
        let (producer, consumer) = rb.split();

        info!(
            "Audio queue initialized: capacity_frames={}, sample_rate={}",
            capacity_frames, sample_rate
        );

        (
            AudioProducer {
                producer,
                muted: false,
                dropped_frames: 0,
            },
            AudioConsumer {
                consumer,
                last_underrun_log: None,
                underrun_frames: 0,
            },
        )
    }

    pub fn for_sample_rate(sample_rate: u32) -> (AudioProducer, AudioConsumer) {
        let capacity_frames = (sample_rate as usize * QUEUE_MS) / 1000;
        Self::new(capacity_frames, sample_rate as f64)
    }
}

impl AudioProducer {
    pub fn push_frame(&mut self, left: i16, right: i16) -> usize {
        self.push_frames(&[left, right], 1)
    }

    pub fn push_frames(&mut self, samples: &[i16], frames: usize) -> usize {
        let available_frames = (samples.len() / CHANNELS).min(frames);
        if self.muted {
            return available_frames;
        }

        let mut queued_frames = 0;
        for frame in samples.chunks_exact(CHANNELS).take(available_frames) {
            if self.producer.try_push(frame[0]).is_err() {
                self.dropped_frames += (available_frames - queued_frames) as u64;
                break;
            }

            if self.producer.try_push(frame[1]).is_err() {
                self.dropped_frames += (available_frames - queued_frames) as u64;
                break;
            }

            queued_frames += 1;
        }

        queued_frames
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}

impl AudioConsumer {
    pub fn fill_i16(&mut self, out: &mut [i16]) -> FillStats {
        let mut samples_filled = 0;
        for sample in out.iter_mut() {
            match self.next_sample() {
                Some(value) => {
                    *sample = value;
                    samples_filled += 1;
                }
                None => {
                    *sample = 0;
                }
            }
        }

        self.finish_fill(samples_filled, out.len())
    }

    #[cfg(feature = "simulator")]
    fn fill_f32(&mut self, out: &mut [f32]) {
        let mut samples_filled = 0;
        for sample in out.iter_mut() {
            match self.next_sample() {
                Some(value) => {
                    *sample = value as f32 / i16::MAX as f32;
                    samples_filled += 1;
                }
                None => {
                    *sample = 0.0;
                }
            }
        }
        self.finish_fill(samples_filled, out.len());
    }

    #[cfg(feature = "simulator")]
    fn fill_u16(&mut self, out: &mut [u16]) {
        let mut samples_filled = 0;
        for sample in out.iter_mut() {
            match self.next_sample() {
                Some(value) => {
                    *sample = (value as i32 + 32768) as u16;
                    samples_filled += 1;
                }
                None => {
                    *sample = 32768;
                }
            }
        }
        self.finish_fill(samples_filled, out.len());
    }

    fn next_sample(&mut self) -> Option<i16> {
        self.consumer.try_pop()
    }

    fn finish_fill(&mut self, samples_filled: usize, samples_requested: usize) -> FillStats {
        let frames_filled = samples_filled / CHANNELS;
        let requested_frames = samples_requested / CHANNELS;
        let underrun_frames = requested_frames.saturating_sub(frames_filled);

        if underrun_frames > 0 {
            self.underrun_frames += underrun_frames as u64;
            self.log_underrun();
        }

        FillStats {
            frames_filled,
            underrun_frames,
        }
    }

    fn log_underrun(&mut self) {
        let now = Instant::now();
        if self
            .last_underrun_log
            .is_none_or(|last| now.duration_since(last) >= UNDERRUN_LOG_INTERVAL)
        {
            warn!(
                "Audio underrun: total_underrun_frames={}",
                self.underrun_frames
            );
            self.last_underrun_log = Some(now);
        }
    }
}

pub fn validate_sample_rate(sample_rate: f64) -> Result<u32> {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Err(anyhow!(
            "Core reported invalid audio sample rate: {}",
            sample_rate
        ));
    }

    let rounded = sample_rate.round();
    if (sample_rate - rounded).abs() > f64::EPSILON {
        return Err(anyhow!(
            "Core reported non-integer audio sample rate: {}",
            sample_rate
        ));
    }

    Ok(rounded as u32)
}

#[cfg(feature = "simulator")]
pub struct SimulatorAudio {
    _stream: cpal::Stream,
}

#[cfg(feature = "simulator")]
impl SimulatorAudio {
    pub fn new(sample_rate: u32, consumer: AudioConsumer) -> Result<Self> {
        use cpal::traits::{HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default audio output device"))?;
        let supported = select_cpal_config(&device, sample_rate)?;
        let sample_format = supported.sample_format();
        let config = supported.config();

        info!(
            "Starting simulator audio: sample_rate={}, channels={}, format={:?}",
            config.sample_rate.0, config.channels, sample_format
        );

        let err_fn = |err| warn!("cpal audio stream error: {}", err);
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_cpal_stream_f32(&device, &config, consumer, err_fn)?,
            cpal::SampleFormat::I16 => build_cpal_stream_i16(&device, &config, consumer, err_fn)?,
            cpal::SampleFormat::U16 => build_cpal_stream_u16(&device, &config, consumer, err_fn)?,
            other => return Err(anyhow!("Unsupported cpal sample format: {:?}", other)),
        };
        stream.play().context("Failed to start cpal audio stream")?;

        Ok(Self { _stream: stream })
    }
}

#[cfg(feature = "simulator")]
fn select_cpal_config(
    device: &cpal::Device,
    sample_rate: u32,
) -> Result<cpal::SupportedStreamConfig> {
    use cpal::traits::DeviceTrait;

    let requested = cpal::SampleRate(sample_rate);
    let supported = device
        .supported_output_configs()
        .context("Failed to query cpal output configs")?
        .filter(|config| config.channels() == CHANNELS as u16)
        .find_map(|config| config.try_with_sample_rate(requested))
        .ok_or_else(|| {
            anyhow!(
                "No cpal stereo output config supports core sample rate {} Hz; refusing to resample",
                sample_rate
            )
        })?;

    Ok(supported)
}

#[cfg(feature = "simulator")]
fn build_cpal_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioConsumer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::traits::DeviceTrait;

    device
        .build_output_stream(
            config,
            move |data: &mut [i16], _| {
                consumer.fill_i16(data);
            },
            err_fn,
            None,
        )
        .context("Failed to build i16 cpal output stream")
}

#[cfg(feature = "simulator")]
fn build_cpal_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioConsumer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::traits::DeviceTrait;

    device
        .build_output_stream(
            config,
            move |data: &mut [f32], _| {
                consumer.fill_f32(data);
            },
            err_fn,
            None,
        )
        .context("Failed to build f32 cpal output stream")
}

#[cfg(feature = "simulator")]
fn build_cpal_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioConsumer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::traits::DeviceTrait;

    device
        .build_output_stream(
            config,
            move |data: &mut [u16], _| {
                consumer.fill_u16(data);
            },
            err_fn,
            None,
        )
        .context("Failed to build u16 cpal output stream")
}

#[cfg(feature = "miyoo")]
pub struct MiyooAudio {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "miyoo")]
impl MiyooAudio {
    pub fn new(sample_rate: u32, consumer: AudioConsumer) -> Result<Self> {
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let thread_running = running.clone();
        let thread = std::thread::Builder::new()
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

#[cfg(feature = "miyoo")]
impl Drop for MiyooAudio {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(feature = "miyoo")]
fn run_miyoo_thread(
    sample_rate: u32,
    mut consumer: AudioConsumer,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    use ::ffi::*;
    use std::sync::atomic::Ordering;

    let mi_rate = {
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
    };
    let mi_rate_val: u32 = mi_rate as u32;

    if mi_rate_val != sample_rate {
        warn!(
            "MI_AO rounding core sample rate {} Hz to {} Hz",
            sample_rate, mi_rate_val
        );
    }

    const AO_DEV: MI_AUDIO_DEV = 0;
    const AO_CHN: MI_AO_CHN = 0;
    const PERIOD_FRAMES: usize = 1024;

    let mut attr = MI_AUDIO_Attr_s {
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
    };

    unsafe {
        let ret = MI_AO_SetPubAttr(AO_DEV, &mut attr);
        if ret != 0 {
            warn!("MI_AO_SetPubAttr returned {}", ret);
        }

        let ret = MI_AO_Enable(AO_DEV);
        if ret != 0 {
            warn!("MI_AO_Enable returned {}", ret);
        }

        let ret = MI_AO_EnableChn(AO_DEV, AO_CHN);
        if ret != 0 {
            warn!("MI_AO_EnableChn returned {}", ret);
        }
    }

    info!(
        "Starting Miyoo MI_AO audio: dev={}, chn={}, sample_rate={} Hz, period_frames={}",
        AO_DEV, AO_CHN, mi_rate_val, PERIOD_FRAMES
    );

    let mut buffer = vec![0i16; PERIOD_FRAMES * CHANNELS];
    let mut seq: MI_U32 = 0;

    while running.load(Ordering::Relaxed) {
        let stats = consumer.fill_i16(&mut buffer);
        if stats.underrun_frames > 0 {
            // continue; MI_AO will play silence for gaps
        }

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
        seq = seq.wrapping_add(1);

        frame.apVirAddr[0] = buffer.as_mut_ptr() as *mut std::os::raw::c_void;

        unsafe {
            let ret = MI_AO_SendFrame(AO_DEV, AO_CHN, &mut frame, -1);
            if ret != 0 {
                warn!("MI_AO_SendFrame returned {}", ret);
            }
        }
    }

    unsafe {
        let _ = MI_AO_DisableChn(AO_DEV, AO_CHN);
        let _ = MI_AO_Disable(AO_DEV);
    }

    Ok(())
}

#[allow(dead_code)]
pub enum AudioDriver {
    #[cfg(feature = "simulator")]
    Simulator(SimulatorAudio),
    #[cfg(feature = "miyoo")]
    Miyoo(MiyooAudio),
}

impl AudioDriver {
    pub fn new(sample_rate: u32, consumer: AudioConsumer) -> Result<Self> {
        #[cfg(feature = "simulator")]
        {
            Ok(Self::Simulator(SimulatorAudio::new(sample_rate, consumer)?))
        }
        #[cfg(feature = "miyoo")]
        {
            Ok(Self::Miyoo(MiyooAudio::new(sample_rate, consumer)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_preserves_interleaved_stereo_order() {
        let (mut producer, mut consumer) = AudioQueue::new(4, 60.0);

        assert_eq!(producer.push_frames(&[1, 2, 3, 4], 2), 2);

        let mut out = [0; 4];
        let stats = consumer.fill_i16(&mut out);

        assert_eq!(out, [1, 2, 3, 4]);
        assert_eq!(stats.frames_filled, 2);
        assert_eq!(stats.underrun_frames, 0);
    }

    #[test]
    fn queue_counts_only_complete_frames_when_full() {
        let (mut producer, mut consumer) = AudioQueue::new(1, 60.0);

        assert_eq!(producer.push_frames(&[1, 2, 3, 4], 2), 1);
        assert_eq!(producer.dropped_frames(), 1);

        let mut out = [0; 4];
        let stats = consumer.fill_i16(&mut out);

        assert_eq!(out, [1, 2, 0, 0]);
        assert_eq!(stats.frames_filled, 1);
        assert_eq!(stats.underrun_frames, 1);
    }

    #[test]
    fn queue_outputs_silence_on_underrun() {
        let (_producer, mut consumer) = AudioQueue::new(2, 60.0);
        let mut out = [7; 4];

        let stats = consumer.fill_i16(&mut out);

        assert_eq!(out, [0, 0, 0, 0]);
        assert_eq!(stats.frames_filled, 0);
        assert_eq!(stats.underrun_frames, 2);
    }

    #[test]
    fn muted_producer_accepts_and_drops_audio() {
        let (mut producer, mut consumer) = AudioQueue::new(2, 60.0);

        producer.set_muted(true);

        assert_eq!(producer.push_frames(&[1, 2, 3, 4], 2), 2);

        let mut out = [7; 4];
        let stats = consumer.fill_i16(&mut out);

        assert_eq!(out, [0, 0, 0, 0]);
        assert_eq!(stats.frames_filled, 0);
        assert_eq!(stats.underrun_frames, 2);
    }
}
