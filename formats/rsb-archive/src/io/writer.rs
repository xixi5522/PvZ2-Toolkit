use crate::error::{Result, RsbError};
use crate::schema::types::*;
use byteorder::{LE, WriteBytesExt};
use std::collections::HashMap;
use std::io::{Seek, Write};

pub struct RsbWriter<W: Write + Seek> {
    pub writer: W,
}

impl<W: Write + Seek> RsbWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write_header(&mut self, header: &RsbHeader) -> Result<()> {
        if header.magic != RSB_MAGIC {
            let found = header.magic.to_be_bytes();
            return Err(RsbError::InvalidMagic(
                "rsb1".to_string(),
                String::from_utf8_lossy(&found).to_string(),
            ));
        }
        self.writer.write_u32::<LE>(header.magic)?;
        self.writer.write_u32::<LE>(header.version)?;
        self.writer.write_u32::<LE>(0)?; // Padding/reserved at 0x8? Sen reads 0
        self.writer
            .write_u32::<LE>(header.information_section_size)?;

        self.writer
            .write_u32::<LE>(header.resource_path_section_size)?;
        self.writer
            .write_u32::<LE>(header.resource_path_section_offset)?;

        // Bytes 0x18-0x1F: constant zero padding (Twinning: unknown_5, unknown_6)
        self.writer.write_u64::<LE>(0)?;

        self.writer.write_u32::<LE>(header.rsg_list_length)?;
        self.writer.write_u32::<LE>(header.rsg_list_begin_offset)?;
        self.writer.write_u32::<LE>(header.rsg_number)?;
        self.writer.write_u32::<LE>(header.rsg_info_begin_offset)?;
        self.writer.write_u32::<LE>(header.rsg_info_each_length)?;

        self.writer.write_u32::<LE>(header.composite_number)?;
        self.writer
            .write_u32::<LE>(header.composite_info_begin_offset)?;
        self.writer
            .write_u32::<LE>(header.composite_info_each_length)?;
        self.writer.write_u32::<LE>(header.composite_list_length)?;
        self.writer
            .write_u32::<LE>(header.composite_list_begin_offset)?;

        self.writer.write_u32::<LE>(header.autopool_number)?;
        self.writer
            .write_u32::<LE>(header.autopool_info_begin_offset)?;
        self.writer
            .write_u32::<LE>(header.autopool_info_each_length)?;

        self.writer.write_u32::<LE>(header.ptx_number)?;
        self.writer.write_u32::<LE>(header.ptx_info_begin_offset)?;
        self.writer.write_u32::<LE>(header.ptx_info_each_length)?;

        self.writer.write_u32::<LE>(header.part1_begin_offset)?;
        self.writer.write_u32::<LE>(header.part2_begin_offset)?;
        self.writer.write_u32::<LE>(header.part3_begin_offset)?;

        // v4+: information_without_manifest_section_size (Twinning)
        if header.version >= 4 {
            self.writer
                .write_u32::<LE>(header.information_without_manifest_section_size)?;
        }

        Ok(())
    }

    pub fn write_resources_description(
        &mut self,
        desc: &ResourcesDescription,
        header: &mut RsbHeader,
    ) -> Result<()> {
        // We need to generate Part 1, Part 2, Part 3.
        // Part 3 is the string pool. Part 1 and 2 reference it.
        // We'll write to temporary buffers first, then combine.

        let mut part1_buf = Vec::new();
        let mut part2_buf = Vec::new();
        let mut part3_buf = Vec::new();

        // Initialize string pool with empty string at offset 0
        let mut string_pool: HashMap<String, u32> = HashMap::new();

        // Helper to add string to pool and return offset
        let mut add_string = |s: &str| -> std::io::Result<u32> {
            if let Some(&off) = string_pool.get(s) {
                Ok(off)
            } else {
                let off = part3_buf.len() as u32;
                string_pool.insert(s.to_string(), off);
                // Strings in Part 3 are null-terminated?
                // Sen: writeStringByEmpty -> writes string + null? Or just string if empty?
                // Sen: writeStringByEmpty() writes length-prefixed? No, SenBuffer string read/write usually varies.
                // Sen Read: getStringByEmpty -> Reads until null or based on some structure.
                // Sen Read: readStringByEmpty -> reads until \0.
                // Sen Write: writeStringByEmpty -> Writes chars then \0.
                // Let's assume null-terminated.
                if s.is_empty() {
                    part3_buf.write_u8(0)?;
                } else {
                    part3_buf.write_all(s.as_bytes())?;
                    part3_buf.write_u8(0)?;
                }
                Ok(off)
            }
        };

        // Initialize with empty string
        add_string("")?;

        // Sort keys for deterministic output (Sen does foreach on Keys, which is undefined order in C#,
        // usually definition order or hash order. To be safe/stable, we sort).
        // Sen source uses `resourcesDescription.groups.Keys` which comes from Dictionary.
        // We should sort.

        let mut group_keys: Vec<&String> = desc.groups.keys().collect();
        group_keys.sort();

        for g_key in group_keys {
            let id_offset_part3 = add_string(g_key)?;
            part1_buf.write_u32::<LE>(id_offset_part3)?;

            let group = &desc.groups[g_key];
            let mut subgroup_keys: Vec<&String> = group.subgroups.keys().collect();
            subgroup_keys.sort();

            part1_buf.write_u32::<LE>(subgroup_keys.len() as u32)?; // rsgNumber
            part1_buf.write_u32::<LE>(0x10)?; // checkVal

            for sub_key in subgroup_keys {
                let sub = &group.subgroups[sub_key];

                // Resolution ratio i32
                let res_val = sub.res.parse::<i32>().unwrap_or(0);
                part1_buf.write_i32::<LE>(res_val)?;

                // Language 4 chars
                let mut lang_bytes = sub.language.as_bytes().to_vec();
                if lang_bytes.is_empty() {
                    part1_buf.write_u32::<LE>(0)?;
                } else {
                    lang_bytes.resize(4, 0x20); // Pad with spaces if needed? Sen: "language + '    '".Substring(0,4)
                    // Sen: if empty write 0, else write 4 chars.
                    // The logic `(language + "    ")[..4]` implies space padding.
                    part1_buf.write_all(&lang_bytes[..4])?;
                }

                let rsg_id_offset = add_string(sub_key)?;
                part1_buf.write_u32::<LE>(rsg_id_offset)?;

                let mut res_keys: Vec<&String> = sub.resources.keys().collect();
                res_keys.sort();

                part1_buf.write_u32::<LE>(res_keys.len() as u32)?; // resourcesNumber

                for res_key in res_keys {
                    let resource = &sub.resources[res_key];

                    let info_offset_part2 = part2_buf.len() as u32;
                    part1_buf.write_u32::<LE>(info_offset_part2)?;

                    // Write Part 2 Entry
                    part2_buf.write_u32::<LE>(0)?; // Check 0
                    part2_buf.write_u16::<LE>(resource.res_type as u16)?;
                    part2_buf.write_u16::<LE>(0x1C)?; // Check 0x1C

                    // Placeholders for PTX offsets (written later)
                    let ptx_offsets_pos = part2_buf.len();
                    part2_buf.write_u32::<LE>(0)?; // End
                    part2_buf.write_u32::<LE>(0)?; // Begin

                    let res_id_off = add_string(res_key)?;
                    let path_off = add_string(&resource.path)?;

                    part2_buf.write_u32::<LE>(res_id_off)?;
                    part2_buf.write_u32::<LE>(path_off)?;

                    part2_buf.write_u32::<LE>(resource.properties.len() as u32)?;

                    // Write PTX optional info
                    if resource.res_type == 0 {
                        let ptx_begin = part2_buf.len() as u32;
                        if let Some(ptx) = &resource.ptx_info {
                            part2_buf.write_u16::<LE>(ptx.imagetype.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.aflags.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.x.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.y.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.ax.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.ay.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.aw.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.ah.parse().unwrap_or(0))?;
                            part2_buf.write_u16::<LE>(ptx.rows.parse().unwrap_or(1))?;
                            part2_buf.write_u16::<LE>(ptx.cols.parse().unwrap_or(1))?;
                            let parent_off = add_string(&ptx.parent)?;
                            part2_buf.write_u32::<LE>(parent_off)?;
                        }
                        let ptx_end = part2_buf.len() as u32;

                        // Go back and fill offsets
                        // Write to vec by index
                        // ptx_offsets_pos is index of End
                        // ptx_offsets_pos + 4 is index of Begin
                        let mut slice = &mut part2_buf[ptx_offsets_pos..ptx_offsets_pos + 8];
                        slice.write_u32::<LE>(ptx_end)?;
                        slice.write_u32::<LE>(ptx_begin)?;
                    }

                    // Write Properties
                    let mut prop_keys: Vec<&String> = resource.properties.keys().collect();
                    prop_keys.sort();

                    for p_key in prop_keys {
                        let val = &resource.properties[p_key];
                        let k_off = add_string(p_key)?;
                        let v_off = add_string(val)?;

                        part2_buf.write_u32::<LE>(k_off)?;
                        part2_buf.write_u32::<LE>(0)?; // padding/check? Sen reads 0 check here
                        part2_buf.write_u32::<LE>(v_off)?;
                    }
                }
            }
        }

        // Write buffers to writer and update header
        // Align writer to verify?

        header.part1_begin_offset = self.writer.stream_position()? as u32;
        self.writer.write_all(&part1_buf)?;

        header.part2_begin_offset = self.writer.stream_position()? as u32;
        self.writer.write_all(&part2_buf)?;

        header.part3_begin_offset = self.writer.stream_position()? as u32;
        self.writer.write_all(&part3_buf)?;

        Ok(())
    }

    pub fn write_file_list(&mut self, file_list: &[FileListInfo]) -> Result<(u32, u32)> {
        let start_offset = self.writer.stream_position()? as u32;

        let items: Vec<(String, i32)> = file_list
            .iter()
            .map(|info| (info.name_path.clone(), info.pool_index))
            .collect();

        use crate::schema::file_list::write_file_list;
        write_file_list(&mut self.writer, start_offset as u64, &items)?;

        let end_offset = self.writer.stream_position()? as u32;
        Ok((start_offset, end_offset - start_offset))
    }

    pub fn write_composite_info(&mut self, composite_info: &[CompositeInfo]) -> Result<(u32, u32)> {
        let start_offset = self.writer.stream_position()? as u32;
        let composite_number = composite_info.len() as u32;
        let each_length = 1156;

        for info in composite_info {
            let item_start = self.writer.stream_position()?;

            let name = if info.is_composite {
                info.name.clone()
            } else {
                format!("{}_CompositeShell", info.name)
            };

            self.write_string_fixed(&name, 128)?;

            // Skip to packet info
            self.writer
                .seek(std::io::SeekFrom::Start(item_start + 128))?;

            for pkt in &info.packet_info {
                self.writer.write_i32::<LE>(pkt.packet_index)?;
                // Category[0] as int, Category[1] as string(4)
                let cat0 = pkt.category[0].parse::<i32>().unwrap_or(0);
                self.writer.write_i32::<LE>(cat0)?;

                let mut cat1_bytes: [u8; 4] = [0u8; 4];
                let bytes = pkt.category[1].as_bytes();
                if !bytes.is_empty() {
                    let len = std::cmp::min(bytes.len(), 4);
                    cat1_bytes[..len].copy_from_slice(&bytes[..len]);
                }
                self.writer.write_all(&cat1_bytes)?;
                self.writer.write_u32::<LE>(0)?; // Padding 4 bytes? Sen does writeNull(4) after
            }

            // Write subheader info at end of struct
            self.writer.seek(std::io::SeekFrom::Start(
                item_start + each_length as u64 - 8,
            ))?;
            // Padding checks in Sen: compositeInfo.writeNull(1024 - (subgroupLength * 16));
            // Then writeInt32LE(subgroupLength);
            // We need to match this layout.
            // 128 (Name) + 16 * packet_info.len() + Padding + 4 (Count) = 1156?
            // 128 + 1024 + 4 = 1156. Correct.

            self.writer.write_u32::<LE>(0)?; // Padding? Sen writes at end-4
            self.writer.write_u32::<LE>(info.packet_info.len() as u32)?;

            // Fill padding if needed, but we seeked so spaces should be 0 if pre-allocated or we need to ensure size
            let current = self.writer.stream_position()?;
            if current < item_start + each_length as u64 {
                let pad = (item_start + each_length as u64) - current;
                self.writer.write_all(&vec![0u8; pad as usize])?;
            }
        }

        let end_offset = self.writer.stream_position()? as u32;
        // Verify alignment/size
        if (end_offset - start_offset) != composite_number * each_length {
            // Force alignment if needed, though loop should handle it
        }

        Ok((start_offset, each_length))
    }

    pub fn write_autopool_info(&mut self, autopool_info: &[AutoPoolInfo]) -> Result<(u32, u32)> {
        let start_offset = self.writer.stream_position()? as u32;
        let each_length = 152;

        for info in autopool_info {
            let item_start = self.writer.stream_position()?;
            self.write_string_fixed(&info.name, 128)?;

            self.writer.write_u32::<LE>(info.part0_size)?;
            self.writer.write_u32::<LE>(info.part1_size)?;

            // Pad to next
            let current = self.writer.stream_position()?;
            let end = item_start + each_length as u64;
            if current < end {
                self.writer
                    .write_all(&vec![0u8; (end - current) as usize])?;
            }
        }

        Ok((start_offset, each_length))
    }

    // Refactored RSG Info Writer
    pub fn write_rsg_info(
        &mut self,
        rsg_infos: &[RsgInfo],
        ptx_counts: &[(u32, u32)],
    ) -> Result<(u32, u32)> {
        let start_offset = self.writer.stream_position()? as u32;
        let each_length = 204;

        for (i, info) in rsg_infos.iter().enumerate() {
            let item_start = self.writer.stream_position()?;

            self.write_string_fixed(&info.name, 128)?;

            self.writer.write_u32::<LE>(info.rsg_offset)?;
            self.writer.write_u32::<LE>(info.rsg_length)?;
            self.writer.write_i32::<LE>(info.pool_index)?;

            // PacketHeadInfo (32 bytes) - usually extracted from RSG
            if let Some(head) = &info.packet_head_info {
                if head.len() >= 32 {
                    self.writer.write_all(&head[..32])?;
                } else {
                    self.writer.write_all(head)?;
                    self.writer.write_all(&vec![0u8; 32 - head.len()])?;
                }
            } else {
                self.writer.write_all(&[0u8; 32])?;
            }

            // Offsets and Size
            // Sen writes:
            // rsgInfo.writeInt32LE((int)RSGFile.readInt32LE(0x20), rsgWriteOffset - 36); --> This is inside packet head info area?
            // rsgInfo.writeInt32LE(ptxNumber, rsgWriteOffset);
            // rsgInfo.writeInt32LE(ptxBeforeNumber);

            // We need to write ptx numbers at correct offset.
            // 204 - 8 = 196 -> ptxNumber
            // 200 -> ptxBeforeNumber

            self.writer.seek(std::io::SeekFrom::Start(
                item_start + each_length as u64 - 8,
            ))?;

            let (ptx_num, ptx_before) = ptx_counts[i];
            self.writer.write_u32::<LE>(ptx_num)?;
            self.writer.write_u32::<LE>(ptx_before)?;

            // Ensure full size is written (pad if seek jumped forward, though seek handles it)
            let end_packet = item_start + each_length as u64;
            let current = self.writer.stream_position()?;
            if current < end_packet {
                self.writer
                    .write_all(&vec![0u8; (end_packet - current) as usize])?;
            }
        }

        Ok((start_offset, each_length))
    }

    pub fn write_ptx_info(&mut self, ptx_infos: &[RsbPtxInfo], each_len: u32) -> Result<u32> {
        let start_offset = self.writer.stream_position()? as u32;

        for info in ptx_infos {
            self.writer.write_i32::<LE>(info.width)?;
            self.writer.write_i32::<LE>(info.height)?;
            self.writer.write_i32::<LE>(info.pitch)?;
            self.writer.write_i32::<LE>(info.format)?;

            if each_len >= 0x14 {
                self.writer.write_i32::<LE>(info.alpha_size.unwrap_or(0))?;
            }
            if each_len >= 0x18 {
                self.writer
                    .write_i32::<LE>(info.alpha_format.unwrap_or(0))?;
            }
        }

        Ok(start_offset)
    }

    fn write_string_fixed(&mut self, s: &str, len: usize) -> Result<()> {
        let mut bytes = vec![0u8; len];
        let src = s.as_bytes();
        let copy_len = std::cmp::min(src.len(), len - 1); // Leave space for null? Sen writes nulls after string length
        bytes[..copy_len].copy_from_slice(&src[..copy_len]);
        self.writer.write_all(&bytes)?;
        Ok(())
    }
}
