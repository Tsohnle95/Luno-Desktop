//! Symphonia bridge for the pure-Rust Opus decoder used by the macOS player.

use std::fmt;

use ruopus::OpusDecoder as RuOpusDecoder;
use symphonia::core::{
    audio::{
        layouts, AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, Channels,
        GenericAudioBufferRef,
    },
    codecs::{
        audio::{
            well_known::CODEC_ID_OPUS, AudioCodecParameters, AudioDecoder, AudioDecoderOptions,
        },
        registry::{RegisterableAudioDecoder, SupportedAudioCodec},
        CodecInfo,
    },
    errors::{unsupported_error, Error, Result},
    packet::PacketRef,
};

const OUTPUT_SAMPLE_RATE: u32 = 48_000;
const MAX_SAMPLES_PER_CHANNEL: usize = OUTPUT_SAMPLE_RATE as usize * 120 / 1_000;

pub struct OpusAdapter {
    params: AudioCodecParameters,
    decoder: RuOpusDecoder,
    buffer: AudioBuffer<f32>,
    sample_count: usize,
    channels: usize,
    channel_layout: Channels,
    pre_skip: usize,
}

impl fmt::Debug for OpusAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpusAdapter")
            .field("params", &self.params)
            .field("buffer", &"<audio buffer>")
            .field("sample_count", &self.sample_count)
            .field("channels", &self.channels)
            .field("pre_skip", &self.pre_skip)
            .finish_non_exhaustive()
    }
}

impl OpusAdapter {
    fn try_new(params: &AudioCodecParameters, _opts: &AudioDecoderOptions) -> Result<Self> {
        let channels = params
            .channels
            .as_ref()
            .map(Channels::count)
            .ok_or(Error::Unsupported("opus: channel layout is required"))?;
        let channel_layout = match channels {
            1 => layouts::CHANNEL_LAYOUT_MONO,
            2 => layouts::CHANNEL_LAYOUT_STEREO,
            _ => return unsupported_error("opus: only mono and stereo streams are supported"),
        };

        let pre_skip = params
            .extra_data
            .as_deref()
            .map(|data| parse_opus_head(data, channels))
            .transpose()?
            .unwrap_or_default();
        let mut params = params.clone();
        params.sample_rate = Some(OUTPUT_SAMPLE_RATE);

        Ok(Self {
            params,
            decoder: RuOpusDecoder::new(channels),
            buffer: make_buffer(&channel_layout, 960),
            sample_count: 960,
            channels,
            channel_layout,
            pre_skip,
        })
    }
}

fn parse_opus_head(data: &[u8], channels: usize) -> Result<usize> {
    if data.len() < 19 || &data[..8] != b"OpusHead" || usize::from(data[9]) != channels {
        return Err(Error::DecodeError("opus: invalid OpusHead codec data"));
    }
    if data[18] != 0 {
        return unsupported_error("opus: mapped multi-stream layouts are not supported");
    }

    Ok(usize::from(u16::from_le_bytes([data[10], data[11]])))
}

fn make_buffer(channels: &Channels, capacity: usize) -> AudioBuffer<f32> {
    AudioBuffer::new(AudioSpec::new(OUTPUT_SAMPLE_RATE, channels.clone()), capacity)
}

impl AudioDecoder for OpusAdapter {
    fn reset(&mut self) {
        self.decoder = RuOpusDecoder::new(self.channels);
        self.buffer.clear();
        self.pre_skip = 0;
    }

    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs()[0].info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        let pcm = match self.decoder.decode_packet(packet.data) {
            Ok(pcm) => pcm,
            Err(_) => {
                self.buffer.clear();
                return Err(Error::DecodeError("opus: invalid audio packet"));
            }
        };
        let sample_count = pcm.len() / self.channels;
        if sample_count > MAX_SAMPLES_PER_CHANNEL {
            self.buffer.clear();
            return Err(Error::LimitError("opus: packet decoded beyond the maximum frame size"));
        }
        if sample_count != self.sample_count {
            self.buffer = make_buffer(&self.channel_layout, sample_count);
            self.sample_count = sample_count;
        }

        self.buffer.clear();
        self.buffer.render_uninit(None);
        self.buffer.copy_from_slice_interleaved(&pcm);
        let skipped = self.pre_skip.min(sample_count);
        self.pre_skip -= skipped;
        let trim_start = packet.trim_start.get() as usize + skipped;
        let trim_end = packet.trim_end.get() as usize;
        self.buffer.trim(
            trim_start.min(sample_count),
            trim_end.min(sample_count.saturating_sub(trim_start)),
        );
        Ok(self.buffer.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> symphonia::core::codecs::audio::FinalizeResult {
        symphonia::core::codecs::audio::FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buffer.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for OpusAdapter {
    fn try_registry_new(
        params: &AudioCodecParameters,
        opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        Ok(Box::new(Self::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        static SUPPORTED_CODECS: [SupportedAudioCodec; 1] = [SupportedAudioCodec {
            id: CODEC_ID_OPUS,
            info: CodecInfo {
                short_name: "opus",
                long_name: "Opus",
                profiles: &[],
            },
        }];
        &SUPPORTED_CODECS
    }
}
