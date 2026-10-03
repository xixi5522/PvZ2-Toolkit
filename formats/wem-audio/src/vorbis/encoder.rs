use crate::container::WWISE_FORMAT_VORBIS;
use crate::error::{WemError, WemResult};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use ogg::{Packet, PacketReader};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

/// Streams a standard Ogg Vorbis stream into a self-contained Wwise WEM.
pub struct VorbisWemEncoder<R: Read + Seek> {
    input: PacketReader<R>,
}

impl<R: Read + Seek> VorbisWemEncoder<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: PacketReader::new(input),
        }
    }

    pub fn encode<W: Write + Seek>(mut self, output: &mut W) -> WemResult<()> {
        let identification = self.next_required_packet("identification")?;
        let comment = self.next_required_packet("comment")?;
        let setup = self.next_required_packet("setup")?;
        let stream_serial = identification.stream_serial();
        if comment.stream_serial() != stream_serial || setup.stream_serial() != stream_serial {
            return Err(WemError::invalid_chunk(
                "Ogg",
                "Vorbis headers use different logical stream serials",
            ));
        }
        if !comment.data.starts_with(b"\x03vorbis") || !setup.data.starts_with(b"\x05vorbis") {
            return Err(WemError::invalid_chunk(
                "Ogg",
                "invalid Vorbis comment or setup packet",
            ));
        }
        let id = parse_identification_header(&identification.data)?;
        let setup_payload = setup
            .data
            .strip_prefix(b"\x05vorbis")
            .unwrap_or(&setup.data);
        let setup_size = u16::try_from(setup_payload.len())
            .map_err(|_| WemError::size_overflow("Vorbis setup packet"))?;

        output.write_all(b"RIFF")?;
        let riff_size_position = output.stream_position()?;
        output.write_u32::<LittleEndian>(0)?;
        output.write_all(b"WAVEfmt ")?;
        output.write_u32::<LittleEndian>(0x18)?;
        output.write_u16::<LittleEndian>(WWISE_FORMAT_VORBIS)?;
        output.write_u16::<LittleEndian>(id.channels)?;
        output.write_u32::<LittleEndian>(id.sample_rate)?;
        let average_bytes_position = output.stream_position()?;
        output.write_u32::<LittleEndian>(0)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_u16::<LittleEndian>(6)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_u32::<LittleEndian>(0)?;

        output.write_all(b"vorb")?;
        output.write_u32::<LittleEndian>(0x34)?;
        let sample_count_position = output.stream_position()?;
        output.write_u32::<LittleEndian>(0)?;
        for _ in 0..5 {
            output.write_u32::<LittleEndian>(0)?;
        }
        let setup_offset_position = output.stream_position()?;
        output.write_u32::<LittleEndian>(0)?;
        let first_audio_offset_position = output.stream_position()?;
        output.write_u32::<LittleEndian>(0)?;
        for _ in 0..3 {
            output.write_u32::<LittleEndian>(0)?;
        }
        output.write_u32::<LittleEndian>(0xABCDEF01)?;
        output.write_u8(id.blocksize_0)?;
        output.write_u8(id.blocksize_1)?;
        output.write_u16::<LittleEndian>(0)?;

        output.write_all(b"data")?;
        let data_size_position = output.stream_position()?;
        output.write_u32::<LittleEndian>(0)?;
        let data_start = output.stream_position()?;

        output.write_u16::<LittleEndian>(setup_size)?;
        output.write_u32::<LittleEndian>(0)?;
        output.write_all(setup_payload)?;
        let first_audio_offset = u32::try_from(output.stream_position()? - data_start)
            .map_err(|_| WemError::size_overflow("first audio packet offset"))?;

        let mut total_samples = 0_u32;
        while let Some(packet) = self
            .input
            .read_packet()
            .map_err(|error| WemError::ogg(format!("{error:?}")))?
        {
            if packet.stream_serial() != stream_serial {
                continue;
            }
            write_audio_packet(output, &packet)?;
            total_samples = u32::try_from(packet.absgp_page())
                .map_err(|_| WemError::size_overflow("Vorbis sample count"))?;
        }

        let end = output.stream_position()?;
        let data_size = u32::try_from(end - data_start)
            .map_err(|_| WemError::size_overflow("Vorbis WEM data chunk"))?;
        let riff_size = u32::try_from(
            end.checked_sub(8)
                .ok_or_else(|| WemError::size_overflow("RIFF size"))?,
        )
        .map_err(|_| WemError::size_overflow("Vorbis WEM RIFF"))?;
        let average_bytes_per_second = if total_samples == 0 {
            0
        } else {
            u32::try_from(
                u64::from(data_size)
                    .checked_mul(u64::from(id.sample_rate))
                    .ok_or_else(|| WemError::size_overflow("Vorbis byte rate"))?
                    / u64::from(total_samples),
            )
            .map_err(|_| WemError::size_overflow("Vorbis byte rate"))?
        };

        patch_u32(output, riff_size_position, riff_size)?;
        patch_u32(output, average_bytes_position, average_bytes_per_second)?;
        patch_u32(output, sample_count_position, total_samples)?;
        patch_u32(output, setup_offset_position, 0)?;
        patch_u32(output, first_audio_offset_position, first_audio_offset)?;
        patch_u32(output, data_size_position, data_size)?;
        output.seek(SeekFrom::Start(end))?;
        Ok(())
    }

    pub fn encode_to_vec(self) -> WemResult<Vec<u8>> {
        let mut output = Cursor::new(Vec::new());
        self.encode(&mut output)?;
        Ok(output.into_inner())
    }

    fn next_required_packet(&mut self, name: &'static str) -> WemResult<Packet> {
        self.input
            .read_packet()
            .map_err(|error| WemError::ogg(format!("{error:?}")))?
            .ok_or_else(|| {
                WemError::invalid_chunk("Ogg", format!("missing Vorbis {name} header packet"))
            })
    }
}

struct Identification {
    channels: u16,
    sample_rate: u32,
    blocksize_0: u8,
    blocksize_1: u8,
}

fn parse_identification_header(data: &[u8]) -> WemResult<Identification> {
    if data.len() < 30 || !data.starts_with(b"\x01vorbis") {
        return Err(WemError::invalid_chunk(
            "Ogg",
            "invalid Vorbis identification packet",
        ));
    }
    let mut input = Cursor::new(data);
    input.set_position(7);
    if input.read_u32::<LittleEndian>()? != 0 {
        return Err(WemError::unsupported_variant("Vorbis version"));
    }
    let channels = u16::from(input.read_u8()?);
    let sample_rate = input.read_u32::<LittleEndian>()?;
    input.set_position(28);
    let block_sizes = input.read_u8()?;
    let blocksize_0 = block_sizes & 0x0F;
    let blocksize_1 = block_sizes >> 4;
    let framing = input.read_u8()?;
    if channels == 0 || sample_rate == 0 {
        return Err(WemError::invalid_chunk(
            "Ogg",
            "Vorbis channel count and sample rate must be non-zero",
        ));
    }
    if !(6..=13).contains(&blocksize_0)
        || !(6..=13).contains(&blocksize_1)
        || blocksize_0 > blocksize_1
    {
        return Err(WemError::invalid_chunk(
            "Ogg",
            "invalid Vorbis block-size exponents",
        ));
    }
    if framing & 1 == 0 {
        return Err(WemError::invalid_chunk(
            "Ogg",
            "Vorbis identification framing bit is unset",
        ));
    }
    Ok(Identification {
        channels,
        sample_rate,
        blocksize_0,
        blocksize_1,
    })
}

fn write_audio_packet<W: Write>(output: &mut W, packet: &Packet) -> WemResult<()> {
    let size = u16::try_from(packet.data.len())
        .map_err(|_| WemError::size_overflow("Vorbis audio packet"))?;
    let granule = u32::try_from(packet.absgp_page())
        .map_err(|_| WemError::size_overflow("Vorbis granule position"))?;
    output.write_u16::<LittleEndian>(size)?;
    output.write_u32::<LittleEndian>(granule)?;
    output.write_all(&packet.data)?;
    Ok(())
}

fn patch_u32<W: Write + Seek>(output: &mut W, position: u64, value: u32) -> WemResult<()> {
    output.seek(SeekFrom::Start(position))?;
    output.write_u32::<LittleEndian>(value)?;
    Ok(())
}
