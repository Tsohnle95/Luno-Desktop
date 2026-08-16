//! cpal/symphonia implementation of the backend contract (macOS release target).

#![cfg(target_os = "macos")]

use crate::playback::backend::{
    AudioBackend, BackendError, OutputFormat, TrackInfo, CHANNELS, SAMPLE_RATE,
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Stream, StreamConfig,
};
use std::{
    fs::File,
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::DecoderOptions,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

#[derive(Default)]
pub struct CpalSymphoniaBackend {
    track: Option<TrackInfo>,
    state: Option<Arc<Mutex<OutputState>>>,
    commands: Option<mpsc::Sender<StreamCommand>>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct OutputState {
    samples: Vec<f32>,
    frame: usize,
    playing: bool,
}

enum StreamCommand {
    Stop,
}

impl CpalSymphoniaBackend {
    pub fn new() -> Self {
        Self::default()
    }
    fn decode(path: &Path) -> Result<(Vec<f32>, Option<Duration>), BackendError> {
        let file = File::open(path).map_err(|e| BackendError::Io(e.to_string()))?;
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
            hint.with_extension(ext);
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
            .ok_or_else(|| BackendError::Decode("file has no audio track".into()))?;
        let track_id = track.id;
        let duration = track.codec_params.time_base.and_then(|tb| {
            track.codec_params.n_frames.map(|frames| {
                let time = tb.calc_time(frames);
                Duration::from_secs_f64(time.seconds as f64 + time.frac)
            })
        });
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| BackendError::Decode(e.to_string()))?;
        let mut format = probed.format;
        let mut output = Vec::new();
        while let Ok(packet) = format.next_packet() {
            if packet.track_id() != track_id {
                continue;
            }
            let decoded = decoder
                .decode(&packet)
                .map_err(|e| BackendError::Decode(e.to_string()))?;
            append_stereo_44100(&decoded, &mut output);
        }
        Ok((output, duration))
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
                    for frame in data.chunks_mut(CHANNELS as usize) {
                        let sample = if state.playing
                            && state.frame < state.samples.len() / CHANNELS as usize
                        {
                            let offset = state.frame * CHANNELS as usize;
                            state.frame += 1;
                            [state.samples[offset], state.samples[offset + 1]]
                        } else {
                            state.playing = false;
                            [0.0, 0.0]
                        };
                        for (channel, value) in frame.iter_mut().enumerate() {
                            *value = sample[channel.min(1)];
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
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "mp3" | "m4a" | "mp4" | "flac" | "wav") {
            return Err(BackendError::UnsupportedFormat(ext));
        }
        let (samples, duration) = Self::decode(path)?;
        let info = TrackInfo {
            path: path.to_path_buf(),
            duration,
            format: OutputFormat {
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
            },
        };
        self.stop_stream();
        self.state = Some(Arc::new(Mutex::new(OutputState {
            samples,
            frame: 0,
            playing: false,
        })));
        self.track = Some(info.clone());
        Ok(info)
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
        let state = self.state.as_ref().ok_or(BackendError::NotLoaded)?;
        let frame = position.as_secs_f64().mul_add(SAMPLE_RATE as f64, 0.0) as usize;
        let mut state = state.lock().unwrap();
        state.frame = frame.min(state.samples.len() / CHANNELS as usize);
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
            state.frame as f64 / SAMPLE_RATE as f64,
        ))
    }
    fn duration(&self) -> Result<Option<Duration>, BackendError> {
        self.track
            .as_ref()
            .map(|t| t.duration)
            .ok_or(BackendError::NotLoaded)
    }

    fn stop(&mut self) {
        if let Some(state) = &self.state {
            state.lock().unwrap().playing = false;
        }
        self.stop_stream();
    }

    fn shutdown(&mut self) {
        self.stop();
    }
}

impl Drop for CpalSymphoniaBackend {
    fn drop(&mut self) {
        self.stop_stream();
    }
}
