use crate::adpcm::AdpcmParams;
use crate::container::{WemCodec, WemReader};
use crate::error::{WemError, WemResult};
use crate::pcm::PcmParams;
use crate::{CodebookLibrary, VorbisOptions, VorbisWemDecoder};
#[cfg(target_arch = "wasm32")]
use std::io::Cursor;
#[cfg(not(target_arch = "wasm32"))]
use std::io::SeekFrom;
use std::io::{BufReader, Read, Seek, Write};
use symphonia::core::codecs::audio::{AudioCodecId, AudioDecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;

/// Options for the high-level WEM-to-WAV decoder.
#[derive(Clone)]
pub struct WemDecodeOptions {
    vorbis_codebooks: Option<CodebookLibrary>,
}

impl Default for WemDecodeOptions {
    fn default() -> Self {
        Self {
            vorbis_codebooks: Some(CodebookLibrary::standard()),
        }
    }
}

impl WemDecodeOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_vorbis_codebooks(mut self, codebooks: CodebookLibrary) -> Self {
        self.vorbis_codebooks = Some(codebooks);
        self
    }

    pub fn without_vorbis_codebooks(mut self) -> Self {
        self.vorbis_codebooks = None;
        self
    }
}

/// Decodes any supported WEM codec to a 16-bit integer WAV stream.
pub fn decode_wem_to_wav<R, W>(input: R, output: W, options: &WemDecodeOptions) -> WemResult<()>
where
    R: Read + Seek + Send + Sync + 'static,
    W: Write + Seek,
{
    let reader = WemReader::new(BufReader::new(input))?;
    let metadata = reader.metadata().clone();
    let data_size =
        u32::try_from(metadata.data_size).map_err(|_| WemError::size_overflow("WEM payload"))?;

    match metadata.codec {
        WemCodec::Vorbis => {
            let mut vorbis_options = VorbisOptions::new().without_codebooks();
            if let Some(codebooks) = &options.vorbis_codebooks {
                vorbis_options = vorbis_options.with_codebooks(codebooks.clone());
            }
            let mut decoder = VorbisWemDecoder::from_reader(reader, vorbis_options)?;

            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut ogg = tempfile::tempfile()?;
                decoder.decode_to_ogg(&mut ogg)?;
                ogg.seek(SeekFrom::Start(0))?;
                decode_ogg_to_wav(ogg, output)
            }
            #[cfg(target_arch = "wasm32")]
            {
                let mut ogg = Vec::new();
                decoder.decode_to_ogg(&mut ogg)?;
                decode_ogg_to_wav(Cursor::new(ogg), output)
            }
        }
        WemCodec::ImaAdpcm => crate::adpcm::process_adpcm(
            reader.into_inner(),
            output,
            AdpcmParams {
                channels: metadata.channels,
                sample_rate: metadata.sample_rate,
                block_align: metadata.block_align,
                is_little_endian: metadata.endian.is_little(),
                data_offset: metadata.data_offset,
                data_size,
            },
        ),
        WemCodec::Aac => crate::aac::decode_aac_to_wav(
            reader.into_inner(),
            output,
            metadata.data_offset,
            data_size,
            metadata.channels,
            metadata.sample_rate,
        ),
        WemCodec::Pcm | WemCodec::ExtensiblePcm => crate::pcm::process_pcm(
            reader.into_inner(),
            output,
            PcmParams {
                channels: metadata.channels,
                sample_rate: metadata.sample_rate,
                bits_per_sample: metadata.bits_per_sample,
                is_little_endian: metadata.endian.is_little(),
                data_offset: metadata.data_offset,
                data_size,
            },
        ),
        WemCodec::Unknown(format_tag) => Err(WemError::UnsupportedCodec { format_tag }),
    }
}

fn decode_ogg_to_wav<S, W>(input: S, output: W) -> WemResult<()>
where
    S: MediaSource + 'static,
    W: Write + Seek,
{
    let media = MediaSourceStream::new(Box::new(input), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            media,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| WemError::audio(format!("Vorbis probe failed: {error}")))?;
    let track = format
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .and_then(|params| params.audio())
                .is_some_and(|audio| audio.codec != AudioCodecId::default())
        })
        .ok_or_else(|| WemError::audio("Ogg stream has no Vorbis track"))?;
    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| WemError::audio("Vorbis audio parameters are unavailable"))?;
    let channels = u16::try_from(
        audio_params
            .channels
            .as_ref()
            .map(|channels| channels.count())
            .filter(|channels| *channels > 0)
            .ok_or_else(|| WemError::audio("Vorbis channel count is unavailable"))?,
    )
    .map_err(|_| WemError::size_overflow("Vorbis channel count"))?;
    let sample_rate = audio_params
        .sample_rate
        .filter(|sample_rate| *sample_rate > 0)
        .ok_or_else(|| WemError::audio("Vorbis sample rate is unavailable"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|error| WemError::audio(format!("Vorbis decoder failed: {error}")))?;
    let mut wav = hound::WavWriter::new(
        output,
        hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(symphonia::core::errors::Error::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => {
                return Err(WemError::audio(format!(
                    "Vorbis packet read failed: {error}"
                )));
            }
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(symphonia::core::errors::Error::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => {
                return Err(WemError::audio(format!("Vorbis decode failed: {error}")));
            }
        };
        let mut samples = Vec::<i16>::with_capacity(decoded.samples_interleaved());
        decoded.copy_to_vec_interleaved(&mut samples);
        for sample in samples {
            wav.write_sample(sample)?;
        }
    }
    wav.finalize()?;
    Ok(())
}
