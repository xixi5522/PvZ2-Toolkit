use crate::container::{WemCodec, WemReader};
use crate::error::{WemError, WemResult};
#[cfg(feature = "transcode")]
use std::io::SeekFrom;
use std::io::{Read, Seek, Write};
#[cfg(feature = "transcode")]
use symphonia::core::codecs::audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
#[cfg(feature = "transcode")]
use symphonia::core::formats::FormatOptions;
#[cfg(feature = "transcode")]
use symphonia::core::formats::probe::Hint;
#[cfg(feature = "transcode")]
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
#[cfg(feature = "transcode")]
use symphonia::core::meta::MetadataOptions;

pub fn extract_wem_aac<R: Read + Seek, W: Write>(input: R, output: W) -> WemResult<()> {
    let mut reader = WemReader::new(input)?;
    if reader.metadata().codec != WemCodec::Aac {
        return Err(WemError::UnsupportedCodec {
            format_tag: reader.metadata().codec.format_tag(),
        });
    }
    let data = reader
        .chunks()
        .data
        .ok_or_else(|| WemError::missing_chunk("data"))?;
    reader.copy_chunk_to(data, output)?;
    Ok(())
}

#[cfg(feature = "transcode")]
pub(crate) fn decode_aac_to_wav<R: Read + Seek + Send + Sync + 'static, W: Write + Seek>(
    mut input: R,
    output: W,
    data_offset: u64,
    data_size: u32,
    channels: u16,
    sample_rate: u32,
) -> WemResult<()> {
    input.seek(SeekFrom::Start(data_offset))?;

    let constrained_input = Box::new(ReadOnlySource::new(input.take(data_size as u64)));
    let mss = MediaSourceStream::new(constrained_input, Default::default());

    let mut hint = Hint::new();
    hint.with_extension("aac");
    hint.with_extension("m4a");

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| WemError::parse(format!("Symphonia probe error: {error}")))?;

    let tracks = format.tracks();
    let track = tracks
        .iter()
        .find(|t| {
            t.codec_params
                .as_ref()
                .and_then(|p| p.audio())
                .map(|a| a.codec != CODEC_ID_NULL_AUDIO)
                .unwrap_or(false)
        })
        .ok_or_else(|| WemError::parse("No supported audio track found"))?;

    let track_id = track.id;

    // Get audio params, filling in from WEM header if needed
    let track_audio = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| WemError::parse("No audio codec params"))?;

    let channels_count = track_audio
        .channels
        .as_ref()
        .map(|channels| {
            u16::try_from(channels.count())
                .map_err(|_| WemError::parse("AAC channel count exceeds u16"))
        })
        .transpose()?
        .unwrap_or(channels);
    let sample_rate_val = track_audio.sample_rate.unwrap_or(sample_rate);
    if channels_count == 0 {
        return Err(WemError::parse("AAC channel count is unavailable"));
    }
    if sample_rate_val == 0 {
        return Err(WemError::parse("AAC sample rate is unavailable"));
    }

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(track_audio, &AudioDecoderOptions::default())
        .map_err(|error| WemError::parse(format!("Symphonia codec error: {error}")))?;

    let spec = hound::WavSpec {
        channels: channels_count,
        sample_rate: sample_rate_val,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav_writer = hound::WavWriter::new(output, spec).map_err(WemError::Wav)?;

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
                return Err(WemError::parse(format!("AAC packet read error: {error}")));
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
            Err(error) => return Err(WemError::parse(format!("AAC decode error: {error}"))),
        };
        let mut samples = Vec::<i16>::with_capacity(decoded.samples_interleaved());
        decoded.copy_to_vec_interleaved(&mut samples);
        for sample in samples {
            wav_writer.write_sample(sample).map_err(WemError::Wav)?;
        }
    }

    wav_writer.finalize().map_err(WemError::Wav)?;

    Ok(())
}
