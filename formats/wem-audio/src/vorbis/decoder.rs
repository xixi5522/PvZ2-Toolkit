use super::codebook::CodebookLibrary;
use super::options::{PacketFormat, SetupFormat, VorbisOptions};
use crate::bit_stream::BitReader;
use crate::bit_stream::BitWriter;
use crate::container::{
    RiffEndian, WemChunk, WemChunks, WemCodec, WemMetadata, WemReader, read_chunk_exact_from,
};
use crate::error::{WemError, WemResult};
use crate::vorbis::helpers::ilog;
use crate::vorbis::packet::{Packet, Packet8};
use ogg::{PacketWriteEndInfo, PacketWriter};
use std::io::{Read, Seek, SeekFrom, Write};

const VORBIS_BYTES: &[u8] = b"vorbis";
/// Converts a Wwise Vorbis WEM stream to standard Ogg Vorbis.
pub struct VorbisWemDecoder<R: Read + Seek> {
    input: R,
    codebooks: Option<CodebookLibrary>,
    inline_codebooks: bool,
    setup_format: SetupFormat,
    little_endian: bool,
    metadata: WemMetadata,
    chunks: WemChunks,

    // RIFF fmt
    channels: u16,
    sample_rate: u32,
    avg_bytes_per_second: u32,
    ext_unk: u16,
    // Cue info
    cue_count: u32,

    // Smpl info
    loop_count: u32,
    loop_start: u32,
    loop_end: u32,

    // Vorbis info
    sample_count: u32,
    uid: u32,
    blocksize_0_pow: u8,
    blocksize_1_pow: u8,
    setup_packet_offset: u32,
    first_audio_packet_offset: u32,

    // Flags
    no_granule: bool,
    mod_packets: bool,
    header_triad_present: bool,
    old_packet_headers: bool,
}

impl<R: Read + Seek> VorbisWemDecoder<R> {
    pub fn new(input: R) -> WemResult<Self> {
        Self::with_options(input, VorbisOptions::default())
    }

    pub fn with_options(input: R, options: VorbisOptions) -> WemResult<Self> {
        Self::from_reader(WemReader::new(input)?, options)
    }

    pub fn from_reader(reader: WemReader<R>, options: VorbisOptions) -> WemResult<Self> {
        if reader.metadata().codec != WemCodec::Vorbis {
            return Err(WemError::UnsupportedCodec {
                format_tag: reader.metadata().codec.format_tag(),
            });
        }
        let (input, metadata, chunks) = reader.into_parts();
        let mut converter = Self {
            input,
            codebooks: options.codebooks,
            inline_codebooks: options.inline_codebooks,
            setup_format: options.setup_format,
            little_endian: metadata.endian == RiffEndian::Little,
            channels: metadata.channels,
            sample_rate: metadata.sample_rate,
            avg_bytes_per_second: metadata.average_bytes_per_second,
            metadata,
            chunks,
            ext_unk: 0,
            cue_count: 0,
            loop_count: 0,
            loop_start: 0,
            loop_end: 0,
            sample_count: 0,
            uid: 0,
            blocksize_0_pow: 0,
            blocksize_1_pow: 0,
            setup_packet_offset: 0,
            first_audio_packet_offset: 0,
            no_granule: false,
            mod_packets: false,
            header_triad_present: false,
            old_packet_headers: false,
        };

        converter.parse_fmt_chunk()?;
        converter.parse_cue_chunk()?;
        converter.parse_smpl_chunk()?;
        converter.parse_vorb_chunk(options.packet_format)?;

        if converter.metadata.is_prefetch() && converter.metadata.declared_data_size != 0 {
            let scaled = u64::from(converter.sample_count)
                .checked_mul(converter.metadata.data_size)
                .ok_or_else(|| WemError::size_overflow("prefetch sample count"))?
                / u64::from(converter.metadata.declared_data_size);
            converter.sample_count = u32::try_from(scaled)
                .map_err(|_| WemError::size_overflow("prefetch sample count"))?;
        }

        converter.validate_loops()?;
        Ok(converter)
    }

    // Helper methods for reading values
    fn read_u32_static(input: &mut R, little_endian: bool) -> WemResult<u32> {
        let mut buf = [0u8; 4];
        input.read_exact(&mut buf)?;
        Ok(if little_endian {
            u32::from_le_bytes(buf)
        } else {
            u32::from_be_bytes(buf)
        })
    }

    fn read_u32(&mut self) -> WemResult<u32> {
        Self::read_u32_static(&mut self.input, self.little_endian)
    }

    fn read_u16(&mut self) -> WemResult<u16> {
        let mut buf = [0u8; 2];
        self.input.read_exact(&mut buf)?;
        Ok(if self.little_endian {
            u16::from_le_bytes(buf)
        } else {
            u16::from_be_bytes(buf)
        })
    }

    fn read_byte(&mut self) -> WemResult<u8> {
        let mut buf = [0u8; 1];
        self.input.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn parse_fmt_chunk(&mut self) -> WemResult<()> {
        let fmt = self
            .chunks
            .fmt
            .ok_or_else(|| WemError::missing_chunk("fmt "))?;
        let fmt_size = fmt.size;

        if self.chunks.vorb.is_none() && fmt_size != 0x42 {
            return Err(WemError::invalid_chunk(
                "fmt ",
                "expected a 0x42-byte fmt chunk when vorb is embedded",
            ));
        }

        if self.chunks.vorb.is_some() && fmt_size != 0x28 && fmt_size != 0x18 && fmt_size != 0x12 {
            return Err(WemError::invalid_chunk(
                "fmt ",
                format!("unsupported Vorbis fmt size 0x{fmt_size:X}"),
            ));
        }

        // If vorb chunk is missing but fmt is 0x42, vorb data is embedded in fmt
        if self.chunks.vorb.is_none() && fmt_size == 0x42 {
            self.chunks.vorb = Some(WemChunk::new(
                *b"vorb",
                fmt.offset + 0x18,
                fmt_size - 0x18,
                u32::try_from(fmt_size - 0x18)
                    .map_err(|_| WemError::size_overflow("embedded vorb chunk"))?,
            ));
        }

        self.input.seek(SeekFrom::Start(fmt.offset))?;

        if self.read_u16()? != crate::container::WWISE_FORMAT_VORBIS {
            return Err(WemError::invalid_chunk("fmt ", "codec is not Wwise Vorbis"));
        }

        let _channels = self.read_u16()?;
        let _sample_rate = self.read_u32()?;
        let _average_bytes_per_second = self.read_u32()?;

        if self.read_u16()? != 0 {
            return Err(WemError::invalid_chunk(
                "fmt ",
                "Vorbis block alignment must be zero",
            ));
        }

        if self.read_u16()? != 0 {
            return Err(WemError::invalid_chunk(
                "fmt ",
                "Vorbis bits per sample must be zero",
            ));
        }

        if self.read_u16()? != (fmt_size - 0x12) as u16 {
            return Err(WemError::invalid_chunk(
                "fmt ",
                "extension length does not match chunk size",
            ));
        }

        if fmt_size - 0x12 >= 2 {
            self.ext_unk = self.read_u16()?;

            if fmt_size - 0x12 >= 6 {
                let _channel_layout = self.read_u32()?;
            }
        }

        if fmt_size == 0x28 {
            let mut unknown = [0u8; 16];
            let expected: [u8; 16] = [
                1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9b, 0x71,
            ];
            self.input.read_exact(&mut unknown)?;

            if unknown != expected {
                return Err(WemError::invalid_chunk(
                    "fmt ",
                    "unexpected WAVEFORMATEXTENSIBLE signature",
                ));
            }
        }

        Ok(())
    }

    fn parse_cue_chunk(&mut self) -> WemResult<()> {
        if let Some(cue) = self.chunks.cue {
            let mut bytes = [0_u8; 4];
            read_chunk_exact_from(&mut self.input, cue, 0, &mut bytes)?;
            self.cue_count = self.read_endian_u32(bytes);
        }
        Ok(())
    }

    fn parse_smpl_chunk(&mut self) -> WemResult<()> {
        if let Some(smpl) = self.chunks.smpl {
            let mut bytes = [0_u8; 0x34];
            read_chunk_exact_from(&mut self.input, smpl, 0, &mut bytes)?;
            self.loop_count =
                self.read_endian_u32([bytes[0x1C], bytes[0x1D], bytes[0x1E], bytes[0x1F]]);

            if self.loop_count != 1 {
                return Err(WemError::unsupported_variant(
                    "sample chunks with a loop count other than one",
                ));
            }

            self.loop_start =
                self.read_endian_u32([bytes[0x2C], bytes[0x2D], bytes[0x2E], bytes[0x2F]]);
            self.loop_end =
                self.read_endian_u32([bytes[0x30], bytes[0x31], bytes[0x32], bytes[0x33]]);
        }
        Ok(())
    }

    fn parse_vorb_chunk(&mut self, packet_format: PacketFormat) -> WemResult<()> {
        let vorb = self
            .chunks
            .vorb
            .ok_or_else(|| WemError::missing_chunk("vorb"))?;
        let vorb_size = vorb.size;

        match vorb_size {
            0x28 | 0x2A | 0x2C | 0x32 | 0x34 => {
                self.input.seek(SeekFrom::Start(vorb.offset))?;
            }
            _ => {
                return Err(WemError::invalid_chunk(
                    "vorb",
                    format!("unsupported size 0x{vorb_size:X}"),
                ));
            }
        }

        self.sample_count = self.read_u32()?;

        match vorb_size {
            0x2A => {
                self.no_granule = true;
                self.input.seek(SeekFrom::Start(vorb.offset + 0x4))?;
                let mod_signal = self.read_u32()?;

                if mod_signal != 0x4A
                    && mod_signal != 0x4B
                    && mod_signal != 0x69
                    && mod_signal != 0x70
                {
                    self.mod_packets = true;
                }

                self.input.seek(SeekFrom::Start(vorb.offset + 0x10))?;
            }
            _ => {
                self.input.seek(SeekFrom::Start(vorb.offset + 0x18))?;
            }
        }

        match packet_format {
            PacketFormat::Auto => {}
            PacketFormat::Modified => self.mod_packets = true,
            PacketFormat::Standard => self.mod_packets = false,
        }

        self.setup_packet_offset = self.read_u32()?;
        self.first_audio_packet_offset = self.read_u32()?;

        match vorb_size {
            0x2A => {
                self.input.seek(SeekFrom::Start(vorb.offset + 0x24))?;
            }
            0x32 | 0x34 => {
                self.input.seek(SeekFrom::Start(vorb.offset + 0x2C))?;
            }
            _ => {}
        }

        match vorb_size {
            0x28 | 0x2C => {
                self.header_triad_present = true;
                self.old_packet_headers = true;
            }
            0x2A | 0x32 | 0x34 => {
                self.uid = self.read_u32()?;
                self.blocksize_0_pow = self.read_byte()?;
                self.blocksize_1_pow = self.read_byte()?;
            }
            _ => {}
        }

        let data = self
            .chunks
            .data
            .ok_or_else(|| WemError::missing_chunk("data"))?;
        if u64::from(self.setup_packet_offset) >= data.size
            || u64::from(self.first_audio_packet_offset) > data.size
            || self.setup_packet_offset >= self.first_audio_packet_offset
        {
            return Err(WemError::invalid_chunk(
                "vorb",
                "packet offsets are outside the data chunk or out of order",
            ));
        }
        if !self.header_triad_present
            && (!(6..=13).contains(&self.blocksize_0_pow)
                || !(6..=13).contains(&self.blocksize_1_pow)
                || self.blocksize_0_pow > self.blocksize_1_pow)
        {
            return Err(WemError::invalid_field(
                "Vorbis block sizes",
                (u64::from(self.blocksize_1_pow) << 8) | u64::from(self.blocksize_0_pow),
                "exponents must be in 6..=13 and ordered from small to large",
            ));
        }

        Ok(())
    }

    fn validate_loops(&mut self) -> WemResult<()> {
        if self.loop_count != 0 {
            if self.loop_end == 0 {
                self.loop_end = self.sample_count;
            } else {
                self.loop_end = self
                    .loop_end
                    .checked_add(1)
                    .ok_or_else(|| WemError::size_overflow("loop end"))?;
            }

            if self.loop_start >= self.sample_count
                || self.loop_end > self.sample_count
                || self.loop_start > self.loop_end
            {
                return Err(WemError::parse("loops out of range"));
            }
        }
        Ok(())
    }

    fn read_endian_u32(&self, bytes: [u8; 4]) -> u32 {
        if self.little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    }

    /// Generates a standard Ogg Vorbis stream from the parsed Wwise audio data.
    ///
    /// This method performs the actual conversion, writing a complete Ogg Vorbis
    /// stream to the provided output. The output includes:
    ///
    /// 1. **Identification header** - Vorbis version, channels, sample rate, etc.
    /// 2. **Comment header** - Vendor string identifying this converter
    /// 3. **Setup header** - Codebooks, floor/residue/mapping configuration
    /// 4. **Audio packets** - Converted audio data with proper granule positions
    ///
    /// # Arguments
    ///
    /// * `output` - Any type implementing [`Write`], such as a file or buffer
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The audio data is corrupted or truncated
    /// - A referenced codebook ID is not found in the library
    /// - The codebook data doesn't match (wrong codebook library)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use std::io::{BufReader, BufWriter};
    /// use wem_audio::VorbisWemDecoder;
    ///
    /// # fn main() -> Result<(), wem_audio::WemError> {
    /// let input = BufReader::new(File::open("input.wem")?);
    /// let mut decoder = VorbisWemDecoder::new(input)?;
    ///
    /// // Write to a file
    /// let output = BufWriter::new(File::create("output.ogg")?);
    /// decoder.decode_to_ogg(output)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// You can also write to a `Vec<u8>` for in-memory processing:
    ///
    /// ```no_run
    /// use std::io::Cursor;
    /// use wem_audio::VorbisWemDecoder;
    ///
    /// # fn convert(wem_data: &[u8]) -> Result<Vec<u8>, wem_audio::WemError> {
    /// let input = Cursor::new(wem_data);
    /// let mut decoder = VorbisWemDecoder::new(input)?;
    ///
    /// let mut ogg_data = Vec::new();
    /// decoder.decode_to_ogg(&mut ogg_data)?;
    /// # Ok(ogg_data)
    /// # }
    /// ```
    pub fn decode_to_ogg<W: Write>(&mut self, output: W) -> WemResult<()> {
        let data = self
            .chunks
            .data
            .ok_or_else(|| WemError::missing_chunk("data"))?;
        let data_offset = data.offset;
        let data_size = data.size;
        // data.size is already clamped to file_size in read_chunks if prefetch
        let data_end = data_offset + data_size;

        let mut packet_writer = PacketWriter::new(output);
        let serial = 0x80000001; // Random serial or fixed

        let mut mode_bits = 0i32;

        let mut prev_blockflag = false;

        let mode_blockflag = if self.header_triad_present {
            let expected_types = [1_u8, 3, 5];
            let mut offset = data_offset
                .checked_add(u64::from(self.setup_packet_offset))
                .ok_or_else(|| WemError::size_overflow("header triad offset"))?;
            for (index, expected_type) in expected_types.into_iter().enumerate() {
                let packet = Packet8::read(&mut self.input, offset, self.little_endian)?;
                if packet.next_offset > data_end {
                    return Err(WemError::invalid_chunk(
                        "data",
                        "Vorbis header triad packet is truncated",
                    ));
                }
                if packet.granule != 0 {
                    return Err(WemError::invalid_chunk(
                        "data",
                        "Vorbis header triad granule must be zero",
                    ));
                }
                let packet_size = usize::try_from(packet.size)
                    .map_err(|_| WemError::size_overflow("Vorbis header packet"))?;
                let mut payload = vec![0_u8; packet_size];
                self.input.seek(SeekFrom::Start(packet.offset))?;
                self.input.read_exact(&mut payload)?;
                if payload.first().copied() != Some(expected_type)
                    || payload.get(1..7) != Some(VORBIS_BYTES)
                {
                    return Err(WemError::invalid_chunk(
                        "data",
                        format!("invalid Vorbis header packet at triad index {index}"),
                    ));
                }
                packet_writer.write_packet(payload, serial, PacketWriteEndInfo::EndPage, 0)?;
                offset = packet.next_offset;
            }
            let expected_audio_offset = data_offset
                .checked_add(u64::from(self.first_audio_packet_offset))
                .ok_or_else(|| WemError::size_overflow("first audio packet offset"))?;
            if offset != expected_audio_offset {
                return Err(WemError::invalid_chunk(
                    "vorb",
                    "header triad does not end at the first audio packet offset",
                ));
            }
            None
        } else {
            let id_data = self.generate_identification_packet()?;
            packet_writer.write_packet(id_data, serial, PacketWriteEndInfo::EndPage, 0)?;

            let comment_data = self.generate_comment_packet()?;
            packet_writer.write_packet(comment_data, serial, PacketWriteEndInfo::EndPage, 0)?;

            let (setup_data, mb_flag) = self.generate_setup_packet()?;
            packet_writer.write_packet(setup_data, serial, PacketWriteEndInfo::EndPage, 0)?;

            if !mb_flag.is_empty() {
                mode_bits = ilog(mb_flag.len() as u32 - 1) as i32;
            }
            Some(mb_flag)
        };

        // For granule calculation
        let (blocksize_0, blocksize_1) = if self.no_granule {
            (
                1_u32
                    .checked_shl(u32::from(self.blocksize_0_pow))
                    .ok_or_else(|| {
                        WemError::invalid_field(
                            "blocksize_0_pow",
                            u64::from(self.blocksize_0_pow),
                            "shift exceeds u32",
                        )
                    })?,
                1_u32
                    .checked_shl(u32::from(self.blocksize_1_pow))
                    .ok_or_else(|| {
                        WemError::invalid_field(
                            "blocksize_1_pow",
                            u64::from(self.blocksize_1_pow),
                            "shift exceeds u32",
                        )
                    })?,
            )
        } else {
            (0, 0)
        };
        let mut granule_pos: i64 = 0;
        let mut prev_blocksize: u32 = 0;
        let mut first_packet = true;

        // Audio pages
        let mut offset = data_offset + self.first_audio_packet_offset as u64;

        while offset < data_end {
            let (packet_header_size, size, packet_payload_offset, granule, next_offset) =
                if self.old_packet_headers {
                    let packet = Packet8::read(&mut self.input, offset, self.little_endian)?;
                    (
                        packet.header_size,
                        packet.size,
                        packet.offset,
                        packet.granule,
                        packet.next_offset,
                    )
                } else {
                    let packet =
                        Packet::read(&mut self.input, offset, self.little_endian, self.no_granule)?;
                    (
                        packet.header_size,
                        packet.size,
                        packet.offset,
                        packet.granule,
                        packet.next_offset,
                    )
                };

            if offset
                .checked_add(packet_header_size)
                .is_none_or(|packet_header_end| packet_header_end > data_end)
                || next_offset > data_end
            {
                return Err(WemError::parse("page header truncated"));
            }

            offset = packet_payload_offset;
            self.input.seek(SeekFrom::Start(offset))?;

            let current_granule: u64;

            // Determine granule for this page
            let is_last_packet = next_offset >= data_end;

            if self.no_granule {
                // Calculate granule from block sizes
                let curr_blocksize = if let Some(ref mbf) = mode_blockflag {
                    if mode_bits > 0 && size > 0 {
                        // Read mode number from first byte
                        let first_byte = self.read_byte()?;
                        self.input.seek(SeekFrom::Start(offset))?; // Seek back

                        let mode_number = if self.mod_packets {
                            (first_byte as u32) & ((1 << mode_bits) - 1)
                        } else {
                            ((first_byte as u32) >> 1) & ((1 << mode_bits) - 1)
                        };

                        let blockflag = mbf
                            .get(mode_number as usize)
                            .copied()
                            .ok_or_else(|| WemError::parse("invalid Vorbis mode number"))?;
                        if blockflag { blocksize_1 } else { blocksize_0 }
                    } else {
                        blocksize_0
                    }
                } else {
                    blocksize_0
                };

                // Calculate samples for this packet
                if first_packet {
                    first_packet = false;
                } else {
                    granule_pos += ((prev_blocksize + curr_blocksize) / 4) as i64;
                }

                prev_blocksize = curr_blocksize;

                if is_last_packet && self.sample_count > 0 {
                    current_granule = self.sample_count as u64;
                } else {
                    current_granule = granule_pos as u64;
                }
            } else {
                // Use granule from packet
                current_granule = if granule == 0xFFFFFFFF {
                    1
                } else {
                    granule as u64
                };
            }

            // Packet Data Rebuilding
            let mut packet_data = BitWriter::new();

            // First byte handling
            if self.mod_packets {
                let mbf = mode_blockflag
                    .as_ref()
                    .ok_or_else(|| WemError::parse("didn't load mode_blockflag"))?;

                // OUT: 1 bit packet type (0 == audio)
                packet_data.write_bits(0, 1);

                self.input.seek(SeekFrom::Start(offset))?;
                let mut bit_reader = BitReader::new(&mut self.input);

                // IN/OUT: N bit mode number
                let mode_number = bit_reader.read_bits(mode_bits as u8)?;
                packet_data.write_bits(mode_number, mode_bits as u8);
                let current_blockflag = mbf
                    .get(mode_number as usize)
                    .copied()
                    .ok_or_else(|| WemError::parse("invalid Vorbis mode number"))?;

                // IN: remaining bits of first byte
                let remainder = bit_reader.read_bits((8 - mode_bits) as u8)?;

                if current_blockflag {
                    // Long window, peek at next frame
                    let next_blockflag = if next_offset + packet_header_size <= data_end {
                        let next_packet = Packet::read(
                            &mut self.input,
                            next_offset,
                            self.little_endian,
                            self.no_granule,
                        )?;
                        if next_packet.size > 0 {
                            self.input.seek(SeekFrom::Start(next_packet.offset))?;
                            let mut next_bit_reader = BitReader::new(&mut self.input);
                            let next_mode_number = next_bit_reader.read_bits(mode_bits as u8)?;
                            mbf.get(next_mode_number as usize)
                                .copied()
                                .ok_or_else(|| WemError::parse("invalid next Vorbis mode number"))?
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // OUT: previous/next window type bits
                    packet_data.write_bits(if prev_blockflag { 1 } else { 0 }, 1);
                    packet_data.write_bits(if next_blockflag { 1 } else { 0 }, 1);

                    self.input.seek(SeekFrom::Start(offset + 1))?;
                }

                prev_blockflag = current_blockflag;
                packet_data.write_bits(remainder, (8 - mode_bits) as u8);
            } else {
                let v = self.read_byte()?;
                packet_data.write_bits(v as u32, 8);
            }

            // Remainder of packet
            for _ in 1..size {
                let v = self.read_byte()?;
                packet_data.write_bits(v as u32, 8);
            }

            offset = next_offset;

            let end_info = if offset == data_end {
                PacketWriteEndInfo::EndStream
            } else {
                PacketWriteEndInfo::EndPage
            };

            packet_writer.write_packet(
                packet_data.into_inner(),
                serial,
                end_info,
                current_granule,
            )?;
        }

        if offset > data_end {
            return Err(WemError::parse("page truncated"));
        }

        Ok(())
    }

    fn write_vorbis_packet_header(&self, writer: &mut BitWriter, packet_type: u8) {
        writer.write_bits(packet_type as u32, 8);
        for b in VORBIS_BYTES {
            writer.write_bits(*b as u32, 8);
        }
    }

    fn generate_identification_packet(&mut self) -> WemResult<Vec<u8>> {
        let mut writer = BitWriter::new();
        self.write_vorbis_packet_header(&mut writer, 1);
        writer.write_bits(0, 32); // version
        writer.write_bits(self.channels as u32, 8);
        writer.write_bits(self.sample_rate, 32);
        writer.write_bits(0, 32); // bitrate_max
        let nominal_bitrate = self
            .avg_bytes_per_second
            .checked_mul(8)
            .ok_or_else(|| WemError::size_overflow("Vorbis nominal bitrate"))?;
        writer.write_bits(nominal_bitrate, 32);
        writer.write_bits(0, 32); // bitrate_minimum

        // Valid block sizes 0 and 1

        writer.write_bits(self.blocksize_0_pow as u32, 4);
        writer.write_bits(self.blocksize_1_pow as u32, 4);
        writer.write_bits(1, 1); // framing

        Ok(writer.into_inner())
    }

    fn generate_comment_packet(&mut self) -> WemResult<Vec<u8>> {
        let mut writer = BitWriter::new();
        self.write_vorbis_packet_header(&mut writer, 3);

        let vendor = format!(
            "converted from Audiokinetic Wwise by wem-audio {}",
            env!("CARGO_PKG_VERSION")
        );
        writer.write_bits(vendor.len() as u32, 32);
        for c in vendor.bytes() {
            writer.write_bits(c as u32, 8);
        }

        if self.loop_count == 0 {
            writer.write_bits(0, 32); // no user comments
        } else {
            writer.write_bits(2, 32); // two comments
            let loop_start = format!("LoopStart={}", self.loop_start);
            let loop_end = format!("LoopEnd={}", self.loop_end);

            writer.write_bits(loop_start.len() as u32, 32);
            for c in loop_start.bytes() {
                writer.write_bits(c as u32, 8);
            }

            writer.write_bits(loop_end.len() as u32, 32);
            for c in loop_end.bytes() {
                writer.write_bits(c as u32, 8);
            }
        }

        writer.write_bits(1, 1); // framing
        Ok(writer.into_inner())
    }

    fn generate_setup_packet(&mut self) -> WemResult<(Vec<u8>, Vec<bool>)> {
        let mut writer = BitWriter::new();
        self.write_vorbis_packet_header(&mut writer, 5);

        let data = self
            .chunks
            .data
            .ok_or_else(|| WemError::parse("missing data chunk"))?;

        // Save current position
        let _current_pos = self.input.stream_position()?;

        let setup_packet = Packet::read(
            &mut self.input,
            data.offset + self.setup_packet_offset as u64,
            self.little_endian,
            self.no_granule,
        )?;

        self.input.seek(SeekFrom::Start(setup_packet.offset))?;

        if setup_packet.granule != 0 {
            return Err(WemError::parse("setup packet granule != 0"));
        }

        let setup_is_full = match self.setup_format {
            SetupFormat::Full => true,
            SetupFormat::Stripped => false,
            SetupFormat::Auto => {
                if setup_packet.size < 4 {
                    false
                } else {
                    let mut probe = [0_u8; 4];
                    self.input.read_exact(&mut probe)?;
                    self.input.seek(SeekFrom::Start(setup_packet.offset))?;
                    probe[1] == b'B' && probe[2] == b'C' && probe[3] == b'V'
                }
            }
        };

        if setup_is_full {
            let setup_size = usize::try_from(setup_packet.size)
                .map_err(|_| WemError::size_overflow("Vorbis setup packet"))?;
            let mut setup_payload = vec![0_u8; setup_size];
            self.input.read_exact(&mut setup_payload)?;
            let mut packet = writer.into_inner();
            packet.extend_from_slice(&setup_payload);
            return Ok((packet, Vec::new()));
        }

        let mut reader = BitReader::new(&mut self.input);

        let codebook_count_less1 = reader.read_bits(8)?;
        let codebook_count = codebook_count_less1 + 1;
        writer.write_bits(codebook_count_less1, 8);

        // Rebuild codebooks
        if self.inline_codebooks {
            for _ in 0..codebook_count {
                self.codebooks
                    .as_ref()
                    .ok_or(WemError::MissingCodebooks)?
                    .rebuild_from_reader(&mut reader, &mut writer)?;
            }
        } else {
            let codebooks = self.codebooks.as_ref().ok_or(WemError::MissingCodebooks)?;
            for _ in 0..codebook_count {
                let codebook_id = reader.read_bits(10)?;

                match codebooks.rebuild(codebook_id as usize, &mut writer) {
                    Ok(_) => {}
                    Err(WemError::InvalidCodebookId { .. }) => {
                        if codebook_id == 0x342 {
                            let codebook_identifier = reader.read_bits(14)?;
                            if codebook_identifier == 0x1590 {
                                return Err(WemError::parse(
                                    "invalid codebook id 0x342, try --full-setup",
                                ));
                            }
                        }
                        return Err(WemError::invalid_codebook_id(codebook_id as i32));
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Time domain transforms placeholder
        writer.write_bits(0, 6); // time_count_less1
        writer.write_bits(0, 16); // dummy_time_value

        let mode_blockflag =
            super::setup::rebuild_setup(self.channels, &mut reader, &mut writer, codebook_count)?;

        Ok((writer.into_inner(), mode_blockflag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogg::PacketReader;
    use std::io::Cursor;

    fn header_triad_wem() -> Vec<u8> {
        let headers: [&[u8]; 3] = [
            b"\x01vorbis-identification",
            b"\x03vorbis-comment",
            b"\x05vorbis-setup",
        ];
        let mut data = Vec::new();
        for header in headers {
            data.extend_from_slice(&(header.len() as u32).to_le_bytes());
            data.extend_from_slice(&0_u32.to_le_bytes());
            data.extend_from_slice(header);
        }
        let first_audio_offset = data.len() as u32;

        let mut fmt = Vec::new();
        fmt.extend_from_slice(&crate::WWISE_FORMAT_VORBIS.to_le_bytes());
        fmt.extend_from_slice(&1_u16.to_le_bytes());
        fmt.extend_from_slice(&44_100_u32.to_le_bytes());
        fmt.extend_from_slice(&8_000_u32.to_le_bytes());
        fmt.extend_from_slice(&0_u16.to_le_bytes());
        fmt.extend_from_slice(&0_u16.to_le_bytes());
        fmt.extend_from_slice(&6_u16.to_le_bytes());
        fmt.extend_from_slice(&0_u16.to_le_bytes());
        fmt.extend_from_slice(&0_u32.to_le_bytes());

        let mut vorb = vec![0_u8; 0x2C];
        vorb[0..4].copy_from_slice(&1_u32.to_le_bytes());
        vorb[0x18..0x1C].copy_from_slice(&0_u32.to_le_bytes());
        vorb[0x1C..0x20].copy_from_slice(&first_audio_offset.to_le_bytes());

        let riff_size = 4 + 8 + fmt.len() + 8 + vorb.len() + 8 + data.len();
        let mut wem = Vec::new();
        wem.extend_from_slice(b"RIFF");
        wem.extend_from_slice(&(riff_size as u32).to_le_bytes());
        wem.extend_from_slice(b"WAVEfmt ");
        wem.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        wem.extend_from_slice(&fmt);
        wem.extend_from_slice(b"vorb");
        wem.extend_from_slice(&(vorb.len() as u32).to_le_bytes());
        wem.extend_from_slice(&vorb);
        wem.extend_from_slice(b"data");
        wem.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wem.extend_from_slice(&data);
        wem
    }

    #[test]
    fn copies_legacy_header_triad() {
        let mut decoder =
            VorbisWemDecoder::with_options(Cursor::new(header_triad_wem()), VorbisOptions::new())
                .unwrap();
        let mut ogg = Vec::new();
        decoder.decode_to_ogg(&mut ogg).unwrap();

        let mut packets = PacketReader::new(Cursor::new(ogg));
        for packet_type in [1, 3, 5] {
            let packet = packets.read_packet().unwrap().unwrap();
            assert_eq!(packet.data[0], packet_type);
            assert_eq!(&packet.data[1..7], b"vorbis");
        }
    }
}
