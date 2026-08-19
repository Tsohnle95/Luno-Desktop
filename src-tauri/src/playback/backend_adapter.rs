//! cpal/symphonia implementation of the backend contract (macOS release target).
//!
//! Playback is streaming: a producer thread keeps a bounded buffer of decoded
//! stereo 44.1 kHz frames just ahead of the cpal output callback, so a track
//! starts in a fraction of a second (no full-file decode upfront) and seeks
//! land via Symphonia's format seek. Volume is applied by the output callback.

#![cfg(target_os = "macos")]

use crate::playback::backend::{
    AudioBackend, BackendError, OutputFormat, TrackInfo, CHANNELS, SAMPLE_RATE,
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Stream, StreamConfig,
};
use std::collections::VecDeque;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::DecoderOptions,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

/// How far ahead (in stereo frames, ~2 seconds at 44.1 kHz) the producer may
/// decode while staying bounded.
const BUFFER_CAPACITY_FRAMES: usize = SAMPLE_RATE as usize * 2;

#[derive(Default)]
pub struct CpalSymphoniaBackend {
    track: Option<TrackInfo>,
    state: Option<Arc<Mutex<OutputState>>>,
    commands: Option<mpsc::Sender<StreamCommand>>,
    thread: Option<JoinHandle<()>>,
    producer: Option<ProducerHandle>,
}

struct ProducerHandle {
    stop: mpsc::Sender<ProducerCommand>,
    join: Option<JoinHandle<()>>,
}

enum ProducerCommand {
    Stop,
}

#[derive(Debug)]
struct OutputState {
    /// Interleaved stereo frames decoded but not yet played.
    buffer: VecDeque<f32>,
    /// Frames consumed by the output callback; the authoritative position.
    played_frames: u64,
    playing: bool,
    /// The producer reached EOF (or a fatal decode error).
    ended: bool,
    volume: f32,
    /// Frame target requested by `seek`; consumed by the producer thread.
    seek_to: Option<u64>,
}

enum StreamCommand {
    Stop,
}

impl CpalSymphoniaBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn open_track(
        path: &Path,
    ) -> Result<(Box<dyn FormatReader>, u32, Option<Duration>), BackendError> {
        let file = File::open(path).map_err(|e| BackendError::Io(e.to_string()))?;
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }
        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| BackendError::Decode(e.to_string()))?;
        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .ok_or_else(|| BackendError::Decode("file has no audio track".into()))?
            .id;
        let duration = probed.format.tracks()[track as usize].codec_params.time_base.and_then(
            |tb| {
                probed.format.tracks()[track as usize]
                    .codec_params
                    .n_frames
                    .map(|frames| {
                        let time = tb.calc_time(frames);
                        Duration::from_secs_f64(time.seconds as f64 + time.frac)
                    })
            },
        );
        Ok((probed.format, track, duration))
    }

    fn producer_loop(
        path: PathBuf,
        state: Arc<Mutex<OutputState>>,
        stop: mpsc::Receiver<ProducerCommand>,
    ) {
        let (mut format, track_id, _) = match Self::open_track(&path) {
            Ok(probed) => probed,
            Err(error) => {
                eprintln!("audio producer could not open track: {error}");
                state.lock().unwrap().ended = true;
                return;
            }
        };
        let mut decoder = match symphonia::default::get_codecs().make(
            &format.tracks()[track_id as usize].codec_params,
            &DecoderOptions::default(),
        ) {
            Ok(decoder) => decoder,
            Err(error) => {
                eprintln!("audio producer could not build decoder: {error}");
                state.lock().unwrap().ended = true;
                return;
            }
        };
        loop {
            if !matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                break; // stopped or disconnected
            }
            // Pull a pending seek before consuming anything else.
            let pending_seek = state.lock().unwrap().seek_to;
            if let Some(target) = pending_seek {
                state.lock().unwrap().seek_to = None;
                let seconds = (target / SAMPLE_RATE as u64) as u64;
                let to = SeekTo::Time {
                    time: symphonia::core::units::Time::new(seconds, 0.0),
                    track_id: Some(track_id),
                };
                if let Err(error) = format.seek(SeekMode::Coarse, to) {
                    eprintln!("audio seek failed at {seconds}s: {error}");
                    continue;
                }
                match format.tracks().iter().find(|t| t.id == track_id) {
                    Some(track) => match symphonia::default::get_codecs().make(
                        &track.codec_params,
                        &DecoderOptions::default(),
                    ) {
                        Ok(next) => {
                            decoder = next;
                            let mut state = state.lock().unwrap();
                            state.buffer.clear();
                            state.played_frames = target;
                            state.ended = false;
                        }
                        Err(error) => {
                            eprintln!("audio seek: decoder rebuild failed: {error}");
                        }
                    },
                    None => {
                        eprintln!("audio seek: audio track vanished after seek");
                    }
                }
                continue;
            }
            let full = {
                let state = state.lock().unwrap();
                state.buffer.len() / (CHANNELS as usize) >= BUFFER_CAPACITY_FRAMES
            };
            if full {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            match format.next_packet() {
                Ok(packet) => {
                    if packet.track_id() != track_id {
                        continue;
                    }
                    match decoder.decode(&packet) {
                        Ok(decoded) => {
                            let mut chunk = Vec::new();
                            append_stereo_44100(&decoded, &mut chunk);
                            let mut state = state.lock().unwrap();
                            // A seek arrived while decoding; drop this stale chunk.
                            if state.seek_to.is_some() {
                                continue;
                            }
                            state.buffer.extend(chunk);
                        }
                        Err(error) => {
                            eprintln!("audio decode error: {error}");
                        }
                    }
                }
                Err(symphonia::core::errors::Error::IoError(_)) => {
                    state.lock().unwrap().ended = true;
                    break; // expected end of stream
                }
                Err(error) => {
                    eprintln!("audio stream ended with error: {error}");
                    state.lock().unwrap().ended = true;
                    break;
                }
            }
        }
        let _ = state.lock().unwrap().ended;
    }

    fn spawn_producer(
        &mut self,
        path: PathBuf,
        state: Arc<Mutex<OutputState>>,
    ) -> Result<(), BackendError> {
        self.stop_producer();
        let (stop_tx, stop_rx) = mpsc::channel::<ProducerCommand>();
        let thread = std::thread::Builder::new()
            .name("luno-audio-decode".into())
            .spawn(move || Self::producer_loop(path, state, stop_rx))
            .map_err(|e| BackendError::Device(e.to_string()))?;
        self.producer = Some(ProducerHandle {
            stop: stop_tx,
            join: Some(thread),
        });
        Ok(())
    }

    fn stop_producer(&mut self) {
        if let Some(producer) = self.producer.take() {
            let _ = producer.stop.send(ProducerCommand::Stop);
            if let Some(join) = producer.join {
                let _ = join.join();
            }
        }
    }

    fn stream(state: Arc<Mutex<OutputState>>) -> Result<Stream, BackendError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| BackendError::Device("no default output device".into()))?;
        let config = StreamConfig {
            channels: CHANNELS,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };
        let error = |error| eprintln!("audio output stream error: {error}");
        device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    let mut state = state.lock().unwrap();
                    let volume = state.volume;
                    for frame in data.chunks_mut(CHANNELS as usize) {
                        let mut left = 0.0f32;
                        let mut right = 0.0f32;
                        if state.playing {
                            if state.buffer.len() >= 2 {
                                left = state.buffer.pop_front().unwrap();
                                right = state.buffer.pop_front().unwrap();
                                state.played_frames += 1;
                            } else if state.ended {
                                state.playing = false;
                            }
                            // While decoding: underrun = keep silence, stay playing.
                        }
                        frame[0] = left * volume;
                        if let Some(slot) = frame.get_mut(1) {
                            *slot = right * volume;
                        }
                    }
                },
                error,
                None,
            )
            .map_err(|e| BackendError::Device(e.to_string()))
    }

    fn stop_stream(&mut self) {
        if let Some(commands) = self.commands.take() {
            let _ = commands.send(StreamCommand::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn start_stream(&mut self, state: Arc<Mutex<OutputState>>) -> Result<(), BackendError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (commands_tx, commands_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("luno-audio-output".into())
            .spawn(move || match Self::stream(state) {
                Ok(stream) => {
                    if stream.play().is_err() {
                        let _ = ready_tx.send(Err(BackendError::Device(
                            "could not start output stream".into(),
                        )));
                        return;
                    }
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    let _stream = stream;
                    let _ = commands_rx.recv();
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })
            .map_err(|e| BackendError::Device(e.to_string()))?;
        match ready_rx
            .recv()
            .map_err(|e| BackendError::Device(e.to_string()))?
        {
            Ok(()) => {
                self.commands = Some(commands_tx);
                self.thread = Some(thread);
                Ok(())
            }
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }
}

fn append_stereo_44100(buffer: &AudioBufferRef<'_>, output: &mut Vec<f32>) {
    let spec = buffer.spec();
    let channels = spec.channels.count().max(1);
    let rate = spec.rate;
    let frames = buffer.frames();
    let mut samples = vec![0.0f32; frames * channels];
    match buffer {
        AudioBufferRef::F32(buffer) => {
            for channel in 0..channels {
                for frame in 0..frames {
                    samples[frame * channels + channel] = buffer.chan(channel)[frame];
                }
            }
        }
        _ => {
            let mut converted =
                symphonia::core::audio::SampleBuffer::<f32>::new(frames as u64, *spec);
            converted.copy_interleaved_ref(buffer.clone());
            samples.copy_from_slice(converted.samples());
        }
    }
    let output_frames = ((frames as u64 * SAMPLE_RATE as u64) / rate as u64) as usize;
    for frame in 0..output_frames {
        let source = ((frame as u64 * rate as u64) / SAMPLE_RATE as u64) as usize;
        let left = samples[source.min(frames.saturating_sub(1)) * channels];
        let right = samples[source.min(frames.saturating_sub(1)) * channels + channels.min(2) - 1];
        output.extend_from_slice(&[left, right]);
    }
}

impl AudioBackend for CpalSymphoniaBackend {
    fn load(&mut self, path: &Path) -> Result<TrackInfo, BackendError> {
        self.load_streaming(path)
    }

    fn load_streaming(&mut self, path: &Path) -> Result<TrackInfo, BackendError> {
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "mp3" | "m4a" | "mp4" | "flac" | "wav") {
            return Err(BackendError::UnsupportedFormat(ext));
        }
        self.stop_stream();
        self.stop_producer();
        // Loading replaces the output state, so carry the master volume across
        // it: the volume slider is a master output setting, not per-track, and
        // a fresh track must never reset the user's volume back to 1.0.
        let prior_volume = self
            .state
            .as_ref()
            .map(|state| state.lock().map(|state| state.volume).unwrap_or(1.0))
            .unwrap_or(1.0);
        let (_, _, duration) = Self::open_track(path)?;
        let info = TrackInfo {
            path: path.to_path_buf(),
            duration,
            format: OutputFormat {
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
            },
        };
        let state = Arc::new(Mutex::new(OutputState {
            buffer: VecDeque::new(),
            played_frames: 0,
            playing: false,
            ended: false,
            volume: prior_volume.clamp(0.0, 1.0),
            seek_to: None,
        }));
        self.spawn_producer(path.to_path_buf(), state.clone())?;
        self.state = Some(state);
        self.track = Some(info.clone());
        Ok(info)
    }

    fn set_volume(&mut self, volume: f32) {
        if let Some(state) = &self.state {
            state.lock().unwrap().volume = volume.clamp(0.0, 1.0);
        }
    }

    fn play(&mut self) -> Result<(), BackendError> {
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?.clone();
        if self.commands.is_none() {
            self.start_stream(state.clone())?;
        }
        state.lock().unwrap().playing = true;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), BackendError> {
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?;
        state.lock().unwrap().playing = false;
        Ok(())
    }

    fn seek(&mut self, position: Duration) -> Result<(), BackendError> {
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?.clone();
        let frame = position.as_secs_f64().mul_add(SAMPLE_RATE as f64, 0.0) as u64;
        let ended = {
            let mut state = state.lock().unwrap();
            state.seek_to = Some(frame);
            state.ended
        };
        if ended {
            // The producer already finished the file; restart it at the target.
            let info = self.track.as_ref().ok_or(BackendError::NotLoaded)?.clone();
            self.spawn_producer(info.path, state)?;
        }
        Ok(())
    }

    fn position(&self) -> Result<Duration, BackendError> {
        let state = self
            .state
            .as_ref()
            .ok_or(BackendError::NotLoaded)?
            .lock()
            .unwrap();
        Ok(Duration::from_secs_f64(
            state.played_frames as f64 / SAMPLE_RATE as f64,
        ))
    }

    fn duration(&self) -> Result<Option<Duration>, BackendError> {
        self.track
            .as_ref()
            .map(|t| t.duration)
            .ok_or(BackendError::NotLoaded)
    }

    fn stop(&mut self) {
        self.stop_stream();
        self.stop_producer();
        if let Some(state) = &self.state {
            state.lock().unwrap().playing = false;
        }
    }

    fn shutdown(&mut self) {
        self.stop();
    }
}

impl Drop for CpalSymphoniaBackend {
    fn drop(&mut self) {
        self.stop_stream();
        self.stop_producer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic 44.1 kHz stereo PCM WAV, valid enough for the Symphonia
    /// probe used by `open_track`.
    fn wav_fixture(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("luno-stream-{label}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let path = root.join("track.wav");
        let sample_rate = 44_100u32;
        let channels = 2u16;
        let bits_per_sample = 16u16;
        let frames = 441u32;
        let data_size = frames * channels as u32 * (bits_per_sample as u32 / 8);
        let mut wav = Vec::with_capacity(44 + data_size as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
        wav.extend_from_slice(&(channels * 2).to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for frame in 0..frames {
            wav.extend_from_slice(&(frame as i16).to_le_bytes());
            wav.extend_from_slice(&(frame as i16).to_le_bytes());
        }
        std::fs::write(&path, wav).unwrap();
        path
    }

    #[test]
    fn master_volume_survives_track_reload() {
        let mut backend = CpalSymphoniaBackend::new();
        let path = wav_fixture("reload");
        backend.load_streaming(&path).expect("first track should load");
        assert_eq!(
            backend.state.as_ref().unwrap().lock().unwrap().volume,
            1.0,
            "a fresh backend starts at full volume"
        );
        backend.set_volume(0.3);
        // A transition reloads the output state; the user's volume must carry
        // over instead of snapping back to 1.0.
        backend.load_streaming(&path).expect("reload should swap the output state");
        let volume = backend.state.as_ref().unwrap().lock().unwrap().volume;
        assert!(
            (volume - 0.3).abs() < 1e-6,
            "master volume must survive a track reload, got {volume}"
        );
    }
}

