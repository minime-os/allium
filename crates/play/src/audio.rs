// Pure generic memory-based ring buffer audio queue.
// This module provides the thread-safe i16 sample pipeline and formats conversion.

use anyhow::{Result, anyhow};
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
    pub(crate) consumer: HeapCons<i16>,
    last_underrun_log: Option<Instant>,
    underrun_frames: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FillStats {
    pub frames_filled: usize,
    pub underrun_frames: usize,
}

impl AudioQueue {
    // Creates a new bounded stereo sound queue buffer.
    pub fn new(capacity_frames: usize, sample_rate: f64) -> (AudioProducer, AudioConsumer) {
        let capacity_frames = capacity_frames.max(1);
        let capacity_samples = capacity_frames * CHANNELS;
        let rb = HeapRb::<i16>::new(capacity_samples);
        let (producer, consumer) = rb.split();
        info!("Audio queue initialized: capacity_frames={capacity_frames}, sample_rate={sample_rate}");
        (
            AudioProducer { producer, muted: false, dropped_frames: 0 },
            AudioConsumer { consumer, last_underrun_log: None, underrun_frames: 0 },
        )
    }

    // Creates a queue matching the specified sample rate in Hz.
    pub fn for_sample_rate(sample_rate: u32) -> (AudioProducer, AudioConsumer) {
        let capacity_frames = (sample_rate as usize * QUEUE_MS) / 1000;
        Self::new(capacity_frames, sample_rate as f64)
    }
}

impl AudioProducer {
    // Pushes a single stereo sound frame to the queue.
    pub fn push_frame(&mut self, left: i16, right: i16) -> usize {
        self.push_frames(&[left, right], 1)
    }

    // Pushes a block of interleaved stereo frames.
    pub fn push_frames(&mut self, samples: &[i16], frames: usize) -> usize {
        if self.muted {
            return (samples.len() / CHANNELS).min(frames);
        }
        let mut queued = 0;
        for f in samples.chunks_exact(CHANNELS).take(frames) {
            if self.producer.try_push(f[0]).is_err() || self.producer.try_push(f[1]).is_err() {
                self.dropped_frames += ((samples.len() / CHANNELS).min(frames) - queued) as u64;
                break;
            }
            queued += 1;
        }
        queued
    }

    // Sets whether audio production is muted.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}

impl AudioConsumer {
    // Returns the number of unread stereo samples.
    pub fn occupied_len(&self) -> usize {
        self.consumer.occupied_len()
    }

    // Fills an output buffer with signed 16-bit sound samples.
    pub fn fill_i16(&mut self, out: &mut [i16]) -> FillStats {
        let mut samples_filled = 0;
        for sample in out.iter_mut() {
            let val = self.next_sample();
            *sample = val.unwrap_or(0);
            if val.is_some() {
                samples_filled += 1;
            }
        }
        self.finish_fill(samples_filled, out.len())
    }

    // Fills an output buffer with converted 32-bit floating point samples.
    pub fn fill_f32(&mut self, out: &mut [f32]) {
        let mut samples_filled = 0;
        for sample in out.iter_mut() {
            let val = self.next_sample();
            *sample = val.map(|v| v as f32 / i16::MAX as f32).unwrap_or(0.0);
            if val.is_some() {
                samples_filled += 1;
            }
        }
        self.finish_fill(samples_filled, out.len());
    }

    // Fills an output buffer with converted unsigned 16-bit samples.
    pub fn fill_u16(&mut self, out: &mut [u16]) {
        let mut samples_filled = 0;
        for sample in out.iter_mut() {
            let val = self.next_sample();
            *sample = val.map(|v| (v as i32 + 32768) as u16).unwrap_or(32768);
            if val.is_some() {
                samples_filled += 1;
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

// Validates the sample rate reported by the emulation core.
pub fn validate_sample_rate(sample_rate: f64) -> Result<u32> {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Err(anyhow!(
            "Core reported invalid audio sample rate: {}",
            sample_rate
        ));
    }

    let rounded = sample_rate.round();
    if (sample_rate - rounded).abs() > 0.5 {
        return Err(anyhow!(
            "Core reported non-integer audio sample rate: {}",
            sample_rate
        ));
    }

    Ok(rounded as u32)
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
