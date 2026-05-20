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
            .name("play-alsa-audio".to_string())
            .spawn(move || {
                if let Err(err) = run_alsa_thread(sample_rate, consumer, thread_running) {
                    warn!("ALSA audio thread stopped: {:#}", err);
                }
            })
            .context("Failed to spawn ALSA audio thread")?;

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
fn run_alsa_thread(
    sample_rate: u32,
    mut consumer: AudioConsumer,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
    use std::sync::atomic::Ordering;

    const PERIOD_FRAMES: usize = 1024;
    const BUFFER_FRAMES: c_ulong = 4096;
    const SND_PCM_STREAM_PLAYBACK: c_int = 0;
    const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;
    const SND_PCM_FORMAT_S16_LE: c_int = 2;

    #[repr(C)]
    struct SndPcm {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct SndPcmHwParams {
        _private: [u8; 0],
    }

    struct AlsaApi {
        _lib: libloading::Library,
        snd_pcm_open: unsafe extern "C" fn(*mut *mut SndPcm, *const c_char, c_int, c_int) -> c_int,
        snd_pcm_close: unsafe extern "C" fn(*mut SndPcm) -> c_int,
        snd_pcm_prepare: unsafe extern "C" fn(*mut SndPcm) -> c_int,
        snd_pcm_recover: unsafe extern "C" fn(*mut SndPcm, c_int, c_int) -> c_int,
        snd_pcm_writei: unsafe extern "C" fn(*mut SndPcm, *const c_void, c_ulong) -> c_long,
        snd_pcm_hw_params_malloc: unsafe extern "C" fn(*mut *mut SndPcmHwParams) -> c_int,
        snd_pcm_hw_params_free: unsafe extern "C" fn(*mut SndPcmHwParams),
        snd_pcm_hw_params_any: unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams) -> c_int,
        snd_pcm_hw_params: unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams) -> c_int,
        snd_pcm_hw_params_set_access:
            unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams, c_int) -> c_int,
        snd_pcm_hw_params_set_format:
            unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams, c_int) -> c_int,
        snd_pcm_hw_params_set_channels:
            unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams, c_uint) -> c_int,
        snd_pcm_hw_params_set_rate_resample:
            unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams, c_uint) -> c_int,
        snd_pcm_hw_params_set_rate_near: unsafe extern "C" fn(
            *mut SndPcm,
            *mut SndPcmHwParams,
            *mut c_uint,
            *mut c_int,
        ) -> c_int,
        snd_pcm_hw_params_set_buffer_size_near:
            unsafe extern "C" fn(*mut SndPcm, *mut SndPcmHwParams, *mut c_ulong) -> c_int,
        snd_pcm_hw_params_set_period_size_near: unsafe extern "C" fn(
            *mut SndPcm,
            *mut SndPcmHwParams,
            *mut c_ulong,
            *mut c_int,
        ) -> c_int,
        snd_strerror: unsafe extern "C" fn(c_int) -> *const c_char,
    }

    unsafe impl Send for AlsaApi {}

    impl AlsaApi {
        unsafe fn load() -> Result<Self> {
            let lib = unsafe { libloading::Library::new("libasound.so.2") }
                .context("Failed to load libasound.so.2")?;
            unsafe {
                Ok(Self {
                    snd_pcm_open: *lib.get(b"snd_pcm_open")?,
                    snd_pcm_close: *lib.get(b"snd_pcm_close")?,
                    snd_pcm_prepare: *lib.get(b"snd_pcm_prepare")?,
                    snd_pcm_recover: *lib.get(b"snd_pcm_recover")?,
                    snd_pcm_writei: *lib.get(b"snd_pcm_writei")?,
                    snd_pcm_hw_params_malloc: *lib.get(b"snd_pcm_hw_params_malloc")?,
                    snd_pcm_hw_params_free: *lib.get(b"snd_pcm_hw_params_free")?,
                    snd_pcm_hw_params_any: *lib.get(b"snd_pcm_hw_params_any")?,
                    snd_pcm_hw_params: *lib.get(b"snd_pcm_hw_params")?,
                    snd_pcm_hw_params_set_access: *lib.get(b"snd_pcm_hw_params_set_access")?,
                    snd_pcm_hw_params_set_format: *lib.get(b"snd_pcm_hw_params_set_format")?,
                    snd_pcm_hw_params_set_channels: *lib.get(b"snd_pcm_hw_params_set_channels")?,
                    snd_pcm_hw_params_set_rate_resample: *lib
                        .get(b"snd_pcm_hw_params_set_rate_resample")?,
                    snd_pcm_hw_params_set_rate_near: *lib
                        .get(b"snd_pcm_hw_params_set_rate_near")?,
                    snd_pcm_hw_params_set_buffer_size_near: *lib
                        .get(b"snd_pcm_hw_params_set_buffer_size_near")?,
                    snd_pcm_hw_params_set_period_size_near: *lib
                        .get(b"snd_pcm_hw_params_set_period_size_near")?,
                    snd_strerror: *lib.get(b"snd_strerror")?,
                    _lib: lib,
                })
            }
        }

        fn check(&self, code: c_int, action: &str) -> Result<()> {
            if code < 0 {
                return Err(anyhow!("{}: {}", action, self.error(code)));
            }
            Ok(())
        }

        fn error(&self, code: c_int) -> String {
            unsafe {
                std::ffi::CStr::from_ptr((self.snd_strerror)(code))
                    .to_string_lossy()
                    .into_owned()
            }
        }
    }

    struct PcmHandle<'a> {
        api: &'a AlsaApi,
        pcm: *mut SndPcm,
    }

    impl Drop for PcmHandle<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.api.snd_pcm_close)(self.pcm);
            }
        }
    }

    struct HwParamsHandle<'a> {
        api: &'a AlsaApi,
        params: *mut SndPcmHwParams,
    }

    impl Drop for HwParamsHandle<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.api.snd_pcm_hw_params_free)(self.params);
            }
        }
    }

    let api = unsafe { AlsaApi::load()? };
    let name = CString::new("hw:0,0")?;
    let mut pcm = std::ptr::null_mut();
    api.check(
        unsafe { (api.snd_pcm_open)(&mut pcm, name.as_ptr(), SND_PCM_STREAM_PLAYBACK, 0) },
        "Failed to open ALSA PCM hw:0,0",
    )?;
    let pcm = PcmHandle { api: &api, pcm };

    let mut params = std::ptr::null_mut();
    api.check(
        unsafe { (api.snd_pcm_hw_params_malloc)(&mut params) },
        "Failed to allocate ALSA hw params",
    )?;
    let params = HwParamsHandle { api: &api, params };

    api.check(
        unsafe { (api.snd_pcm_hw_params_any)(pcm.pcm, params.params) },
        "Failed to initialize ALSA hw params",
    )?;
    api.check(
        unsafe {
            (api.snd_pcm_hw_params_set_access)(
                pcm.pcm,
                params.params,
                SND_PCM_ACCESS_RW_INTERLEAVED,
            )
        },
        "Failed to set ALSA access mode",
    )?;
    api.check(
        unsafe {
            (api.snd_pcm_hw_params_set_format)(pcm.pcm, params.params, SND_PCM_FORMAT_S16_LE)
        },
        "Failed to set ALSA sample format",
    )?;
    api.check(
        unsafe { (api.snd_pcm_hw_params_set_channels)(pcm.pcm, params.params, CHANNELS as c_uint) },
        "Failed to set ALSA channels",
    )?;
    api.check(
        unsafe { (api.snd_pcm_hw_params_set_rate_resample)(pcm.pcm, params.params, 0) },
        "Failed to disable ALSA resampling",
    )?;
    let mut actual_rate = sample_rate as c_uint;
    let mut dir = 0;
    api.check(
        unsafe {
            (api.snd_pcm_hw_params_set_rate_near)(
                pcm.pcm,
                params.params,
                &mut actual_rate,
                &mut dir,
            )
        },
        "Failed to set ALSA sample rate",
    )?;
    if actual_rate != sample_rate {
        return Err(anyhow!(
            "ALSA selected {} Hz for core sample rate {} Hz; refusing to resample",
            actual_rate,
            sample_rate
        ));
    }
    let mut buffer_frames = BUFFER_FRAMES;
    api.check(
        unsafe {
            (api.snd_pcm_hw_params_set_buffer_size_near)(pcm.pcm, params.params, &mut buffer_frames)
        },
        "Failed to set ALSA buffer size",
    )?;
    let mut period_frames = PERIOD_FRAMES as c_ulong;
    api.check(
        unsafe {
            (api.snd_pcm_hw_params_set_period_size_near)(
                pcm.pcm,
                params.params,
                &mut period_frames,
                &mut dir,
            )
        },
        "Failed to set ALSA period size",
    )?;
    api.check(
        unsafe { (api.snd_pcm_hw_params)(pcm.pcm, params.params) },
        "Failed to apply ALSA hw params",
    )?;
    drop(params);
    api.check(
        unsafe { (api.snd_pcm_prepare)(pcm.pcm) },
        "Failed to prepare ALSA PCM",
    )?;

    info!(
        "Starting Miyoo ALSA audio on hw:0,0: sample_rate={}, period_frames={}, buffer_frames={}",
        sample_rate, period_frames, buffer_frames
    );

    let mut buffer = vec![0; PERIOD_FRAMES * CHANNELS];
    while running.load(Ordering::Relaxed) {
        consumer.fill_i16(&mut buffer);
        let mut offset_frames = 0;
        while offset_frames < PERIOD_FRAMES {
            let ptr = unsafe { buffer.as_ptr().add(offset_frames * CHANNELS) as *const c_void };
            let remaining = (PERIOD_FRAMES - offset_frames) as c_ulong;
            let written = unsafe { (api.snd_pcm_writei)(pcm.pcm, ptr, remaining) };
            if written < 0 {
                api.check(
                    unsafe { (api.snd_pcm_recover)(pcm.pcm, written as c_int, 1) },
                    "Failed to recover ALSA write error",
                )?;
                break;
            }
            offset_frames += written as usize;
        }
    }

    Ok(())
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
