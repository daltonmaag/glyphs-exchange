use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use norad::{designspace, Glyph};
use rayon::prelude::*;

#[derive(Debug)]
struct Glyphs2DesignspaceContext {
    font: glyphs_plist::Font,
    /// Mapping of relative filename to layer IDs and sparse layer names.
    ufo_mapping: HashMap<String, HashSet<String>>,
}

impl Glyphs2DesignspaceContext {
    fn from_paths(glyphs_path: &Path, designspace_path: &Path) -> Self {
        let font = glyphs_plist::Font::load(glyphs_path).expect("Cannot load Glyphs file");
        let designspace = designspace::DesignSpaceDocument::load(designspace_path)
            .expect("Cannot load Designspace");

        // NOTE: The hashset contains layer IDs and sparse layer names, because why not.
        let mut ufo_mapping: HashMap<String, HashSet<String>> = HashMap::new();
        for source in &designspace.sources {
            if let Some(layer_name) = &source.layer {
                ufo_mapping
                    .entry(source.filename.clone())
                    .or_default()
                    .insert(layer_name.clone());
            } else {
                let glyphs_master = font
                    .font_master
                    .iter()
                    .find(|m| {
                        m.name()
                            == source
                                .stylename
                                .as_ref()
                                .expect("Designspace sources must have a style name")
                    })
                    .expect("Cannot find matching Glyphs master for source");
                ufo_mapping
                    .entry(source.filename.clone())
                    .or_default()
                    .insert(glyphs_master.id.clone());
            }
        }

        Self { font, ufo_mapping }
    }
}

pub fn command_to_designspace(glyphs_path: &Path, designspace_path: &Path) {
    let context = Glyphs2DesignspaceContext::from_paths(glyphs_path, designspace_path);

    context
        .ufo_mapping
        .into_par_iter()
        .for_each(|(ufo_path, layer_ids)| {
            let ufo_path = designspace_path.parent().unwrap().join(ufo_path);
            let mut ufo = norad::Font::load(&ufo_path).expect("Cannot load UFO");

            for glyph in context.font.glyphs.iter() {
                for layer in glyph.layers.iter() {
                    let ufo_layer = if let Some(layer_name) = &layer.name {
                        if !layer_ids.contains(layer_name) {
                            continue;
                        }
                        ufo.layers
                            .get_mut(layer_name)
                            .expect("Can't find layer in UFO")
                    } else {
                        if !layer_ids.contains(&layer.layer_id) {
                            continue;
                        }
                        ufo.layers.default_layer_mut()
                    };

                    let ufo_glyph = ufo_layer
                        .get_glyph_mut(glyph.name())
                        .expect("Can't find glyph in UFO");
                    let converted_glyph = convert_glyphs_glyph_to_ufo_glyph(layer);
                    ufo_glyph.anchors = converted_glyph.anchors;
                    ufo_glyph.contours = converted_glyph.contours;
                    ufo_glyph.components = converted_glyph.components;
                }
            }

            // Save the UFO, but preserve the metainfo.plist, because it's
            // uninteresting and changing it increases git noise.
            let metainfo_path = ufo_path.join("metainfo.plist");
            let metainfo = fs::read(&metainfo_path).expect("Cannot read metainfo.plist");
            ufo.save(&ufo_path).expect("Cannot save UFO");
            fs::write(metainfo_path, metainfo).expect("Cannot write metainfo.plist");

            run_ufonormalizer(&ufo_path)
                .map_err(|e| format!("ufonormalizer failed on {}: {:?}", ufo_path.display(), e))
                .unwrap();
        });
}

fn convert_glyphs_glyph_to_ufo_glyph(layer: &glyphs_plist::Layer) -> norad::Glyph {
    let mut ufo_glyph = Glyph::new("converted_glyph");

    // TODO: Figure out height: only interesting if there is a vertical origin?
    ufo_glyph.width = layer.width;

    if let Some(anchors) = &layer.anchors {
        for anchor in anchors.iter() {
            ufo_glyph.anchors.push(norad::Anchor::new(
                anchor.position.x,
                anchor.position.y,
                Some(norad::Name::new(&anchor.name).expect("Cannot convert anchor name")),
                None,
                None,
                None,
            ));
        }
    }

    if let Some(paths) = &layer.paths {
        for path in paths.iter() {
            let mut points: Vec<norad::ContourPoint> = path
                .nodes
                .iter()
                .map(|node| {
                    let (typ, smooth) = match &node.node_type {
                        glyphs_plist::NodeType::Curve => (norad::PointType::Curve, false),
                        glyphs_plist::NodeType::CurveSmooth => (norad::PointType::Curve, true),
                        glyphs_plist::NodeType::Line => (norad::PointType::Line, false),
                        glyphs_plist::NodeType::LineSmooth => (norad::PointType::Line, true),
                        glyphs_plist::NodeType::OffCurve => (norad::PointType::OffCurve, false),
                    };
                    norad::ContourPoint::new(node.pt.x, node.pt.y, typ, smooth, None, None, None)
                })
                .collect();
            if !path.closed {
                assert!(points[0].typ == norad::PointType::Line && !points[0].smooth);
                points[0].typ = norad::PointType::Move;
            } else {
                // In Glyphs.app, the starting node of a closed contour is
                // always stored at the end of the nodes list.
                points.rotate_right(1);
            }

            ufo_glyph
                .contours
                .push(norad::Contour::new(points, None, None));
        }
    }

    if let Some(components) = &layer.components {
        for component in components.iter() {
            let transform = if let Some(transform) = component.transform {
                let coeffs = transform.as_coeffs();
                norad::AffineTransform {
                    x_scale: coeffs[0],
                    xy_scale: coeffs[1],
                    yx_scale: coeffs[2],
                    y_scale: coeffs[3],
                    x_offset: coeffs[4],
                    y_offset: coeffs[5],
                }
            } else {
                norad::AffineTransform::default()
            };

            let ufo_component = norad::Component::new(
                norad::Name::new(&component.name).expect("Can't convert component name"),
                transform,
                None,
                None,
            );

            ufo_glyph.components.push(ufo_component);
        }
    }

    ufo_glyph
}

fn run_ufonormalizer(ufo_path: &Path) -> Result<(), std::io::Error> {
    use std::process::Command;

    match Command::new("ufonormalizer")
        .arg("-m")
        .arg(ufo_path)
        .output()
    {
        Ok(_) => (),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("ufonormalizer not found, skipping normalization");
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}
