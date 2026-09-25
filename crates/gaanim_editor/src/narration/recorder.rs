//! Microphone capture for narration takes.
//!
//! The capture stream lives on its own thread (streams are not `Send` on
//! every platform) and appends mono samples to a shared buffer. The number of
//! captured samples is the recording clock: it is what the take will contain,
//! so markers and holds measured with it line up with the saved audio.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Default)]
struct Shared {
    samples: Mutex<Vec<f32>>,
    /// Peak magnitude of any channel since the last read, as `f32` bits.
    peak: AtomicU32,
    /// Audio buffers (about 10 ms each) with at least one clipped sample.
    clipped: AtomicU32,
    error: Mutex<Option<String>>,
}

/// A sample at or above this magnitude hit the converter's ceiling.
const CLIP_LEVEL: f32 = 0.99;

pub(crate) struct MicRecorder {
    shared: Arc<Shared>,
    stop: Option<crossbeam_channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    sample_rate: u32,
    device: String,
}

impl MicRecorder {
    /// Start capturing from the default input device.
    pub(crate) fn start() -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let stream_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("gaanim-microphone".into())
            .spawn(move || {
                let stream = match open_default_input(stream_shared) {
                    Ok((stream, rate, device)) => {
                        let _ = ready_tx.send(Ok((rate, device)));
                        stream
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                // Either a stop request or a dropped recorder ends the take.
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|error| error.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok((sample_rate, device))) => Ok(Self {
                shared,
                stop: Some(stop_tx),
                thread: Some(thread),
                sample_rate,
                device,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err("el micrófono no respondió".to_string()),
        }
    }

    /// Seconds captured so far.
    pub(crate) fn elapsed(&self) -> f64 {
        let samples = self
            .shared
            .samples
            .lock()
            .expect("microphone buffer poisoned");
        samples.len() as f64 / self.sample_rate.max(1) as f64
    }

    /// Peak level since the previous call, from 0 to 1.
    pub(crate) fn take_peak(&self) -> f32 {
        f32::from_bits(self.shared.peak.swap(0, Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    /// How many short stretches of the take clipped.
    pub(crate) fn clipped(&self) -> u32 {
        self.shared.clipped.load(Ordering::Relaxed)
    }

    /// Discard what was captured so far and start the take clock at zero,
    /// e.g. after the level check of the countdown.
    pub(crate) fn restart(&self) {
        self.shared
            .samples
            .lock()
            .expect("microphone buffer poisoned")
            .clear();
        self.shared.clipped.store(0, Ordering::Relaxed);
    }

    /// A stream error reported by the audio backend, if any.
    pub(crate) fn error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .expect("microphone error poisoned")
            .clone()
    }

    pub(crate) fn device(&self) -> &str {
        &self.device
    }

    /// Stop capturing and return the mono samples and their sample rate.
    pub(crate) fn finish(mut self) -> (Vec<f32>, u32) {
        self.halt();
        let samples = std::mem::take(
            &mut *self
                .shared
                .samples
                .lock()
                .expect("microphone buffer poisoned"),
        );
        (samples, self.sample_rate)
    }

    fn halt(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for MicRecorder {
    fn drop(&mut self) {
        self.halt();
    }
}

fn open_default_input(shared: Arc<Shared>) -> Result<(cpal::Stream, u32, String), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no hay ningún micrófono disponible".to_string())?;
    let name = device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| "micrófono".to_string());
    let config = device
        .default_input_config()
        .map_err(|error| format!("no se pudo configurar el micrófono: {error}"))?;
    let channels = usize::from(config.channels().max(1));
    let rate = config.sample_rate();
    let stream_config = config.config();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &stream_config, channels, shared),
        cpal::SampleFormat::F64 => build::<f64>(&device, &stream_config, channels, shared),
        cpal::SampleFormat::I16 => build::<i16>(&device, &stream_config, channels, shared),
        cpal::SampleFormat::I32 => build::<i32>(&device, &stream_config, channels, shared),
        cpal::SampleFormat::I8 => build::<i8>(&device, &stream_config, channels, shared),
        cpal::SampleFormat::U16 => build::<u16>(&device, &stream_config, channels, shared),
        cpal::SampleFormat::U8 => build::<u8>(&device, &stream_config, channels, shared),
        other => Err(format!("formato de micrófono no soportado: {other}")),
    }?;
    stream
        .play()
        .map_err(|error| format!("no se pudo iniciar el micrófono: {error}"))?;
    Ok((stream, rate, name))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let error_shared = shared.clone();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| push_frames(&shared, data, channels),
            move |error| {
                *error_shared
                    .error
                    .lock()
                    .expect("microphone error poisoned") = Some(error.to_string());
            },
            None,
        )
        .map_err(|error| format!("no se pudo abrir el micrófono: {error}"))
}

/// Mix interleaved frames down to mono and track their peak.
fn push_frames<T>(shared: &Shared, data: &[T], channels: usize)
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let peak = channel_peak(data);
    if peak >= CLIP_LEVEL {
        shared.clipped.fetch_add(1, Ordering::Relaxed);
    }
    let mono = mix_to_mono(data, channels);
    shared
        .samples
        .lock()
        .expect("microphone buffer poisoned")
        .extend_from_slice(&mono);
    // Non-negative floats order like their bit patterns.
    shared.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
}

/// Loudest sample of any channel: mixing to mono would hide a clipped side.
fn channel_peak<T>(data: &[T]) -> f32
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    data.iter().fold(0.0_f32, |peak, sample| {
        peak.max(<f32 as cpal::FromSample<T>>::from_sample_(*sample).abs())
    })
}

fn mix_to_mono<T>(data: &[T], channels: usize) -> Vec<f32>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    data.chunks(channels.max(1))
        .map(|frame| {
            frame
                .iter()
                .map(|sample| <f32 as cpal::FromSample<T>>::from_sample_(*sample))
                .sum::<f32>()
                / frame.len() as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaved_frames_mix_down_to_mono() {
        assert_eq!(mix_to_mono(&[0.5_f32, -0.5, 1.0, 0.0], 2), [0.0, 0.5]);
        let mono = mix_to_mono(&[i16::MAX, i16::MAX], 1);
        assert!((mono[0] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_clipped_channel_counts_once_per_buffer() {
        let shared = Shared::default();
        // The left channel clips twice; the mono mix alone would not show it.
        push_frames(&shared, &[1.0_f32, 0.0, -1.0, 0.0], 2);
        push_frames(&shared, &[0.5_f32, 0.25], 2);
        assert_eq!(shared.clipped.load(Ordering::Relaxed), 1);
        assert_eq!(f32::from_bits(shared.peak.load(Ordering::Relaxed)), 1.0);
        assert_eq!(*shared.samples.lock().unwrap(), [0.5, -0.5, 0.375]);
    }

    /// Needs a microphone:
    /// `just test-package gaanim_editor --lib -- --ignored microphone`.
    #[test]
    #[ignore = "records from the default microphone"]
    fn records_a_take_from_the_default_microphone() {
        let recorder = MicRecorder::start().expect("the default microphone should start");
        std::thread::sleep(Duration::from_millis(600));
        let elapsed = recorder.elapsed();
        let (samples, sample_rate) = recorder.finish();
        assert!(elapsed > 0.3, "captured only {elapsed} s");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("take.wav");
        gaanim_media::narration::write_wav_take(&path, &samples, sample_rate).unwrap();
        let duration = gaanim_media::narration::audio_duration(&path).unwrap();
        assert!((duration - samples.len() as f64 / sample_rate as f64).abs() < 1e-6);
    }
}
