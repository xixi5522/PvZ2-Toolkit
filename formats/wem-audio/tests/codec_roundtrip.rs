use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;
use wem_audio::{
    CodebookLibrary, PcmWemEncoder, VorbisOptions, VorbisWemDecoder, VorbisWemEncoder, WemCodec,
    WemDecodeOptions, decode_wem_to_wav, inspect_wem,
};

fn create_pcm_wav() -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(
            &mut output,
            hound::WavSpec {
                channels: 2,
                sample_rate: 22_050,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for frame in 0_i16..256 {
            writer.write_sample(frame.saturating_mul(64)).unwrap();
            writer.write_sample(frame.saturating_mul(-32)).unwrap();
        }
        writer.finalize().unwrap();
    }
    output.into_inner()
}

fn optional_sample(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

#[test]
fn pcm_wav_wem_wav_roundtrip() {
    let source_samples = {
        let source = create_pcm_wav();
        hound::WavReader::new(Cursor::new(source))
            .unwrap()
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let source_wav = create_pcm_wav();

    let mut wem = Vec::new();
    PcmWemEncoder::new(Cursor::new(source_wav))
        .unwrap()
        .encode(&mut wem)
        .unwrap();

    let metadata = inspect_wem(&mut Cursor::new(&wem)).unwrap();
    assert_eq!(metadata.codec, WemCodec::Pcm);
    assert_eq!(metadata.channels, 2);
    assert_eq!(metadata.sample_rate, 22_050);

    let mut decoded_wav = Cursor::new(Vec::new());
    decode_wem_to_wav(
        Cursor::new(wem),
        &mut decoded_wav,
        &WemDecodeOptions::new().without_vorbis_codebooks(),
    )
    .unwrap();
    decoded_wav.set_position(0);
    let decoded = hound::WavReader::new(decoded_wav)
        .unwrap()
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(decoded, source_samples);
}

#[test]
fn unsigned_8_bit_pcm_roundtrip_preserves_sample_values() {
    let samples = [-128_i8, -64, -1, 0, 1, 64, 127];
    let mut source_wav = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(
            &mut source_wav,
            hound::WavSpec {
                channels: 1,
                sample_rate: 8_000,
                bits_per_sample: 8,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    let mut wem = Vec::new();
    PcmWemEncoder::new(Cursor::new(source_wav.into_inner()))
        .unwrap()
        .encode(&mut wem)
        .unwrap();
    let metadata = inspect_wem(&mut Cursor::new(&wem)).unwrap();
    assert_eq!(metadata.bits_per_sample, 8);

    let mut decoded_wav = Cursor::new(Vec::new());
    decode_wem_to_wav(
        Cursor::new(wem),
        &mut decoded_wav,
        &WemDecodeOptions::new().without_vorbis_codebooks(),
    )
    .unwrap();
    decoded_wav.set_position(0);
    let decoded = hound::WavReader::new(decoded_wav)
        .unwrap()
        .samples::<i8>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(decoded, samples);
}

#[test]
fn decodes_real_wwise_vorbis_when_sample_is_configured() {
    let Some(path) = optional_sample("WEM_AUDIO_VORBIS_SAMPLE") else {
        return;
    };
    let codebooks = CodebookLibrary::aotuv_603();

    let metadata = inspect_wem(&mut File::open(&path).unwrap()).unwrap();
    assert_eq!(metadata.codec, WemCodec::Vorbis);

    let mut ogg = Vec::new();
    VorbisWemDecoder::with_options(
        File::open(&path).unwrap(),
        VorbisOptions::new().with_codebooks(codebooks.clone()),
    )
    .unwrap()
    .decode_to_ogg(&mut ogg)
    .unwrap();
    assert!(ogg.starts_with(b"OggS"));

    let mut repacked_wem = Vec::new();
    VorbisWemEncoder::new(Cursor::new(&ogg))
        .encode(&mut Cursor::new(&mut repacked_wem))
        .unwrap();
    assert_eq!(
        inspect_wem(&mut Cursor::new(&repacked_wem)).unwrap().codec,
        WemCodec::Vorbis
    );

    let mut wav = Cursor::new(Vec::new());
    decode_wem_to_wav(
        Cursor::new(repacked_wem),
        &mut wav,
        &WemDecodeOptions::new().with_vorbis_codebooks(codebooks),
    )
    .unwrap();
    wav.set_position(0);
    let reader = hound::WavReader::new(wav).unwrap();
    assert_eq!(reader.spec().channels, metadata.channels);
    assert_eq!(reader.spec().sample_rate, metadata.sample_rate);
    assert!(reader.len() > 0);
}

#[test]
fn decodes_real_wwise_adpcm_when_sample_is_configured() {
    let Some(path) = optional_sample("WEM_AUDIO_ADPCM_SAMPLE") else {
        return;
    };
    let metadata = inspect_wem(&mut File::open(&path).unwrap()).unwrap();
    assert_eq!(metadata.codec, WemCodec::ImaAdpcm);

    let mut wav = Cursor::new(Vec::new());
    decode_wem_to_wav(
        File::open(path).unwrap(),
        &mut wav,
        &WemDecodeOptions::new().without_vorbis_codebooks(),
    )
    .unwrap();
    wav.set_position(0);
    let reader = hound::WavReader::new(wav).unwrap();
    assert_eq!(reader.spec().channels, metadata.channels);
    assert_eq!(reader.spec().sample_rate, metadata.sample_rate);
    assert!(reader.len() > 0);
}
