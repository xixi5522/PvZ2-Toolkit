use crate::error::{Result, RsbError};
use crate::schema::file_list::read_file_list;
use crate::schema::types::{
    AutoPoolInfo, CompositeInfo, CompositePacketInfo, DescriptionGroup, DescriptionResources,
    DescriptionSubGroup, FileListInfo, PropertiesPtxInfo, RSB_MAGIC, ResourcesDescription,
    RsbHeader, RsbPtxInfo, RsgInfo,
};
use crate::utils::{BinReadExt, read_string_at};
use byteorder::{LE, ReadBytesExt};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

// Payloads for RSB lists handled in utils or custom structs

pub struct Rsb<R> {
    reader: R,
    pub header: RsbHeader,
}

impl<R: Read + Seek> Rsb<R> {
    pub fn open(mut reader: R) -> Result<Self> {
        let magic = reader.read_u32::<LE>()?;
        if magic != RSB_MAGIC {
            let found = magic.to_be_bytes();
            return Err(RsbError::InvalidMagic(
                "rsb1".to_string(),
                String::from_utf8_lossy(&found).to_string(),
            ));
        }

        let version = reader.read_u32::<LE>()?;
        reader.read_u32::<LE>()?; // Skip 4 bytes

        let mut header = RsbHeader {
            magic,
            version,
            ..Default::default()
        };

        header.information_section_size = reader.read_u32::<LE>()?;
        header.resource_path_section_size = reader.read_u32::<LE>()?;
        header.resource_path_section_offset = reader.read_u32::<LE>()?;
        reader.read_u64::<LE>()?; // Skip 8: Twinning ConstantBlock unknown_5+6
        header.rsg_list_length = reader.read_u32::<LE>()?;
        header.rsg_list_begin_offset = reader.read_u32::<LE>()?;
        header.rsg_number = reader.read_u32::<LE>()?;
        header.rsg_info_begin_offset = reader.read_u32::<LE>()?;
        header.rsg_info_each_length = reader.read_u32::<LE>()?;
        header.composite_number = reader.read_u32::<LE>()?;
        header.composite_info_begin_offset = reader.read_u32::<LE>()?;
        header.composite_info_each_length = reader.read_u32::<LE>()?;
        header.composite_list_length = reader.read_u32::<LE>()?;
        header.composite_list_begin_offset = reader.read_u32::<LE>()?;
        header.autopool_number = reader.read_u32::<LE>()?;
        header.autopool_info_begin_offset = reader.read_u32::<LE>()?;
        header.autopool_info_each_length = reader.read_u32::<LE>()?;
        header.ptx_number = reader.read_u32::<LE>()?;
        header.ptx_info_begin_offset = reader.read_u32::<LE>()?;
        header.ptx_info_each_length = reader.read_u32::<LE>()?;
        header.part1_begin_offset = reader.read_u32::<LE>()?;
        header.part2_begin_offset = reader.read_u32::<LE>()?;
        header.part3_begin_offset = reader.read_u32::<LE>()?;

        // v4+: information_without_manifest_section_size
        // Twinning: Header<4> adds this field after string_manifest_information_section_offset.
        // When manifests are empty, it equals information_section_size.
        if version >= 4 {
            header.information_without_manifest_section_size = reader.read_u32::<LE>()?;
        }

        Ok(Rsb { reader, header })
    }

    pub fn read_file_list(&mut self) -> Result<Vec<FileListInfo>> {
        let raw = read_file_list::<i32, _>(
            &mut self.reader,
            self.header.resource_path_section_offset as u64,
            self.header.resource_path_section_size as u64,
        )?;
        Ok(raw
            .into_iter()
            .map(|(n, p)| FileListInfo {
                name_path: n,
                pool_index: p,
            })
            .collect())
    }

    pub fn read_rsg_info(&mut self) -> Result<Vec<RsgInfo>> {
        let mut infos = Vec::new();
        self.reader
            .seek(SeekFrom::Start(self.header.rsg_info_begin_offset as u64))?;

        for _ in 0..self.header.rsg_number {
            let start = self.reader.stream_position()?;

            // Read name (128 bytes fixed string)
            let mut name_buf = [0u8; 128];
            self.reader.read_exact(&mut name_buf)?;
            let name = String::from_utf8_lossy(&name_buf)
                .trim_matches('\0')
                .to_string();

            // Offset logic from C#
            // C# Reader jumps to start + 128
            let rsg_offset = self.reader.read_u32::<LE>()?;
            let rsg_length = self.reader.read_u32::<LE>()?;
            let pool_index = self.reader.read_i32::<LE>()?;

            // PacketHeadInfo (32 bytes? offset 140?)
            // C# Loosely constraints reader reads from 140
            // Here we assume standard layout
            let mut packet_head_info = vec![0u8; 32];
            // self.reader.seek(SeekFrom::Start(start + 140))?; // If needed
            // But let's follow sequential if possible.
            // 128 + 4 + 4 + 4 = 140. Correct.
            self.reader.read_exact(&mut packet_head_info)?;

            // ptxNumber at offset - 8?
            // "ptxNumber = RSBFile.readInt32LE(startOffset + rsbHeadInfo.rsgInfo_EachLength - 8);"
            self.reader.seek(SeekFrom::Start(
                start + self.header.rsg_info_each_length as u64 - 8,
            ))?;
            let ptx_number = self.reader.read_u32::<LE>()?;
            let ptx_before_number = self.reader.read_u32::<LE>()?;

            infos.push(RsgInfo {
                name,
                rsg_offset,
                rsg_length,
                pool_index,
                ptx_number,
                ptx_before_number,
                packet_head_info: Some(packet_head_info),
            });

            self.reader.seek(SeekFrom::Start(
                start + self.header.rsg_info_each_length as u64,
            ))?;
        }

        Ok(infos)
    }

    // Extraction helper
    pub fn read_composite_info(&mut self) -> Result<Vec<CompositeInfo>> {
        let mut infos = Vec::new();
        self.reader.seek(SeekFrom::Start(
            self.header.composite_info_begin_offset as u64,
        ))?;

        for _ in 0..self.header.composite_number {
            let start = self.reader.stream_position()?;

            // Read name (String by empty)
            // Implementation detail: C# `readStringByEmpty` reads until \0.
            // But here the struct likely reserves 128 bytes or similar?
            // "var compositeName = RSBFile.readStringByEmpty();"
            // No, readStringByEmpty reads char by char.
            // Let's implement helper or read byte by byte.
            let composite_name = self.reader.read_null_term_string()?;

            // Packet number is at end of struct: start + each_length - 4
            self.reader.seek(SeekFrom::Start(
                start + self.header.composite_info_each_length as u64 - 4,
            ))?;
            let packet_number = self.reader.read_u32::<LE>()?;

            // Packet info starts at start + 128
            self.reader.seek(SeekFrom::Start(start + 128))?;
            let mut packet_info = Vec::new();
            for _ in 0..packet_number {
                let packet_index = self.reader.read_i32::<LE>()?;
                let cat_id = self.reader.read_i32::<LE>()?.to_string();
                let mut cat_str_buf = [0u8; 4];
                self.reader.read_exact(&mut cat_str_buf)?;
                let cat_str = String::from_utf8_lossy(&cat_str_buf)
                    .trim_matches('\0')
                    .to_string();
                // Check if we need to skip bytes? C# says: `RSBFile.readBytes(4);` (padding?)
                self.reader.read_u32::<LE>()?; // Skip 4

                packet_info.push(CompositePacketInfo {
                    packet_index,
                    category: [cat_id, cat_str],
                });
            }

            let name = composite_name.replace("_CompositeShell", "");
            let is_composite = !composite_name.ends_with("_CompositeShell");

            infos.push(CompositeInfo {
                name,
                is_composite,
                packet_number,
                packet_info,
            });

            self.reader.seek(SeekFrom::Start(
                start + self.header.composite_info_each_length as u64,
            ))?;
        }
        Ok(infos)
    }

    pub fn read_autopool_info(&mut self) -> Result<Vec<AutoPoolInfo>> {
        let mut infos = Vec::new();
        self.reader.seek(SeekFrom::Start(
            self.header.autopool_info_begin_offset as u64,
        ))?;

        for _ in 0..self.header.autopool_number {
            let start = self.reader.stream_position()?;
            let name = self.reader.read_null_term_string()?;

            self.reader.seek(SeekFrom::Start(start + 128))?;
            let part0_size = self.reader.read_u32::<LE>()?;
            let part1_size = self.reader.read_u32::<LE>()?;

            infos.push(AutoPoolInfo {
                name,
                part0_size,
                part1_size,
            });

            self.reader.seek(SeekFrom::Start(
                start + self.header.autopool_info_each_length as u64,
            ))?;
        }
        Ok(infos)
    }

    pub fn read_ptx_info(&mut self) -> Result<Vec<RsbPtxInfo>> {
        let mut infos = Vec::new();
        self.reader
            .seek(SeekFrom::Start(self.header.ptx_info_begin_offset as u64))?;

        let each_len = self.header.ptx_info_each_length;

        for i in 0..self.header.ptx_number {
            let width = self.reader.read_i32::<LE>()?;
            let height = self.reader.read_i32::<LE>()?;
            let pitch = self.reader.read_i32::<LE>()?;
            let format = self.reader.read_i32::<LE>()?;

            let mut alpha_size = None;
            let mut alpha_format = None;

            if each_len >= 0x14 {
                let size = self.reader.read_i32::<LE>()?;
                alpha_size = Some(size);
                if each_len == 0x18 {
                    alpha_format = Some(self.reader.read_i32::<LE>()?);
                }
            }

            infos.push(RsbPtxInfo {
                ptx_index: i as i32,
                width,
                height,
                pitch,
                format,
                alpha_size,
                alpha_format,
            });
        }
        Ok(infos)
    }

    // Extraction helper (moved down to keep order clean)
    pub fn read_resources_description(
        &mut self,
        _out_folder: &str,
    ) -> Result<ResourcesDescription> {
        let part1_offset = self.header.part1_begin_offset as u64;
        let part2_offset = self.header.part2_begin_offset as u64;
        let part3_offset = self.header.part3_begin_offset as u64;

        self.reader.seek(SeekFrom::Start(part1_offset))?;

        let mut groups = HashMap::new();

        // Temporary structure to hold index data for the second pass
        struct TempRsgInfo {
            rsg_id: String,
            resources_info_list: Vec<u32>, // list of infoOffsetPart2
        }
        struct TempCompositeInfo {
            id: String,
            rsg_infos: Vec<TempRsgInfo>,
        }
        let mut temp_composites = Vec::new();

        // 1st Pass: Read Part 1 (Structure)
        while self.reader.stream_position()? < part2_offset {
            let id_offset_part3 = self.reader.read_u32::<LE>()?;
            let id = read_string_at(&mut self.reader, part3_offset + id_offset_part3 as u64)?;

            let rsg_number = self.reader.read_u32::<LE>()?;

            let check_val = self.reader.read_u32::<LE>()?;
            if check_val != 0x10 {
                // Log warning or ignore? C# throws.
                // println!("Warning: Invalid RSG number check: {:x} at {}", check_val, self.reader.stream_position()? - 4);
            }

            let mut subgroups = HashMap::new();
            let mut temp_rsgs = Vec::new();

            for _ in 0..rsg_number {
                let resolution_ratio = self.reader.read_u32::<LE>()?;
                let mut lang_buf = [0u8; 4];
                self.reader.read_exact(&mut lang_buf)?;
                let language = String::from_utf8_lossy(&lang_buf)
                    .trim_matches('\0')
                    .to_string();

                let rsg_id_offset_part3 = self.reader.read_u32::<LE>()?;
                let rsg_id =
                    read_string_at(&mut self.reader, part3_offset + rsg_id_offset_part3 as u64)?;

                let resources_number = self.reader.read_u32::<LE>()?;

                let mut resources_offsets = Vec::new();
                for _ in 0..resources_number {
                    let info_offset_part2 = self.reader.read_u32::<LE>()?;
                    resources_offsets.push(info_offset_part2);
                }

                subgroups.insert(
                    rsg_id.clone(),
                    DescriptionSubGroup {
                        res: resolution_ratio.to_string(),
                        language,
                        resources: HashMap::new(),
                    },
                );

                temp_rsgs.push(TempRsgInfo {
                    rsg_id,
                    resources_info_list: resources_offsets,
                });
            }

            let is_composite = !id.ends_with("_CompositeShell");
            groups.insert(
                id.clone(),
                DescriptionGroup {
                    composite: is_composite,
                    subgroups,
                },
            );

            temp_composites.push(TempCompositeInfo {
                id,
                rsg_infos: temp_rsgs,
            });
        }

        // 2nd Pass: Read Part 2 (Details) using info from 1st pass
        for comp in temp_composites {
            if let Some(group_entry) = groups.get_mut(&comp.id) {
                for rsg_info in comp.rsg_infos {
                    if let Some(subgroup_entry) = group_entry.subgroups.get_mut(&rsg_info.rsg_id) {
                        for info_offset_part2 in rsg_info.resources_info_list {
                            self.reader
                                .seek(SeekFrom::Start(part2_offset + info_offset_part2 as u64))?;

                            let _check1 = self.reader.read_u32::<LE>()?; // Should be 0
                            let type_val = self.reader.read_u16::<LE>()? as i32;
                            let _check2 = self.reader.read_u16::<LE>()?; // Should be 0x1C

                            let ptx_end = self.reader.read_u32::<LE>()?;
                            let ptx_begin = self.reader.read_u32::<LE>()?;
                            let res_id_offset = self.reader.read_u32::<LE>()?;
                            let path_offset = self.reader.read_u32::<LE>()?;

                            let res_id = read_string_at(
                                &mut self.reader,
                                part3_offset + res_id_offset as u64,
                            )?;
                            let res_path = read_string_at(
                                &mut self.reader,
                                part3_offset + path_offset as u64,
                            )?;

                            let props_num = self.reader.read_u32::<LE>()?;

                            let mut ptx_info = None;
                            if ptx_end != 0 && ptx_begin != 0 {
                                let imagetype = self.reader.read_u16::<LE>()?.to_string();
                                let aflags = self.reader.read_u16::<LE>()?.to_string();
                                let x = self.reader.read_u16::<LE>()?.to_string();
                                let y = self.reader.read_u16::<LE>()?.to_string();
                                let ax = self.reader.read_u16::<LE>()?.to_string();
                                let ay = self.reader.read_u16::<LE>()?.to_string();
                                let aw = self.reader.read_u16::<LE>()?.to_string();
                                let ah = self.reader.read_u16::<LE>()?.to_string();
                                let rows = self.reader.read_u16::<LE>()?.to_string();
                                let cols = self.reader.read_u16::<LE>()?.to_string();
                                let parent_offset_rel = self.reader.read_u32::<LE>()?;
                                let parent = read_string_at(
                                    &mut self.reader,
                                    part3_offset + parent_offset_rel as u64,
                                )?;

                                ptx_info = Some(PropertiesPtxInfo {
                                    imagetype,
                                    aflags,
                                    x,
                                    y,
                                    ax,
                                    ay,
                                    aw,
                                    ah,
                                    rows,
                                    cols,
                                    parent,
                                });
                            }

                            let mut properties = HashMap::new();
                            for _ in 0..props_num {
                                let key_offset = self.reader.read_u32::<LE>()?;
                                let _check_prop = self.reader.read_u32::<LE>()?; // Should be 0
                                let val_offset = self.reader.read_u32::<LE>()?;

                                let key = read_string_at(
                                    &mut self.reader,
                                    part3_offset + key_offset as u64,
                                )?;
                                let val = read_string_at(
                                    &mut self.reader,
                                    part3_offset + val_offset as u64,
                                )?;
                                properties.insert(key, val);
                            }

                            subgroup_entry.resources.insert(
                                res_id,
                                DescriptionResources {
                                    res_type: type_val,
                                    path: res_path,
                                    ptx_info,
                                    properties,
                                },
                            );
                        }
                    }
                }
            }
        }

        Ok(ResourcesDescription { groups })
    }

    pub fn extract_packet(&mut self, info: &RsgInfo) -> Result<Vec<u8>> {
        if info.rsg_length == 0 {
            return Ok(Vec::new());
        }

        self.reader.seek(SeekFrom::Start(info.rsg_offset as u64))?;
        let mut buf = vec![0u8; info.rsg_length as usize];
        self.reader.read_exact(&mut buf)?;
        Ok(buf)
    }
}
