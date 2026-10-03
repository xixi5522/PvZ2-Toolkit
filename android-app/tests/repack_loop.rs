//! 回包闭环验证：构造带贴图的小 RSB → rsb.unpack → 改 PNG → rsb.pack →
//! 再解包确认改动生效。顺带覆盖 ptx.decode / ptx.encode 独立互转与
//! rsgp.unpack / rsgp.pack 的 manifest 回包。
//!
//! 全部走命令层 pub 函数（与 UI 表单等价的 Params 调用）。

use pvz2_toolkit_android::commands::{archive, audio};
use pvz2_toolkit_android::params::Params;
use rsb_archive::{
    AutoPoolInfo, FileListInfo, Part1Extra, RSB_MAGIC, RsbHeader, RsbPtxInfo, RsbWriter,
    RsgInfo, UnpackedFile, pack_rsg,
};
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "pvz2-toolkit-loop-{tag}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn params(pairs: &[(&str, &str)]) -> Params {
    Params::new(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>(),
    )
}

/// 8×8 纯色图（Rgba8888 payload）。
fn solid_image(r: u8, g: u8, b: u8, a: u8) -> image::RgbaImage {
    image::RgbaImage::from_fn(8, 8, |_, _| image::Rgba([r, g, b, a]))
}

/// 构造一个 1 分包的 RSB：DATA/TEST.RTON + IMAGES/TEST.ptx（Rgba8888 8×8）。
fn sample_rsb() -> Vec<u8> {
    const PACKET_OFFSET: u32 = 0x1000;
    let ptx_data = rsb_archive::PtxEncoder::encode_image(
        &solid_image(255, 0, 0, 255),
        rsb_archive::PtxEncodeOptions::new(rsb_archive::PtxFormat::Rgba8888),
    )
    .unwrap();
    assert_eq!(ptx_data.len(), 8 * 8 * 4);

    let files = vec![
        UnpackedFile {
            path: "DATA/TEST.RTON".into(),
            data: b"test-rton-payload".to_vec(),
            is_part1: false,
            part1_info: None,
        },
        UnpackedFile {
            path: "IMAGES/TEST.ptx".into(),
            data: ptx_data,
            is_part1: true,
            part1_info: Some(Part1Extra {
                id: 0,
                width: 8,
                height: 8,
            }),
        },
    ];
    let mut packet = Cursor::new(Vec::new());
    pack_rsg(&mut packet, &files, 4, 3).unwrap();
    let mut packet = packet.into_inner();
    let packet_head = packet[16..48].to_vec();
    let packet_length = packet.len().next_multiple_of(0x1000);
    packet.resize(packet_length, 0);

    let header = RsbHeader {
        magic: RSB_MAGIC,
        version: 4,
        information_section_size: PACKET_OFFSET,
        resource_path_section_size: 0,
        resource_path_section_offset: 0x100,
        rsg_list_length: 0,
        rsg_list_begin_offset: 0x200,
        rsg_number: 1,
        rsg_info_begin_offset: 0x300,
        rsg_info_each_length: 204,
        composite_number: 0,
        composite_info_begin_offset: 0,
        composite_info_each_length: 1156,
        composite_list_length: 0,
        composite_list_begin_offset: 0,
        autopool_number: 1,
        autopool_info_begin_offset: 0x500,
        autopool_info_each_length: 152,
        ptx_number: 1,
        ptx_info_begin_offset: 0x600,
        ptx_info_each_length: 24,
        part1_begin_offset: 0,
        part2_begin_offset: 0,
        part3_begin_offset: 0,
        information_without_manifest_section_size: PACKET_OFFSET,
    };
    let info = RsgInfo {
        name: "Sample_Common".into(),
        rsg_offset: PACKET_OFFSET,
        rsg_length: packet_length as u32,
        pool_index: 0,
        ptx_number: 1,
        ptx_before_number: 0,
        packet_head_info: Some(packet_head),
    };

    let mut output = Cursor::new(vec![0; PACKET_OFFSET as usize]);
    output.seek(SeekFrom::Start(0x100)).unwrap();
    let (file_list_offset, file_list_length) = RsbWriter::new(&mut output)
        .write_file_list(&[
            FileListInfo {
                name_path: "DATA/TEST.RTON".into(),
                pool_index: 0,
            },
            FileListInfo {
                name_path: "IMAGES/TEST.ptx".into(),
                pool_index: 0,
            },
        ])
        .unwrap();
    output.seek(SeekFrom::Start(0x200)).unwrap();
    let (rsg_list_offset, rsg_list_length) = RsbWriter::new(&mut output)
        .write_file_list(&[FileListInfo {
            name_path: info.name.clone(),
            pool_index: 0,
        }])
        .unwrap();
    output.seek(SeekFrom::Start(0x300)).unwrap();
    RsbWriter::new(&mut output)
        .write_rsg_info(std::slice::from_ref(&info), &[(1, 0)])
        .unwrap();
    output.seek(SeekFrom::Start(0x500)).unwrap();
    RsbWriter::new(&mut output)
        .write_autopool_info(&[AutoPoolInfo {
            name: "Sample_Common_AutoPool".into(),
            part0_size: 0x1000,
            part1_size: 0,
        }])
        .unwrap();
    output.seek(SeekFrom::Start(0x600)).unwrap();
    RsbWriter::new(&mut output)
        .write_ptx_info(
            &[RsbPtxInfo {
                ptx_index: 0,
                width: 8,
                height: 8,
                pitch: 32,
                format: 0,
                alpha_size: None,
                alpha_format: None,
            }],
            24,
        )
        .unwrap();

    let mut header = header;
    header.resource_path_section_offset = file_list_offset;
    header.resource_path_section_size = file_list_length;
    header.rsg_list_begin_offset = rsg_list_offset;
    header.rsg_list_length = rsg_list_length;
    output.seek(SeekFrom::Start(0)).unwrap();
    RsbWriter::new(&mut output).write_header(&header).unwrap();
    output.seek(SeekFrom::Start(u64::from(PACKET_OFFSET))).unwrap();
    output.write_all(&packet).unwrap();
    output.into_inner()
}

fn write_bytes(p: &Path, data: &[u8]) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, data).unwrap();
}

fn read_png(p: &Path) -> image::RgbaImage {
    image::open(p).unwrap().to_rgba8()
}

/// 主闭环：解包 → 改图 → 回包 → 再解包验证。
#[test]
fn rsb_repack_roundtrip_applies_png_edits() {
    let source = sample_rsb();
    let base = temp_dir("repack");
    let rsb_path = base.join("main.rsb");
    write_bytes(&rsb_path, &source);

    // ---- 1. 解包（转 PNG） ----
    let dir1 = base.join("unpacked");
    let msg = archive::rsb_unpack(&params(&[
        ("input", rsb_path.to_str().unwrap()),
        ("output", dir1.to_str().unwrap()),
        ("ptx-png", "true"),
    ]))
    .unwrap();
    assert!(msg.contains("2 个文件"), "解包结果：{msg}");
    assert!(dir1.join("DATA/TEST.RTON").exists());
    let png1 = dir1.join("IMAGES/TEST.png");
    assert!(png1.exists(), "贴图应转出 PNG");
    assert_eq!(read_png(&png1).get_pixel(0, 0).0, [255, 0, 0, 255]);
    // textures.json 元数据清单
    let textures: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir1.join("textures.json")).unwrap()).unwrap();
    assert_eq!(textures[0]["format_code"], 0);
    assert_eq!(textures[0]["width"], 8);
    assert_eq!(textures[0]["format"], "Rgba8888");

    // ---- 2. 改图（红→蓝） ----
    let blue = solid_image(0, 0, 255, 255);
    blue.save(&png1).unwrap();

    // ---- 3. 回包 ----
    let packed = base.join("main_packed.rsb");
    let msg = archive::rsb_pack(&params(&[
        ("input", rsb_path.to_str().unwrap()),
        ("dir", dir1.to_str().unwrap()),
        ("output", packed.to_str().unwrap()),
    ]))
    .unwrap();
    assert!(msg.contains("回包"), "回包结果：{msg}");

    // ---- 4. 再解包验证 ----
    let dir2 = base.join("recheck");
    archive::rsb_unpack(&params(&[
        ("input", packed.to_str().unwrap()),
        ("output", dir2.to_str().unwrap()),
        ("ptx-png", "true"),
    ]))
    .unwrap();
    assert_eq!(
        read_png(&dir2.join("IMAGES/TEST.png")).get_pixel(0, 0).0,
        [0, 0, 255, 255],
        "回包后贴图应是改过的蓝色"
    );
    // 未改的普通文件内容保持
    assert_eq!(
        std::fs::read(dir2.join("DATA/TEST.RTON")).unwrap(),
        b"test-rton-payload".as_slice()
    );

    // ---- 5. 原样回包应报「没有改动」（用刚解包出的目录对比它自己的来源包） ----
    let r = archive::rsb_pack(&params(&[
        ("input", packed.to_str().unwrap()),
        ("dir", dir2.to_str().unwrap()),
        ("output", base.join("noop.rsb").to_str().unwrap()),
    ]));
    assert!(r.is_err(), "完全一致的内容不应产出新包");
}

/// 独立 PTX 互转：.ptx → PNG（ptx.decode），PNG → .ptx（ptx.encode）。
#[test]
fn ptx_standalone_convert_roundtrip() {
    let base = temp_dir("ptx");
    let ptx = rsb_archive::PtxEncoder::encode_image(
        &solid_image(10, 200, 30, 255),
        rsb_archive::PtxEncodeOptions::new(rsb_archive::PtxFormat::Rgba8888),
    )
    .unwrap();
    let ptx_path = base.join("standalone.ptx");
    write_bytes(&ptx_path, &ptx);

    // 名称解析路径
    let png = base.join("decoded.png");
    let msg = archive::ptx_decode(&params(&[
        ("input", ptx_path.to_str().unwrap()),
        ("width", "8"),
        ("height", "8"),
        ("format", "Rgba8888"),
        ("output", png.to_str().unwrap()),
    ]))
    .unwrap();
    assert!(msg.contains("Rgba8888"), "{msg}");
    assert_eq!(read_png(&png).get_pixel(3, 3).0, [10, 200, 30, 255]);

    // 数字码路径（0 = Rgba8888）
    let png2 = base.join("decoded2.png");
    archive::ptx_decode(&params(&[
        ("input", ptx_path.to_str().unwrap()),
        ("width", "8"),
        ("height", "8"),
        ("format", "0"),
        ("output", png2.to_str().unwrap()),
    ]))
    .unwrap();
    assert_eq!(read_png(&png2).get_pixel(3, 3).0, [10, 200, 30, 255]);

    // PNG → PTX → PNG
    let reencoded = base.join("reencoded.ptx");
    let msg = archive::ptx_encode(&params(&[
        ("input", png.to_str().unwrap()),
        ("format", "Rgba8888"),
        ("output", reencoded.to_str().unwrap()),
    ]))
    .unwrap();
    assert!(msg.contains("8×8"), "{msg}");
    let png3 = base.join("decoded3.png");
    archive::ptx_decode(&params(&[
        ("input", reencoded.to_str().unwrap()),
        ("width", "8"),
        ("height", "8"),
        ("format", "0"),
        ("output", png3.to_str().unwrap()),
    ]))
    .unwrap();
    assert_eq!(read_png(&png3).get_pixel(3, 3).0, [10, 200, 30, 255]);

    // 尺寸不符要报错
    let bad = archive::ptx_decode(&params(&[
        ("input", ptx_path.to_str().unwrap()),
        ("width", "16"),
        ("height", "8"),
        ("format", "0"),
        ("output", base.join("bad.png").to_str().unwrap()),
    ]));
    assert!(bad.is_err(), "数据长度不符应报错");
}

/// RSGP 闭环：手工 pack_rsg 一个分包 → 解包（manifest）→ 回包 → 对比。
#[test]
fn rsgp_repack_roundtrip() {
    use rsb_archive::unpack_rsg;
    use std::io::Cursor;

    let base = temp_dir("rsgp");
    let ptx = rsb_archive::PtxEncoder::encode_image(
        &solid_image(1, 2, 3, 255),
        rsb_archive::PtxEncodeOptions::new(rsb_archive::PtxFormat::Rgba8888),
    )
    .unwrap();
    let files = vec![
        UnpackedFile {
            path: "LEVEL/NOTE.TXT".into(),
            data: b"hello".to_vec(),
            is_part1: false,
            part1_info: None,
        },
        UnpackedFile {
            path: "IMAGES/P.ptx".into(),
            data: ptx,
            is_part1: true,
            part1_info: Some(Part1Extra {
                id: 0,
                width: 8,
                height: 8,
            }),
        },
    ];
    let mut packet = Cursor::new(Vec::new());
    pack_rsg(&mut packet, &files, 4, 3).unwrap();
    let rsgp = base.join("part.rsgp");
    write_bytes(&rsgp, &packet.into_inner());

    // 解包
    let dir = base.join("unpacked");
    let msg = archive::rsgp_unpack(&params(&[
        ("input", rsgp.to_str().unwrap()),
        ("output", dir.to_str().unwrap()),
    ]))
    .unwrap();
    assert!(msg.contains("2 个文件"), "{msg}");
    assert!(dir.join("manifest.json").exists());

    // 改普通文件后回包
    write_bytes(&dir.join("LEVEL/NOTE.TXT"), b"changed");
    let repacked = base.join("part_packed.rsgp");
    let msg = archive::rsgp_pack(&params(&[
        ("input", dir.to_str().unwrap()),
        ("version", "4"),
        ("flags", "3"),
        ("output", repacked.to_str().unwrap()),
    ]))
    .unwrap();
    assert!(msg.contains("1 张贴图"), "{msg}");

    // 再解包验证
    let dir2 = base.join("recheck");
    archive::rsgp_unpack(&params(&[
        ("input", repacked.to_str().unwrap()),
        ("output", dir2.to_str().unwrap()),
    ]))
    .unwrap();
    assert_eq!(std::fs::read(dir2.join("LEVEL/NOTE.TXT")).unwrap(), b"changed");
    // 贴图 payload 经 manifest 重建后仍可解码
    let png = base.join("p.png");
    archive::ptx_decode(&params(&[
        ("input", dir2.join("IMAGES/P.ptx").to_str().unwrap()),
        ("width", "8"),
        ("height", "8"),
        ("format", "0"),
        ("output", png.to_str().unwrap()),
    ]))
    .unwrap();
    assert_eq!(read_png(&png).get_pixel(0, 0).0, [1, 2, 3, 255]);
}

/// WEM 编码冒烟测试：WAV → PCM WEM → 解码回来验证。
#[test]
fn wem_encode_smoke() {
    let base = temp_dir("wem");
    // 手写 16bit 单声道 WAV（RIFF 头 44 字节 + 800 样方波）
    let wav_path = base.join("tone.wav");
    let samples: Vec<i16> = (0..800)
        .map(|i| if (i / 40) % 2 == 0 { 8000 } else { -8000 })
        .collect();
    {
        let data_len = samples.len() * 2;
        let mut wav = Vec::with_capacity(44 + data_len);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // fmt 块大小
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // 单声道
        wav.extend_from_slice(&8000u32.to_le_bytes()); // 采样率
        wav.extend_from_slice(&16000u32.to_le_bytes()); // 字节率
        wav.extend_from_slice(&2u16.to_le_bytes()); // 块对齐
        wav.extend_from_slice(&16u16.to_le_bytes()); // 位深
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data_len as u32).to_le_bytes());
        for s in &samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        write_bytes(&wav_path, &wav);
    }
    let wem_path = base.join("tone.wem");
    let msg = audio::wem_encode(&params(&[
        ("input", wav_path.to_str().unwrap()),
        ("codec", "pcm"),
        ("output", wem_path.to_str().unwrap()),
    ]))
    .unwrap();
    assert!(msg.contains("tone_encoded.wem") || msg.contains("wem"), "{msg}");
    assert!(wem_path.exists());

    // 编码产物能被自家解码器读回（只验 WAV 头字段，避免引入 hound）
    let wav2 = base.join("tone2.wav");
    audio::wem_decode(&params(&[
        ("input", wem_path.to_str().unwrap()),
        ("output", wav2.to_str().unwrap()),
    ]))
    .unwrap();
    let wav2_bytes = std::fs::read(&wav2).unwrap();
    assert!(wav2_bytes.len() > 44 && &wav2_bytes[..4] == b"RIFF");
    let rate = u32::from_le_bytes(wav2_bytes[24..28].try_into().unwrap());
    assert_eq!(rate, 8000);
}
