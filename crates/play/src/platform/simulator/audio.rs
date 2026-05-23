// Desktop host audio output using CPAL (Cross-Platform Audio Library).
// This backend initializes CPAL output stream, supporting floating-point or integer formats.

use crate::audio::AudioConsumer;
use anyhow::{anyhow, Context, Result};
use log::{info, warn};

const CHANNELS: usize = 2;

pub struct SimulatorAudio {
    _stream: cpal::Stream,
}

impl SimulatorAudio {
    // Spawns/builds a CPAL audio stream using default host output device.
    pub fn new(sample_rate: u32, consumer: AudioConsumer) -> Result<Self> {
        use cpal::traits::StreamTrait;
        let (device, config) = get_device_and_config(sample_rate)?;
        let stream = build_stream(&device, &config, consumer)?;
        stream.play().context("Failed to start cpal audio stream")?;
        info!("Simulator audio output stream started successfully");
        Ok(Self { _stream: stream })
    }
}

// Selects cpal config targeting the required sample rate.
fn select_config(device: &cpal::Device, sample_rate: u32) -> Result<cpal::SupportedStreamConfig> {
    use cpal::traits::DeviceTrait;
    let req = cpal::SampleRate(sample_rate);
    device.supported_output_configs()
        .context("Failed to query cpal output configs")?
        .filter(|config| config.channels() == CHANNELS as u16)
        .find_map(|config| config.try_with_sample_rate(req))
        .ok_or_else(|| anyhow!("No cpal stereo output config supports {} Hz", sample_rate))
}

// Queries default hardware device and its streaming configurations.
fn get_device_and_config(sample_rate: u32) -> Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No default audio output device"))?;
    let config = select_config(&device, sample_rate)?;
    Ok((device, config))
}

// Delegates the cpal stream construction depending on target sample format.
fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    consumer: AudioConsumer,
) -> Result<cpal::Stream> {
    let sample_format = config.sample_format();
    let stream_config = config.config();
    let err_fn = |err| warn!("cpal audio stream error: {}", err);
    match sample_format {
        cpal::SampleFormat::F32 => build_f32(device, &stream_config, consumer, err_fn),
        cpal::SampleFormat::I16 => build_i16(device, &stream_config, consumer, err_fn),
        cpal::SampleFormat::U16 => build_u16(device, &stream_config, consumer, err_fn),
        other => Err(anyhow!("Unsupported cpal sample format: {:?}", other)),
    }
}

fn build_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioConsumer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::traits::DeviceTrait;
    device
        .build_output_stream(config, move |data, _| { consumer.fill_f32(data); }, err_fn, None)
        .context("Failed to build f32 cpal output stream")
}

fn build_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioConsumer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::traits::DeviceTrait;
    device
        .build_output_stream(config, move |data, _| { consumer.fill_i16(data); }, err_fn, None)
        .context("Failed to build i16 cpal output stream")
}

fn build_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioConsumer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::traits::DeviceTrait;
    device
        .build_output_stream(config, move |data, _| { consumer.fill_u16(data); }, err_fn, None)
        .context("Failed to build u16 cpal output stream")
}
