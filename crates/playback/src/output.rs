//! Sound outputs. An output pulls samples from the player's ring buffer in its own callback;
//! [`Shared`] carries the few things the player and the callback both touch.

use rtrb::Consumer;
use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

/// State shared between the player thread, the [`crate::Player`] handle and the output callback.
/// Everything is atomic: the callback must never wait for a lock. An [`OutputFactory`] only
/// needs [`Shared::fill`].
#[derive(Debug)]
pub struct Shared {
    /// Volume 0–1, as `f32` bits.
    volume: AtomicU32,
    /// Whether the callback should play; when false it outputs silence and keeps the queue.
    pub(crate) playing: AtomicBool,
    /// Frames the callback has played since the player last reset it.
    pub(crate) played_frames: AtomicU64,
    /// Set by the player to have the callback discard everything queued; cleared by the
    /// callback once done.
    pub(crate) flush: AtomicBool,
}

impl Shared {
    pub(crate) fn new() -> Self {
        Self {
            volume: AtomicU32::new(1.0f32.to_bits()),
            playing: AtomicBool::new(false),
            played_frames: AtomicU64::new(0),
            flush: AtomicBool::new(false),
        }
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub(crate) fn set_volume(&self, volume: f32) {
        let volume = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 1.0 };
        self.volume.store(volume.to_bits(), Ordering::Relaxed);
    }

    /// The body of every output callback: fills `out` (interleaved, `channels` wide) from the
    /// queue, applying volume, and pads with silence when paused or starved.
    pub fn fill(&self, out: &mut [f32], queue: &mut Consumer<f32>, channels: u16) {
        if self.flush.load(Ordering::Acquire) {
            let queued = queue.slots();
            if let Ok(chunk) = queue.read_chunk(queued) {
                chunk.commit_all();
            }
            self.flush.store(false, Ordering::Release);
        }
        if !self.playing.load(Ordering::Relaxed) {
            out.fill(0.0);
            return;
        }
        let (filled, rest) = queue.pop_partial_slice(out);
        let volume = self.volume();
        // A perceptual curve: the slider's midpoint sounds like half as loud.
        let gain = volume * volume;
        filled.iter_mut().for_each(|s| *s *= gain);
        let frames = filled.len() / usize::from(channels.max(1));
        rest.fill(0.0);
        self.played_frames.fetch_add(frames as u64, Ordering::Relaxed);
    }
}

/// The format an output runs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Opens sound outputs. Lives on the player thread; the stream it returns stays there too
/// (some platforms' streams cannot move between threads).
pub trait OutputFactory: Send + 'static {
    /// The format the output will run at when asked for `wanted` (the file's own format).
    fn negotiate(&mut self, wanted: OutputFormat) -> Result<OutputFormat, String>;

    /// Starts an output in `format` that drains `queue`. Dropping the returned value stops it.
    fn start(
        &mut self,
        format: OutputFormat,
        queue: Consumer<f32>,
        shared: Arc<Shared>,
    ) -> Result<Box<dyn Any>, String>;
}

/// An output with no sound device: a thread drains the queue at `speed` times real time.
/// Headless tools use it at 1×; tests run it fast.
#[derive(Clone, Copy, Debug)]
pub struct NullOutput {
    pub speed: f64,
}

impl Default for NullOutput {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
}

/// Stops a [`NullOutput`] thread when dropped.
struct NullStream {
    stop: Arc<AtomicBool>,
}

impl Drop for NullStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl OutputFactory for NullOutput {
    fn negotiate(&mut self, wanted: OutputFormat) -> Result<OutputFormat, String> {
        Ok(wanted)
    }

    fn start(
        &mut self,
        format: OutputFormat,
        mut queue: Consumer<f32>,
        shared: Arc<Shared>,
    ) -> Result<Box<dyn Any>, String> {
        const FRAMES: usize = 512;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let period =
            Duration::from_secs_f64(FRAMES as f64 / f64::from(format.sample_rate.max(1)) / self.speed.max(0.001));
        std::thread::Builder::new()
            .name("tunebox-null-output".into())
            .spawn(move || {
                let mut buffer = vec![0.0; FRAMES * usize::from(format.channels.max(1))];
                while !stop_flag.load(Ordering::Relaxed) {
                    shared.fill(&mut buffer, &mut queue, format.channels);
                    std::thread::sleep(period);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Box::new(NullStream { stop }))
    }
}

#[cfg(feature = "cpal")]
pub use cpal_output::CpalOutput;

#[cfg(feature = "cpal")]
mod cpal_output {
    use super::{OutputFactory, OutputFormat, Shared};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
    use rtrb::Consumer;
    use std::any::Any;
    use std::sync::Arc;

    /// The system's default sound output.
    #[derive(Debug, Default)]
    pub struct CpalOutput {
        sample_format: Option<SampleFormat>,
    }

    impl CpalOutput {
        fn device() -> Result<cpal::Device, String> {
            cpal::default_host()
                .default_output_device()
                .ok_or_else(|| "No sound output device is available.".to_owned())
        }
    }

    impl OutputFactory for CpalOutput {
        /// Prefers the file's own rate and channel count, so audio passes through unconverted;
        /// otherwise takes the device's default format.
        fn negotiate(&mut self, wanted: OutputFormat) -> Result<OutputFormat, String> {
            let device = Self::device()?;
            let exact = device.supported_output_configs().ok().and_then(|mut configs| {
                configs.find(|c| {
                    c.channels() == wanted.channels
                        && (c.min_sample_rate()..=c.max_sample_rate()).contains(&wanted.sample_rate)
                        && matches!(c.sample_format(), SampleFormat::F32 | SampleFormat::I16)
                })
            });
            let config = match exact {
                Some(range) => range.with_sample_rate(wanted.sample_rate),
                None => device.default_output_config().map_err(|e| e.to_string())?,
            };
            self.sample_format = Some(config.sample_format());
            Ok(OutputFormat { sample_rate: config.sample_rate(), channels: config.channels() })
        }

        fn start(
            &mut self,
            format: OutputFormat,
            queue: Consumer<f32>,
            shared: Arc<Shared>,
        ) -> Result<Box<dyn Any>, String> {
            let device = Self::device()?;
            let config = StreamConfig {
                channels: format.channels,
                sample_rate: format.sample_rate,
                buffer_size: cpal::BufferSize::Default,
            };
            let stream = match self.sample_format.unwrap_or(SampleFormat::F32) {
                SampleFormat::I16 => build::<i16>(&device, &config, queue, shared)?,
                SampleFormat::U16 => build::<u16>(&device, &config, queue, shared)?,
                SampleFormat::I32 => build::<i32>(&device, &config, queue, shared)?,
                _ => build::<f32>(&device, &config, queue, shared)?,
            };
            stream.play().map_err(|e| e.to_string())?;
            Ok(Box::new(stream))
        }
    }

    /// A stream whose callback fills an `f32` scratch buffer through [`Shared::fill`] and
    /// converts it to the device's sample type.
    fn build<T>(
        device: &cpal::Device,
        config: &StreamConfig,
        mut queue: Consumer<f32>,
        shared: Arc<Shared>,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = config.channels;
        let mut scratch: Vec<f32> = Vec::new();
        device
            .build_output_stream::<T, _, _>(
                *config,
                move |out: &mut [T], _| {
                    // Grows once to the device's period size, then never allocates again.
                    scratch.resize(out.len(), 0.0);
                    shared.fill(&mut scratch, &mut queue, channels);
                    for (dst, src) in out.iter_mut().zip(&scratch) {
                        *dst = T::from_sample(*src);
                    }
                },
                |error| log::warn!("sound output error: {error}"),
                None,
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_applies_volume_and_pads_with_silence() {
        let shared = Shared::new();
        shared.set_volume(0.5);
        shared.playing.store(true, Ordering::Relaxed);
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(16);
        let _ = producer.push_partial_slice(&[1.0, 1.0]);
        let mut out = [9.0; 4];
        shared.fill(&mut out, &mut consumer, 2);
        assert_eq!(out, [0.25, 0.25, 0.0, 0.0]);
        assert_eq!(shared.played_frames.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn paused_output_keeps_the_queue() {
        let shared = Shared::new();
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(16);
        let _ = producer.push_partial_slice(&[1.0, 1.0]);
        let mut out = [9.0; 2];
        shared.fill(&mut out, &mut consumer, 2);
        assert_eq!(out, [0.0, 0.0]);
        assert_eq!(consumer.slots(), 2);
    }

    #[test]
    fn flush_discards_the_queue() {
        let shared = Shared::new();
        shared.flush.store(true, Ordering::Relaxed);
        let (mut producer, mut consumer) = rtrb::RingBuffer::new(16);
        let _ = producer.push_partial_slice(&[1.0; 6]);
        shared.fill(&mut [0.0; 2], &mut consumer, 2);
        assert_eq!(consumer.slots(), 0);
        assert!(!shared.flush.load(Ordering::Relaxed));
    }

    #[test]
    fn volume_is_clamped() {
        let shared = Shared::new();
        shared.set_volume(3.0);
        assert_eq!(shared.volume(), 1.0);
        shared.set_volume(f32::NAN);
        assert_eq!(shared.volume(), 1.0);
    }
}
