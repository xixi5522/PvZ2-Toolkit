use crate::{Reanim, ReanimTrack, ReanimTransform};
use anyhow::{Context, Result, bail};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn decode_xfl(input_dir: &Path) -> Result<Reanim> {
    let dom_doc_path = input_dir.join("DOMDocument.xml");
    let file =
        File::open(&dom_doc_path).with_context(|| format!("Failed to open {dom_doc_path:?}"))?;
    let r = BufReader::new(file);
    let mut reader = Reader::from_reader(r);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut reanim = Reanim {
        fps: 30.0,
        ..Default::default()
    };

    let mut tracks = Vec::new();

    // We will track the hierarchical state to simulate DOM extraction
    let mut in_dom_document = false;
    let mut in_layers = false;

    let mut current_layer_name = String::new();
    let mut current_frames = Vec::new();

    let mut current_duration = 1;
    let mut current_matrix: Option<(f64, f64, f64, f64, f64, f64)> = None;
    let mut current_library_item: Option<String> = None;
    let mut current_alpha: Option<f32> = None;
    let mut in_symbol_instance = false;

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let name = e.name();
                match name.as_ref() {
                    "DOMDocument" => {
                        in_dom_document = true;
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "frameRate" {
                                let val = attr.value.as_ref();
                                reanim.fps = val.parse().unwrap_or(30.0);
                            }
                        }
                    }
                    "DOMTimeline" => {
                        // We do not need to track timelines natively when scraping timelines directly
                        // Sen does parse Animation timelines explicitly, but XFL uses default `animation`.
                    }
                    "layers" => in_layers = true,
                    "DOMLayer" => {
                        if in_layers {
                            let mut name_val = "Unknown".to_string();
                            for attr in e.attributes() {
                                let attr = attr?;
                                if attr.key.as_ref() == "name" {
                                    name_val = attr.value.to_string();
                                }
                            }
                            current_layer_name = name_val;
                            current_frames.clear();
                        }
                    }
                    "DOMFrame" => {
                        current_duration = 1;
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "duration" {
                                let val = attr.value.as_ref();
                                current_duration = val.parse().unwrap_or(1);
                            }
                        }
                        // Reset instance properties
                        current_matrix = None;
                        current_library_item = None;
                        current_alpha = None;
                    }
                    "DOMSymbolInstance" => {
                        in_symbol_instance = true;
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "libraryItemName" {
                                current_library_item = Some(attr.value.to_string());
                            }
                        }
                    }
                    "Matrix" => {
                        if in_symbol_instance {
                            let mut a = 1.0;
                            let mut b = 0.0;
                            let mut c = 0.0;
                            let mut d = 1.0;
                            let mut tx = 0.0;
                            let mut ty = 0.0;
                            for attr in e.attributes() {
                                let attr = attr?;
                                let val_str = attr.value.as_ref();
                                let val = val_str.parse::<f64>().unwrap_or(0.0);
                                match attr.key.as_ref() {
                                    "a" => a = val,
                                    "b" => b = val,
                                    "c" => c = val,
                                    "d" => d = val,
                                    "tx" => tx = val,
                                    "ty" => ty = val,
                                    _ => {}
                                }
                            }
                            current_matrix = Some((a, b, c, d, tx, ty));
                        }
                    }
                    "Color" if in_symbol_instance => {
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "alphaMultiplier" {
                                let val_str = attr.value.as_ref();
                                current_alpha = Some(val_str.parse::<f32>().unwrap_or(1.0));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(ref e) => {
                let name = e.name();
                match name.as_ref() {
                    "DOMFrame" => {
                        // Sometimes DOMFrame is empty if no elements.
                        current_duration = 1;
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "duration" {
                                let val = attr.value.as_ref();
                                current_duration = val.parse().unwrap_or(1);
                            }
                        }
                        current_frames.push(ParsedFrame {
                            duration: current_duration,
                            library_item: None,
                            matrix: None,
                            alpha: None,
                        });
                    }
                    "DOMSymbolInstance" => {
                        let mut lib_name = None;
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "libraryItemName" {
                                lib_name = Some(attr.value.to_string());
                            }
                        }
                        current_library_item = lib_name.clone();
                    }
                    "Matrix" => {
                        if in_symbol_instance {
                            let mut a = 1.0;
                            let mut b = 0.0;
                            let mut c = 0.0;
                            let mut d = 1.0;
                            let mut tx = 0.0;
                            let mut ty = 0.0;
                            for attr in e.attributes() {
                                let attr = attr?;
                                let val_str = attr.value.as_ref();
                                let val = val_str.parse::<f64>().unwrap_or(0.0);
                                match attr.key.as_ref() {
                                    "a" => a = val,
                                    "b" => b = val,
                                    "c" => c = val,
                                    "d" => d = val,
                                    "tx" => tx = val,
                                    "ty" => ty = val,
                                    _ => {}
                                }
                            }
                            current_matrix = Some((a, b, c, d, tx, ty));
                        }
                    }
                    "Color" if in_symbol_instance => {
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == "alphaMultiplier" {
                                let val_str = attr.value.as_ref();
                                current_alpha = Some(val_str.parse::<f32>().unwrap_or(1.0));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref e) => {
                let name = e.name();
                match name.as_ref() {
                    "DOMDocument" => in_dom_document = false,
                    "layers" => in_layers = false,
                    "DOMLayer" => {
                        tracks.push(ReanimTrack {
                            name: current_layer_name.clone(),
                            transforms: convert_parsed_frames_to_transforms(&current_frames),
                        });
                    }
                    "DOMFrame" => {
                        current_frames.push(ParsedFrame {
                            duration: current_duration,
                            library_item: current_library_item.clone(),
                            matrix: current_matrix,
                            alpha: current_alpha,
                        });
                    }
                    "DOMSymbolInstance" => in_symbol_instance = false,
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !in_dom_document && tracks.is_empty() {
        bail!("Invalid DOMDocument or missing tracks");
    }

    // reverse tracks as DOMLayers are top-down visually but bottom-up sequentially natively in Flash.
    tracks.reverse();
    reanim.tracks = tracks;

    Ok(reanim)
}

struct ParsedFrame {
    duration: usize,
    library_item: Option<String>,
    matrix: Option<(f64, f64, f64, f64, f64, f64)>,
    alpha: Option<f32>,
}

fn convert_parsed_frames_to_transforms(frames: &[ParsedFrame]) -> Vec<ReanimTransform> {
    let mut transforms = Vec::new();
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

    let mut temp_name = "-1".to_string();
    let mut frame_switch = false;

    for frame in frames {
        let duration = frame.duration;

        for frame_offset in 0..duration {
            let mut this_transform = ReanimTransform::default();

            if frame.library_item.is_some() || frame.matrix.is_some() {
                if let Some(lib_name) = &frame.library_item
                    && temp_name != *lib_name
                {
                    this_transform.i = Some(lib_name.to_string());
                    temp_name = lib_name.to_string();
                }

                if let Some((a, b, c, d, tx, ty)) = frame.matrix {
                    let skew_x = b.atan2(a);
                    let skew_y = c.atan2(d);
                    let quarter_pi = std::f64::consts::FRAC_PI_4;

                    let sx = if skew_x.abs() < quarter_pi || skew_x.abs() > 3.0 * quarter_pi {
                        a / skew_x.cos()
                    } else {
                        b / skew_x.sin()
                    } as f32;

                    let sy = if skew_y.abs() < quarter_pi || skew_y.abs() > 3.0 * quarter_pi {
                        d / skew_y.cos()
                    } else {
                        c / skew_y.sin()
                    } as f32;

                    let dx = 180.0 / std::f64::consts::PI;
                    let kx = (dx * skew_x) as f32;
                    let ky = (-dx * skew_y) as f32;

                    let tx = tx as f32;
                    let ty = ty as f32;

                    if (default_transform.x.unwrap_or(0.0) - tx).abs() > 0.001 {
                        default_transform.x = Some(tx);
                        this_transform.x = Some(tx);
                    }
                    if (default_transform.y.unwrap_or(0.0) - ty).abs() > 0.001 {
                        default_transform.y = Some(ty);
                        this_transform.y = Some(ty);
                    }
                    if (default_transform.kx.unwrap_or(0.0) - kx).abs() > 0.001 {
                        default_transform.kx = Some(kx);
                        this_transform.kx = Some(kx);
                    }
                    if (default_transform.ky.unwrap_or(0.0) - ky).abs() > 0.001 {
                        default_transform.ky = Some(ky);
                        this_transform.ky = Some(ky);
                    }
                    if (default_transform.sx.unwrap_or(1.0) - sx).abs() > 0.001 {
                        default_transform.sx = Some(sx);
                        this_transform.sx = Some(sx);
                    }
                    if (default_transform.sy.unwrap_or(1.0) - sy).abs() > 0.001 {
                        default_transform.sy = Some(sy);
                        this_transform.sy = Some(sy);
                    }

                    if frame_switch && frame_offset == 0 {
                        this_transform.f = Some(0.0);
                        frame_switch = false;
                    }
                }

                if let Some(alpha) = frame.alpha
                    && (default_transform.a.unwrap_or(1.0) - alpha).abs() > 0.001
                {
                    default_transform.a = Some(alpha);
                    this_transform.a = Some(alpha);
                }

                transforms.push(this_transform);
            } else {
                if !frame_switch && frame_offset == 0 {
                    this_transform.f = Some(-1.0);
                    frame_switch = true;
                }
                transforms.push(this_transform);
            }
        }
    }

    transforms
}
