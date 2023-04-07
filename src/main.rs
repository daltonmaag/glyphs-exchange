use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use glyphstool::{Layer, Plist, ToPlist};
use norad::designspace::DesignSpaceDocument;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Ufo2glyphs {
        /// Source Designspace to convert.
        #[arg(required = true)]
        designspace_path: PathBuf,

        /// The path to the Glyphs.app file to write (default: next to the input
        /// Designspace).
        output_path: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ufo2glyphs {
            designspace_path,
            output_path,
        } => {
            let context = DesignspaceContext::from_path(&designspace_path);
            let glyphs_font = convert_ufos_to_glyphs(&context);

            let output_path =
                output_path.unwrap_or_else(|| designspace_path.with_extension("glyphs"));
            let plist = glyphs_font.to_plist();
            fs::write(output_path, plist.to_string()).unwrap();
        }
    }
}

#[derive(Debug)]
struct DesignspaceContext {
    designspace: DesignSpaceDocument,
    ufos: HashMap<String, norad::Font>,
}

impl DesignspaceContext {
    fn from_path(designspace_path: &Path) -> Self {
        let designspace = norad::designspace::DesignSpaceDocument::load(&designspace_path)
            .expect("Cannot load Designspace.");
        let unique_filenames: HashSet<String> = HashSet::from_iter(
            designspace
                .sources
                .iter()
                .map(|source| source.filename.to_string()),
        );
        let designspace_dir = designspace_path.parent().unwrap();
        let ufos = HashMap::from_iter(unique_filenames.into_iter().map(|filename| {
            (
                filename.clone(),
                norad::Font::load(designspace_dir.join(filename)).expect("Could not load UFO"),
            )
        }));

        Self { designspace, ufos }
    }
}

fn convert_ufos_to_glyphs(context: &DesignspaceContext) -> glyphstool::Font {
    let mut glyphs: HashMap<String, glyphstool::Glyph> = HashMap::new();
    let mut font_master: Vec<glyphstool::FontMaster> = Vec::new();
    let mut other_stuff: HashMap<String, Plist> = HashMap::new();

    let mut family_name: Option<String> = None;
    let mut units_per_em: Option<i64> = None;
    let mut version_major: Option<i64> = None;
    let mut version_minor: Option<i64> = None;

    let mut glyph_order: Option<Vec<String>> = None;

    for source in context.ufos.values() {
        if let (None, Some(source_family_name)) = (&family_name, &source.font_info.family_name) {
            family_name.replace(source_family_name.clone());
        }
        if let (None, Some(source_units_per_em)) = (&units_per_em, &source.font_info.units_per_em) {
            units_per_em.replace(source_units_per_em.round() as i64);
        }
        if let (None, Some(source_version_major)) =
            (&version_major, &source.font_info.version_major)
        {
            version_major.replace(*source_version_major as i64);
        }
        if let (None, Some(source_version_minor)) =
            (&version_minor, &source.font_info.version_minor)
        {
            version_minor.replace(*source_version_minor as i64);
        }

        if let (None, Some(Some(source_glyph_order))) = (
            &glyph_order,
            source.lib.get("public.glyphOrder").map(|v| v.as_array()),
        ) {
            glyph_order.replace(
                source_glyph_order
                    .iter()
                    .map(|v| {
                        v.as_string()
                            .expect("glyphOrder must be list of strings.")
                            .to_string()
                    })
                    .collect(),
            );
        }

        let id = uuid::Uuid::new_v4().to_string();
        let weight_value: i64 = source.font_info.open_type_os2_weight_class.unwrap_or(400) as i64;
        let mut other_stuff: HashMap<String, Plist> = HashMap::new();

        let layer_name = source.font_info.style_name.clone();
        let ascender = source
            .font_info
            .ascender
            .map(|v| v.round() as i64)
            .unwrap_or(800);
        let cap_height = source
            .font_info
            .cap_height
            .map(|v| v.round() as i64)
            .unwrap_or(700);
        let descender = source
            .font_info
            .descender
            .map(|v| v.round() as i64)
            .unwrap_or(-200);
        let x_height = source
            .font_info
            .x_height
            .map(|v| v.round() as i64)
            .unwrap_or(500);

        if let Some(layer_name) = layer_name {
            other_stuff.insert("custom".into(), layer_name.into());
        }
        other_stuff.insert("ascender".into(), ascender.into());
        other_stuff.insert("capHeight".into(), cap_height.into());
        other_stuff.insert("descender".into(), descender.into());
        other_stuff.insert("xHeight".into(), x_height.into());

        font_master.push(glyphstool::FontMaster {
            id: id.clone(),
            weight_value,
            width_value: None,
            other_stuff,
        });

        for glyph in source.layers.default_layer().iter() {
            let converted_glyph = glyphs.entry(glyph.name().to_string()).or_insert_with(|| {
                let mut other_stuff: HashMap<String, Plist> = Default::default();
                if !glyph.codepoints.is_empty() {
                    other_stuff.insert(
                        "unicode".into(),
                        glyph
                            .codepoints
                            .iter()
                            .map(|c| format!("{:04X}", c as usize))
                            .collect::<Vec<_>>()
                            .join(",")
                            .into(),
                    );
                }

                glyphstool::Glyph {
                    layers: Default::default(),
                    glyphname: glyph.name().to_string(),
                    other_stuff,
                }
            });

            let layer_id = id.clone();
            let width = glyph.width;
            let mut paths: Vec<glyphstool::Path> = Vec::new();
            let mut components: Vec<glyphstool::Component> = Vec::new();
            let mut anchors: Vec<glyphstool::Anchor> = Vec::new();
            let guide_lines = None;
            let other_stuff: HashMap<String, Plist> = HashMap::new();

            for contour in glyph.contours.iter() {
                let mut nodes: Vec<glyphstool::Node> = contour
                    .points
                    .iter()
                    .map(|point| glyphstool::Node {
                        pt: kurbo::Point::new(point.x, point.y),
                        node_type: match (&point.typ, point.smooth) {
                            (norad::PointType::Move, _) => glyphstool::NodeType::Line,
                            (norad::PointType::Line, true) => glyphstool::NodeType::LineSmooth,
                            (norad::PointType::Line, false) => glyphstool::NodeType::Line,
                            (norad::PointType::OffCurve, _) => glyphstool::NodeType::OffCurve,
                            (norad::PointType::Curve, true) => glyphstool::NodeType::CurveSmooth,
                            (norad::PointType::Curve, false) => glyphstool::NodeType::Curve,
                            (norad::PointType::QCurve, true) => todo!(),
                            (norad::PointType::QCurve, false) => todo!(),
                        },
                    })
                    .collect();
                if contour.is_closed() {
                    nodes.rotate_left(1);
                }

                paths.push(glyphstool::Path {
                    closed: contour.is_closed(),
                    nodes,
                });
            }

            for component in glyph.components.iter() {
                components.push(glyphstool::Component {
                    name: component.base.to_string(),
                    transform: if component.transform == Default::default() {
                        None
                    } else {
                        Some(kurbo::Affine::new([
                            component.transform.x_scale,
                            component.transform.xy_scale,
                            component.transform.yx_scale,
                            component.transform.y_scale,
                            component.transform.x_offset,
                            component.transform.y_offset,
                        ]))
                    },
                    other_stuff: Default::default(),
                })
            }

            for anchor in glyph.anchors.iter() {
                if let Some(name) = &anchor.name {
                    anchors.push(glyphstool::Anchor {
                        name: name.to_string(),
                        position: kurbo::Point::new(anchor.x, anchor.y),
                    });
                }
            }

            converted_glyph.layers.push(Layer {
                layer_id,
                width,
                paths: if !paths.is_empty() { Some(paths) } else { None },
                components: if !components.is_empty() {
                    Some(components)
                } else {
                    None
                },
                anchors: if !anchors.is_empty() {
                    Some(anchors)
                } else {
                    None
                },
                guide_lines,
                other_stuff,
            });
        }
    }

    other_stuff.insert(".appVersion".into(), String::from("1361").into());
    other_stuff.insert(
        "familyName".into(),
        family_name.unwrap_or(String::from("New Font")).into(),
    );
    other_stuff.insert("unitsPerEm".into(), units_per_em.unwrap_or(1000).into());
    other_stuff.insert("versionMajor".into(), version_major.unwrap_or(1).into());
    other_stuff.insert("versionMinor".into(), version_minor.unwrap_or(0).into());

    let mut custom_parameters: Vec<Plist> = Vec::new();
    if let Some(glyph_order) = &glyph_order {
        let glyph_order_plist: Vec<Plist> =
            glyph_order.iter().map(|n| n.to_string().into()).collect();
        let glyph_order_plist = HashMap::from([
            ("name".into(), String::from("glyphOrder").into()),
            ("value".into(), glyph_order_plist.into()),
        ]);
        custom_parameters.push(glyph_order_plist.into());
    }
    if !custom_parameters.is_empty() {
        other_stuff.insert("customParameters".into(), custom_parameters.into());
    }

    // Glyphs need to be sorted like the glyphOrder.
    let glyphs = if let Some(mut glyph_order) = glyph_order {
        let all_glyphs: HashSet<&String> = HashSet::from_iter(glyphs.keys());
        let ordered_glyphs: HashSet<&String> = HashSet::from_iter(&glyph_order);
        let mut leftovers: Vec<String> = all_glyphs
            .difference(&ordered_glyphs)
            .map(|n| n.to_string())
            .collect();
        leftovers.sort();
        glyph_order.extend(leftovers);

        let mut glyphs_sorted = Vec::new();
        for glyph_name in glyph_order {
            if let Some(glyph) = glyphs.remove(&glyph_name) {
                glyphs_sorted.push(glyph);
            }
        }
        glyphs_sorted
    } else {
        // Random order :)
        glyphs.into_values().collect::<Vec<_>>()
    };

    glyphstool::Font {
        glyphs,
        font_master,
        other_stuff,
    }
}
