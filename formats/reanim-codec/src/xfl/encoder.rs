use crate::{Reanim, ReanimTransform};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::xfl::xml_writer::XmlWriter;

const XFL_NS: &str = "http://ns.adobe.com/xfl/2008/";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
const XFL_VERSION: &str = "2.971";

pub struct LayerData {
    pub name: String,
    pub frames: Vec<FrameData>,
}

pub struct FrameData {
    pub index: usize,
    pub has_symbol: bool,
    pub library_item: String,
    pub matrix_a: f64,
    pub matrix_b: f64,
    pub matrix_c: f64,
    pub matrix_d: f64,
    pub matrix_tx: f64,
    pub matrix_ty: f64,
    pub alpha: f64,
}

pub fn encode_xfl(reanim: &Reanim, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;

    let library_dir = output_dir.join("library");
    fs::create_dir_all(&library_dir)?;

    // Write proxy file
    fs::write(output_dir.join("main.xfl"), "PROXY-CS5")?;

    let mut media = Vec::new();
    let mut symbols = Vec::new();
    let mut exist = HashSet::new();

    let mut layers = Vec::new();

    for track in reanim.tracks.iter().rev() {
        let mut default_transform = ReanimTransform {
            x: Some(0.0),
            y: Some(0.0),
            sx: Some(1.0),
            sy: Some(1.0),
            kx: Some(0.0),
            ky: Some(0.0),
            f: Some(0.0),
            a: Some(1.0),
            ..Default::default()
        };

        let mut img_list = Vec::new();
        let mut dom_frames = Vec::new();
        let mut index_counter = 0;

        for (k, trans) in track.transforms.iter().enumerate() {
            if let Some(v) = trans.x {
                default_transform.x = Some(v);
            }
            if let Some(v) = trans.y {
                default_transform.y = Some(v);
            }
            if let Some(v) = trans.kx {
                default_transform.kx = Some(v);
            }
            if let Some(v) = trans.ky {
                default_transform.ky = Some(v);
            }
            if let Some(v) = trans.sx {
                default_transform.sx = Some(v);
            }
            if let Some(v) = trans.sy {
                default_transform.sy = Some(v);
            }
            if let Some(v) = trans.f {
                default_transform.f = Some(v);
            }
            if let Some(v) = trans.a {
                default_transform.a = Some(v);
            }

            if let Some(i_val) = &trans.i {
                let nid = get_name_by_id(i_val, &track.name, index_counter);
                index_counter += 1;

                if exist.insert(nid.clone()) {
                    img_list.push(nid.clone());
                    media.push(nid.clone());
                    symbols.push(nid.clone());
                }
                default_transform.i = Some(nid);
            }

            let dx = 180.0 / std::f64::consts::PI;
            let kx = default_transform.kx.unwrap_or(0.0) as f64;
            let ky = default_transform.ky.unwrap_or(0.0) as f64;
            let skew_x = kx / dx;
            let skew_y = -ky / dx;
            let sx = default_transform.sx.unwrap_or(1.0) as f64;
            let sy = default_transform.sy.unwrap_or(1.0) as f64;

            let a = skew_x.cos() * sx;
            let b = skew_x.sin() * sx;
            let c = skew_y.sin() * sy;
            let d = skew_y.cos() * sy;
            let tx = default_transform.x.unwrap_or(0.0) as f64;
            let ty = default_transform.y.unwrap_or(0.0) as f64;
            let alpha = default_transform.a.unwrap_or(1.0) as f64;
            let f = default_transform.f.unwrap_or(0.0);

            // if i is present and f != -1, symbol is shown
            let has_symbol = default_transform.i.is_some() && (f - (-1.0)).abs() > 0.001;

            dom_frames.push(FrameData {
                index: k,
                has_symbol,
                library_item: default_transform.i.clone().unwrap_or_default(),
                matrix_a: a,
                matrix_b: b,
                matrix_c: c,
                matrix_d: d,
                matrix_tx: tx,
                matrix_ty: ty,
                alpha,
            });
        }

        layers.push(LayerData {
            name: track.name.clone(),
            frames: dom_frames,
        });

        for img_name in img_list {
            write_img_xml(&library_dir.join(format!("{img_name}.xml")), &img_name)?;
        }
    }

    write_dom_document(
        reanim,
        &output_dir.join("DOMDocument.xml"),
        &media,
        &symbols,
        &layers,
    )
    .context("DOMDocument generation failed")?;

    Ok(())
}

fn get_name_by_id(id: &str, _label_name: &str, _label_index: usize) -> String {
    let mut name = id.to_string();
    if name.starts_with("IMAGE_REANIM_") {
        name = name[13..].to_string();
    }
    name.to_lowercase()
}

fn write_img_xml(path: &Path, name: &str) -> Result<()> {
    let file = fs::File::create(path)?;
    let mut w = XmlWriter::new(file);

    w.write_header()?;
    w.start_element(
        "DOMSymbolItem",
        &[("xmlns:xsi", XSI_NS), ("xmlns", XFL_NS), ("name", name)],
    )?;

    w.start_element("timeline", &[])?;
    w.start_element("DOMTimeline", &[("name", name)])?;
    w.start_element("layers", &[])?;
    w.start_element(
        "DOMLayer",
        &[
            ("name", "1"),
            ("color", "#4FFF4F"),
            ("current", "true"),
            ("isSelected", "true"),
        ],
    )?;

    w.start_element("frames", &[])?;
    w.start_element("DOMFrame", &[("index", "0")])?;
    w.start_element("elements", &[])?;
    w.write_element(
        "DOMBitmapInstance",
        &[
            ("isSelected", "true"),
            ("libraryItemName", &format!("{name}.png")),
        ],
        None,
    )?;
    w.end_element("elements")?;
    w.end_element("DOMFrame")?;
    w.end_element("frames")?;

    w.end_element("DOMLayer")?;
    w.end_element("layers")?;
    w.end_element("DOMTimeline")?;
    w.end_element("timeline")?;
    w.end_element("DOMSymbolItem")?;

    Ok(())
}

fn write_dom_document(
    reanim: &Reanim,
    path: &Path,
    media: &[String],
    symbols: &[String],
    layers: &[LayerData],
) -> Result<()> {
    let file = fs::File::create(path)?;
    let mut w = XmlWriter::new(file);

    w.write_header()?;
    w.start_element(
        "DOMDocument",
        &[
            ("xmlns:xsi", XSI_NS),
            ("xmlns", XFL_NS),
            ("frameRate", &reanim.fps.to_string()),
            ("width", "80"),
            ("height", "80"),
            ("xflVersion", XFL_VERSION),
        ],
    )?;

    w.start_element("media", &[])?;
    for item in media {
        w.write_element(
            "DOMBitmapItem",
            &[
                ("name", &format!("{item}.png")),
                ("href", &format!("{item}.png")),
            ],
            None,
        )?;
    }
    w.end_element("media")?;

    w.start_element("symbols", &[])?;
    for sym in symbols {
        w.write_element("Include", &[("href", &format!("{sym}.xml"))], None)?;
    }
    w.end_element("symbols")?;

    w.start_element("timelines", &[])?;
    w.start_element("DOMTimeline", &[("name", "animation")])?;
    w.start_element("layers", &[])?;

    for layer in layers {
        w.start_element("DOMLayer", &[("name", &layer.name)])?;
        w.start_element("frames", &[])?;

        for frame in &layer.frames {
            w.start_element("DOMFrame", &[("index", &frame.index.to_string())])?;
            w.start_element("elements", &[])?;

            if frame.has_symbol {
                w.start_element(
                    "DOMSymbolInstance",
                    &[("libraryItemName", &frame.library_item)],
                )?;
                w.start_element("matrix", &[])?;
                w.write_element(
                    "Matrix",
                    &[
                        ("a", &format_float(frame.matrix_a)),
                        ("b", &format_float(frame.matrix_b)),
                        ("c", &format_float(frame.matrix_c)),
                        ("d", &format_float(frame.matrix_d)),
                        ("tx", &format_float(frame.matrix_tx)),
                        ("ty", &format_float(frame.matrix_ty)),
                    ],
                    None,
                )?;
                w.end_element("matrix")?;

                if (frame.alpha - 1.0).abs() > 0.001 {
                    w.start_element("color", &[])?;
                    w.write_element(
                        "Color",
                        &[("alphaMultiplier", &format_float(frame.alpha))],
                        None,
                    )?;
                    w.end_element("color")?;
                }

                w.end_element("DOMSymbolInstance")?;
            }

            w.end_element("elements")?;
            w.end_element("DOMFrame")?;
        }

        w.end_element("frames")?;
        w.end_element("DOMLayer")?;
    }

    w.end_element("layers")?;
    w.end_element("DOMTimeline")?;
    w.end_element("timelines")?;

    w.end_element("DOMDocument")?;

    Ok(())
}

fn format_float(f: f64) -> String {
    format!("{f:.6}")
}
