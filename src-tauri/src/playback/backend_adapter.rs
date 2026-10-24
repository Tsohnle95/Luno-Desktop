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
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use symphonia::core::{
    audio::GenericAudioBufferRef,
    codecs::{
        audio::{AudioDecoder, AudioDecoderOptions},
        registry::CodecRegistry,
        CodecParameters,
    },
    formats::{probe::Hint, FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

/// How far ahead (in stereo frames, ~2 seconds at 44.1 kHz) the producer may
/// decode while staying bounded.
const BUFFER_CAPACITY_FRAMES: usize = SAMPLE_RATE as usize * 2;
const MAX_RETIRED_PRODUCERS: usize = 8;

type OutputSlot = Arc<Mutex<Option<Arc<Mutex<OutputState>>>>>;

fn audio_codec_registry() -> &'static CodecRegistry {
    static CODECS: OnceLock<CodecRegistry> = OnceLock::new();
    CODECS.get_or_init(|| {
        let mut codecs = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut codecs);
        codecs.register_audio_decoder::<super::opus::OpusAdapter>();
        codecs
    })
}

pub struct CpalSymphoniaBackend {
    track: Option<TrackInfo>,
    state: Option<Arc<Mutex<OutputState>>>,
    commands: Option<mpsc::Sender<StreamCommand>>,
    thread: Option<JoinHandle<()>>,
    producer: Option<ProducerHandle>,
    retired_producers: Vec<JoinHandle<()>>,
    output_slot: OutputSlot,
    output_failed: Arc<AtomicBool>,
    master_volume: Arc<AtomicU32>,
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
    /// Filled by the producer as soon as the container header is available.
    /// This lets playback start without a duplicate synchronous pre-probe.
    duration: Option<Duration>,
    playing: bool,
    /// The producer reached EOF (or a fatal decode error).
    ended: bool,
    /// Frame target requested by `seek`; consumed by the producer thread.
    seek_to: Option<u64>,
}

enum StreamCommand {
    Stop,
}

impl CpalSymphoniaBackend {
    pub fn new() -> Self {
        Self::with_volume(Arc::new(AtomicU32::new(1.0f32.to_bits())))
    }

    pub fn with_volume(master_volume: Arc<AtomicU32>) -> Self {
        Self {
            track: None, state: None, commands: None, thread: None, producer: None,
            retired_producers: Vec::new(), output_slot: Arc::new(Mutex::new(None)),
            output_failed: Arc::new(AtomicBool::new(false)), master_volume,
        }
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
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                source,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| BackendError::Decode(e.to_string()))?;
        let codecs = audio_codec_registry();
        let track = format
            .tracks()
            .iter()
            .find(|track| {
                track
                    .codec_params
                    .as_ref()
                    .and_then(CodecParameters::audio)
                    .is_some_and(|params| codecs.get_audio_decoder(params.codec).is_some())
            })
            .ok_or_else(|| BackendError::Decode("file has no supported audio track".into()))?;
        let track_id = track.id;
        let duration = track
            .time_base
            .zip(track.duration)
            .and_then(|(time_base, duration)| time_base.calc_duration(duration))
            .and_then(|time| u64::try_from(time.as_nanos()).ok())
            .map(Duration::from_nanos);
        Ok((format, track_id, duration))
    }

    fn make_decoder(
        format: &dyn FormatReader,
        track_id: u32,
    ) -> Result<Box<dyn AudioDecoder>, BackendError> {
        let params = format
            .tracks()
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.codec_params.as_ref())
            .and_then(CodecParameters::audio)
            .cloned()
            .ok_or_else(|| BackendError::Decode("selected track is not an audio stream".into()))?;

        audio_codec_registry()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|error| BackendError::Decode(error.to_string()))
    }

    /// Probes a file's duration WITHOUT constructing or locking any backend
    /// state, so callers can keep potentially-slow storage (cold disk,
    /// iCloud materialization) OFF the shared audio mutex.
    pub fn probe_duration(path: &Path) -> Result<Option<Duration>, BackendError> {
        Ok(Self::open_track(path)?.2)
    }

    fn producer_loop(
        path: PathBuf,
        state: Arc<Mutex<OutputState>>,
        stop: mpsc::Receiver<ProducerCommand>,
    ) {
        let (mut format, track_id, duration) = match Self::open_track(&path) {
            Ok(probed) => probed,
            Err(error) => {
                eprintln!("audio producer could not open track: {error}");
                state.lock().unwrap_or_else(|e| e.into_inner()).ended = true;
                return;
            }
        };
        if duration.is_some() {
            state.lock().unwrap_or_else(|e| e.into_inner()).duration = duration;
        }
        let mut decoder = match Self::make_decoder(format.as_ref(), track_id) {
            Ok(decoder) => decoder,
            Err(error) => {
                eprintln!("audio producer could not build decoder: {error}");
                state.lock().unwrap_or_else(|e| e.into_inner()).ended = true;
                return;
            }
        };
        loop {
            if !matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                break; // stopped or disconnected
            }
            // Pull a pending seek before consuming anything else.
            let pending_seek = state.lock().unwrap_or_else(|e| e.into_inner()).seek_to;
            if let Some(target) = pending_seek {
                state.lock().unwrap_or_else(|e| e.into_inner()).seek_to = None;
                let nanos = u128::from(target) * 1_000_000_000 / u128::from(SAMPLE_RATE);
                let time = symphonia::core::units::Time::try_from_nanos_u128(nanos)
                    .unwrap_or(symphonia::core::units::Time::MAX);
                let to = SeekTo::Time {
                    time,
                    track_id: Some(track_id),
                };
                if let Err(error) = format.seek(SeekMode::Coarse, to) {
                    eprintln!("audio seek failed at {time:?}: {error}");
                    continue;
                }
                decoder.reset();
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                state.buffer.clear();
                state.played_frames = target;
                state.ended = false;
                continue;
            }
            let full = {
                let state = state.lock().unwrap_or_else(|e| e.into_inner());
                state.buffer.len() / (CHANNELS as usize) >= BUFFER_CAPACITY_FRAMES
            };
            if full {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            match format.next_packet() {
                Ok(Some(packet)) => {
                    if packet.track_id != track_id {
                        continue;
                    }
                    match decoder.decode(&packet) {
                        Ok(decoded) => {
                            let mut chunk = Vec::new();
                            append_stereo_44100(&decoded, &mut chunk);
                            let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
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
                Ok(None) => {
                    state.lock().unwrap_or_else(|e| e.into_inner()).ended = true;
                    break; // expected end of stream
                }
                Err(symphonia::core::errors::Error::ResetRequired) => {
                    eprintln!("audio stream requires a decoder reset");
                    state.lock().unwrap_or_else(|e| e.into_inner()).ended = true;
                    break;
                }
                Err(error) => {
                    eprintln!("audio stream ended with error: {error}");
                    state.lock().unwrap_or_else(|e| e.into_inner()).ended = true;
                    break;
                }
            }
        }
        let _ = state.lock().unwrap_or_else(|e| e.into_inner()).ended;
    }

    fn spawn_producer(
        &mut self,
        path: PathBuf,
        state: Arc<Mutex<OutputState>>,
    ) -> Result<(), BackendError> {
        self.stop_producer();
        if self.retired_producers.len() >= MAX_RETIRED_PRODUCERS {
            return Err(BackendError::Io("previous audio reads are still blocked".into()));
        }
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
                // File reads/probes may be blocked on storage. The retired
                // worker owns only its old sample state, never the live slot.
                self.retired_producers.push(join);
            }
        }
        let mut index = 0;
        while index < self.retired_producers.len() {
            if self.retired_producers[index].is_finished() {
                let _ = self.retired_producers.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
    }

    fn stream(slot: OutputSlot, failed: Arc<AtomicBool>, master_volume: Arc<AtomicU32>) -> Result<Stream, BackendError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| BackendError::Device("no default output device".into()))?;
        let config = StreamConfig {
            channels: CHANNELS,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };
        let error = move |error| {
            eprintln!("audio output stream error: {error}");
            failed.store(true, Ordering::Release);
        };
        device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    let state = slot.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    let Some(state) = state else { data.fill(0.0); return; };
                    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                    let volume = f32::from_bits(master_volume.load(Ordering::Acquire));
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

    fn start_stream(&mut self) -> Result<(), BackendError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (commands_tx, commands_rx) = mpsc::channel();
        let master_volume = self.master_volume.clone();
        let slot = self.output_slot.clone();
        let failed = self.output_failed.clone();
        let thread = std::thread::Builder::new()
            .name("luno-audio-output".into())
            .spawn(move || match Self::stream(slot, failed, master_volume) {
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

fn append_stereo_44100(buffer: &GenericAudioBufferRef<'_>, output: &mut Vec<f32>) {
    let spec = buffer.spec();
    let channels = spec.channels().count().max(1);
    let rate = spec.rate();
    let frames = buffer.frames();
    let mut samples = vec![0.0f32; frames * channels];
    buffer.copy_to_vec_interleaved(&mut samples);
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
        self.load_streaming_with_duration(path, None)
    }

    /// Starts the producer immediately and lets THAT producer perform the one
    /// required container probe. `known_duration` seeds the UI when scan data
    /// has it; an unknown duration is filled by the producer after open.
    fn load_streaming_with_duration(&mut self, path: &Path, known_duration: Option<Duration>) -> Result<TrackInfo, BackendError> {
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "mp3" | "m4a" | "mp4" | "aac" | "flac" | "wav" | "webm") {
            return Err(BackendError::UnsupportedFormat(ext));
        }
        if let Some(state) = &self.state {
            state.lock().unwrap_or_else(|e| e.into_inner()).playing = false;
        }
        self.stop_producer();
        let duration = known_duration;
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
            duration,
            playing: false,
            ended: false,
            seek_to: None,
        }));
        self.spawn_producer(path.to_path_buf(), state.clone())?;
        *self.output_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(state.clone());
        self.state = Some(state);
        self.track = Some(info.clone());
        Ok(info)
    }

    fn set_volume(&mut self, volume: f32) {
        self.master_volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Release);
    }

    fn play(&mut self) -> Result<(), BackendError> {
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?.clone();
        let stream_failed = self.output_failed.swap(false, Ordering::AcqRel);
        if self.commands.is_none() || stream_failed {
            self.stop_stream();
            self.start_stream()?;
        }
        state.lock().unwrap_or_else(|e| e.into_inner()).playing = true;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), BackendError> {
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?;
        state.lock().unwrap_or_else(|e| e.into_inner()).playing = false;
        Ok(())
    }

    fn seek(&mut self, position: Duration) -> Result<(), BackendError> {
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?.clone();
        let frame = position.as_secs_f64().mul_add(SAMPLE_RATE as f64, 0.0) as u64;
        let ended = {
            let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
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
        let track = self.track.as_ref().ok_or(BackendError::NotLoaded)?;
        Ok(self.state.as_ref()
            .and_then(|state| state.lock().ok().and_then(|state| state.duration))
            .or(track.duration))
    }

    fn is_finished(&self) -> Option<bool> {
        self.state.as_ref().and_then(|state| state.lock().ok())
            .map(|state| state.ended && !state.playing && state.buffer.is_empty())
    }

    fn stop(&mut self) {
        if let Some(state) = &self.state {
            state.lock().unwrap_or_else(|e| e.into_inner()).playing = false;
        }
        self.stop_producer();
    }

    fn shutdown(&mut self) {
        self.stop();
        self.stop_stream();
        for producer in self.retired_producers.drain(..) {
            let _ = producer.join();
        }
    }
}

impl Drop for CpalSymphoniaBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a live macOS output device; run explicitly for transport timing"]
    fn native_output_transport_latency() {
        let mut backend = CpalSymphoniaBackend::new();
        backend.set_volume(0.0);
        let path = wav_fixture("native-latency");
        backend.load_streaming(&path).unwrap();
        backend.play().unwrap();
        let stream_id = backend.thread.as_ref().unwrap().thread().id();
        let mut slowest_switch = Duration::ZERO;
        let mut slowest_pause_resume = Duration::ZERO;
        for _ in 0..20 {
            let start = std::time::Instant::now();
            backend.load_streaming(&path).unwrap();
            backend.play().unwrap();
            while backend.position().unwrap() == Duration::ZERO && start.elapsed() < Duration::from_secs(1) {
                std::thread::sleep(Duration::from_millis(1));
            }
            let elapsed = start.elapsed();
            assert!(backend.position().unwrap() > Duration::ZERO, "native callback never consumed replacement audio");
            slowest_switch = slowest_switch.max(elapsed);
            assert_eq!(backend.thread.as_ref().unwrap().thread().id(), stream_id);
            let start = std::time::Instant::now();
            backend.pause().unwrap();
            let paused = backend.position().unwrap();
            std::thread::sleep(Duration::from_millis(20));
            assert_eq!(backend.position().unwrap(), paused, "paused callback consumed samples");
            backend.play().unwrap();
            while backend.position().unwrap() == paused && start.elapsed() < Duration::from_secs(1) {
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(backend.position().unwrap() > paused, "resume did not restart sample consumption");
            slowest_pause_resume = slowest_pause_resume.max(start.elapsed() - Duration::from_millis(20));
        }
        eprintln!("native output: 20 transitions; slowest load-to-sample {slowest_switch:?}; pause/resume-to-sample {slowest_pause_resume:?}");
        assert!(slowest_switch < Duration::from_millis(200));
        assert!(slowest_pause_resume < Duration::from_millis(100));
        // Simulate the device-error callback; the next play must recreate output.
        backend.output_failed.store(true, Ordering::Release);
        backend.play().unwrap();
        assert_ne!(backend.thread.as_ref().unwrap().thread().id(), stream_id);
        backend.shutdown();
        assert!(backend.thread.is_none());
        assert!(backend.retired_producers.is_empty());
    }

    #[test]
    fn retired_decoder_samples_cannot_reach_replacement_output() {
        let mut backend = CpalSymphoniaBackend::new();
        let path = wav_fixture("isolated-output");
        backend.load_streaming(&path).unwrap();
        let old = backend.state.clone().unwrap();
        backend.load_streaming(&path).unwrap();
        let selected = backend.output_slot.lock().unwrap().clone().unwrap();
        assert!(!Arc::ptr_eq(&old, &selected));
        assert!(Arc::ptr_eq(backend.state.as_ref().unwrap(), &selected));
        old.lock().unwrap().buffer.extend([99.0, 99.0]);
        assert!(!selected.lock().unwrap().buffer.contains(&99.0));
        assert!(!old.lock().unwrap().playing, "retired output must be silenced before replacement");
    }

    #[test]
    fn seeking_after_decoder_eof_preserves_play_intent() {
        let mut backend = CpalSymphoniaBackend::new();
        let path = wav_fixture("seek-eof");
        backend.load_streaming(&path).unwrap();
        backend.stop_producer();
        {
            let mut state = backend.state.as_ref().unwrap().lock().unwrap();
            state.ended = true;
            state.playing = true;
        }
        backend.seek(Duration::from_millis(50)).unwrap();
        assert!(backend.state.as_ref().unwrap().lock().unwrap().playing,
            "restarting a finished decoder must not pause the selected output");
    }

    #[test]
    fn blocked_decoder_retirement_is_bounded_and_shutdown_joins_workers() {
        let mut backend = CpalSymphoniaBackend::new();
        let path = wav_fixture("bounded-retirement");
        let mut releases = Vec::new();
        let exited = Arc::new(AtomicU32::new(0));
        for _ in 0..MAX_RETIRED_PRODUCERS {
            let (release, blocked) = mpsc::channel();
            releases.push(release);
            let exited = exited.clone();
            backend.retired_producers.push(std::thread::spawn(move || {
                blocked.recv().unwrap();
                exited.fetch_add(1, Ordering::Release);
            }));
        }
        let rejected = matches!(backend.load_streaming(&path), Err(BackendError::Io(_)));
        for release in releases { release.send(()).unwrap(); }
        backend.shutdown();
        assert!(rejected, "blocked storage must not create unbounded decoder threads");
        assert_eq!(exited.load(Ordering::Acquire), MAX_RETIRED_PRODUCERS as u32);
        assert!(backend.retired_producers.is_empty());
    }

    #[test]
    fn track_replacement_and_pause_do_not_wait_for_a_blocked_decoder() {
        let mut backend = CpalSymphoniaBackend::new();
        let path = wav_fixture("blocked-decoder");
        backend.load_streaming(&path).unwrap();
        backend.stop_producer();
        let (stop, _stop_rx) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        backend.producer = Some(ProducerHandle {
            stop,
            join: Some(std::thread::spawn(move || { blocked.recv().unwrap(); })),
        });
        let (done, result) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            backend.load_streaming(&path).unwrap();
            backend.pause().unwrap();
            done.send(()).unwrap();
            backend
        });
        let responsive = result.recv_timeout(Duration::from_millis(200)).is_ok();
        release.send(()).unwrap();
        drop(worker.join().unwrap());
        assert!(responsive, "replacement and pause waited for unrelated old decoder I/O");
    }

    #[test]
    fn track_replacement_retains_a_healthy_output_stream() {
        let mut backend = CpalSymphoniaBackend::new();
        let path = wav_fixture("retained-output");
        backend.load_streaming(&path).unwrap();
        // Stand in for the native stream lifetime without needing a device.
        let (commands, receiver) = mpsc::channel();
        backend.commands = Some(commands);
        backend.thread = Some(std::thread::spawn(move || { let _ = receiver.recv(); }));
        backend.output_failed.store(true, Ordering::Release);
        backend.load_streaming(&path).unwrap();
        assert!(backend.commands.is_some(), "track replacement tore down the output device");
        assert!(backend.output_failed.load(Ordering::Acquire), "a replacement must preserve pending device recovery");
        backend.stop();
        assert!(backend.commands.is_some(), "ordinary stop must preserve the reusable output device");
    }

    #[test]
    fn mp3_with_large_id3_tag_decodes_without_an_output_device() {
        let root = std::env::temp_dir().join(format!("luno-large-id3-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("track.mp3");
        // Embedded covers can put the first audio frame beyond the probe's
        // junk-search limit. Registered ID3 readers must consume the whole tag.
        let tag_size = 2 * 1024 * 1024u32;
        let mut bytes = b"ID3\x03\x00\x00".to_vec();
        for shift in [21, 14, 7, 0] {
            bytes.push(((tag_size >> shift) & 0x7f) as u8);
        }
        bytes.resize(10 + tag_size as usize, 0);
        // MPEG-1 Layer III, 128 kbps, 44.1 kHz stereo silent frames.
        for _ in 0..8 {
            bytes.extend_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
            bytes.resize(bytes.len() + 413, 0);
        }
        std::fs::write(&path, bytes).unwrap();
        let result = CpalSymphoniaBackend::open_track(&path);
        std::fs::remove_file(&path).unwrap();
        let (mut format, track_id, _) = result.expect("tagged MP3 must be recognized");
        let mut decoder = CpalSymphoniaBackend::make_decoder(format.as_ref(), track_id).unwrap();
        let mut decoded_packets = 0;
        while let Some(packet) = format.next_packet().unwrap() {
            if packet.track_id == track_id {
                decoder.decode(&packet).expect("MP3 audio must decode after the tag");
                decoded_packets += 1;
            }
        }
        assert!(decoded_packets > 0);
    }

    /// Deterministic 44.1 kHz stereo PCM WAV, valid enough for the Symphonia
    /// probe used by `open_track`.
    fn wav_fixture(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("luno-stream-{label}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let path = root.join("track.wav");
        let sample_rate = 44_100u32;
        let channels = 2u16;
        let bits_per_sample = 16u16;
        let frames = 44_100u32 * 2;
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
            f32::from_bits(backend.master_volume.load(Ordering::Acquire)),
            1.0,
            "a fresh backend starts at full volume"
        );
        backend.set_volume(0.3);
        // A transition reloads the output state; the user's volume must carry
        // over instead of snapping back to 1.0.
        backend.load_streaming(&path).expect("reload should swap the output state");
        let volume = f32::from_bits(backend.master_volume.load(Ordering::Acquire));
        assert!(
            (volume - 0.3).abs() < 1e-6,
            "master volume must survive a track reload, got {volume}"
        );
    }
}
