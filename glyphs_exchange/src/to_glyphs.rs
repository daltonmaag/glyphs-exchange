use std::collections::{HashMap, HashSet};
use std::path::Path;

use maplit::hashmap;
use norad::designspace;

use glyphs_plist;
use glyphs_plist::{Layer, Plist};

#[derive(Debug)]
struct DesignspaceContext {
    designspace: designspace::DesignSpaceDocument,
    ufos: HashMap<String, norad::Font>,
    ids: HashMap<String, String>,
}

#[derive(Debug)]
enum LayerId {
    Master(String),
    AssociatedWithMaster(String, String),
}

type DesignLocation = (
    i64,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

type InstanceLocation = (
    f64,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

impl DesignspaceContext {
    fn from_path(designspace_path: &Path) -> Self {
        let designspace = designspace::DesignSpaceDocument::load(designspace_path)
            .expect("Cannot load Designspace.");

        // Check that all sources have unique names, otherwise panic.
        let unique_sources: HashSet<_> = designspace
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect();
        if unique_sources.len() != designspace.sources.len() {
            panic!("Designspace sources must have unique names.");
        }

        // Check that we have at most six axes (Glyphs.app v2.x limitation).
        if designspace.axes.len() > 6 {
            panic!("Designspace must have at most six axes.");
        }

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
        let ids = designspace
            .sources
            .iter()
            .map(|source| {
                (
                    source.name.clone(),
                    uuid::Uuid::new_v4().to_string().to_uppercase(),
                )
            })
            .collect();

        Self {
            designspace,
            ufos,
            ids,
        }
    }

    fn id_for_source_name(&self, source: &designspace::Source) -> LayerId {
        if source.layer.is_none() {
            LayerId::Master(self.ids[&source.name].clone())
        } else {
            let parent_source = self
                .designspace
                .sources
                .iter()
                .find(|parent_source| parent_source.filename == source.filename)
                .expect("Parent source not found in Designspace.");
            LayerId::AssociatedWithMaster(
                self.ids[&parent_source.name].clone(),
                self.ids[&source.name].clone(),
            )
        }
    }

    // TODO: Fix reliance on the order of dimensions in the location.
    fn design_location(location: &[designspace::Dimension]) -> DesignLocation {
        let location_at = |i: usize| {
            location
                .get(i)
                .map(|dim| dim.xvalue.unwrap_or(0.0).round() as i64)
        };
        (
            location_at(0).unwrap_or(0),
            location_at(1),
            location_at(2),
            location_at(3),
            location_at(4),
            location_at(5),
        )
    }

    fn design_location_float(location: &[designspace::Dimension]) -> InstanceLocation {
        let location_at = |i: usize| location.get(i).map(|dim| dim.xvalue.unwrap_or(0.0) as f64);
        (
            location_at(0).unwrap_or(0.0),
            location_at(1),
            location_at(2),
            location_at(3),
            location_at(4),
            location_at(5),
        )
    }

    fn axis_by_name(&self, name: &str) -> &designspace::Axis {
        self.designspace
            .axes
            .iter()
            .find(|axis| axis.name == name)
            .expect("Cannot find axis by name")
    }

    // TODO: Fix reliance on the order of dimensions in the location and axes.
    fn axis_location(&self, source: &designspace::Source) -> Plist {
        let map_backwards = |axis: &designspace::Axis, value: f32| {
            if let Some(mapping) = &axis.map {
                mapping
                    .iter()
                    .find(|map| map.output == value)
                    .map(|map| map.input)
                    .ok_or_else(|| {
                        format!(
                            "Could not find exact axis design to user mapping; axis {}, value {}",
                            &axis.name, value
                        )
                    })
                    .unwrap()
            } else {
                value
            }
        };

        source
            .location
            .iter()
            .map(|dim| {
                let axis = self.axis_by_name(&dim.name);
                let value = map_backwards(axis, dim.xvalue.unwrap_or(0.0));
                Plist::Dictionary(
                    vec![
                        ("Axis".to_string(), Plist::String(axis.name.clone())),
                        ("Location".to_string(), Plist::Integer(value.round() as i64)),
                    ]
                    .into_iter()
                    .collect(),
                )
            })
            .collect::<Vec<_>>()
            .into()
    }

    fn global_axes(&self) -> Plist {
        self.designspace
            .axes
            .iter()
            .map(|axis| {
                Plist::Dictionary(
                    vec![
                        ("Name".to_string(), Plist::String(axis.name.clone())),
                        ("Tag".to_string(), Plist::String(axis.tag.clone())),
                    ]
                    .into_iter()
                    .collect(),
                )
            })
            .collect::<Vec<_>>()
            .into()
    }
}

pub fn command_to_glyphs(designspace_path: &Path) -> glyphs_plist::Font {
    let context = DesignspaceContext::from_path(designspace_path);

    let mut glyphs: HashMap<String, glyphs_plist::Glyph> = HashMap::new();
    let mut font_master: Vec<glyphs_plist::FontMaster> = Vec::new();
    let mut other_stuff: HashMap<String, Plist> = HashMap::new();

    let mut family_name: Option<String> = None;
    let mut units_per_em: Option<i64> = None;
    let mut version_major: Option<i64> = None;
    let mut version_minor: Option<i64> = None;

    let mut disables_automatic_alignment = None;
    let mut glyph_order: Option<Vec<String>> = None;

    for source in context.designspace.sources.iter() {
        let layer_id = context.id_for_source_name(source);
        let font = &context.ufos[&source.filename];

        if source.layer.is_none() {
            if let (None, Some(source_family_name)) = (&family_name, &font.font_info.family_name) {
                family_name.replace(source_family_name.clone());
            }
            if let (None, Some(source_units_per_em)) = (&units_per_em, &font.font_info.units_per_em)
            {
                units_per_em.replace(source_units_per_em.round() as i64);
            }
            if let (None, Some(source_version_major)) =
                (&version_major, &font.font_info.version_major)
            {
                version_major.replace(*source_version_major as i64);
            }
            if let (None, Some(source_version_minor)) =
                (&version_minor, &font.font_info.version_minor)
            {
                version_minor.replace(*source_version_minor as i64);
            }

            if let (None, Some(source_disables_automatic_alignment)) = (
                disables_automatic_alignment,
                font.lib
                    .get("com.schriftgestaltung.customParameter.GSFont.disablesAutomaticAlignment"),
            ) {
                disables_automatic_alignment
                    .replace(source_disables_automatic_alignment.as_boolean().unwrap());
            }

            if let (None, Some(Some(source_glyph_order))) = (
                &glyph_order,
                font.lib.get("public.glyphOrder").map(|v| v.as_array()),
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

            let LayerId::Master(id) = &layer_id else {
                panic!("Master does not seem to be a master?!")
            };
            let (
                weight_value,
                width_value,
                custom_value,
                custom_value1,
                custom_value2,
                custom_value3,
            ) = DesignspaceContext::design_location(&source.location);

            let mut other_stuff: HashMap<String, Plist> = HashMap::new();

            let ascender = font
                .font_info
                .ascender
                .map(|v| v.round() as i64)
                .unwrap_or(800);
            let cap_height = font
                .font_info
                .cap_height
                .map(|v| v.round() as i64)
                .unwrap_or(700);
            let descender = font
                .font_info
                .descender
                .map(|v| v.round() as i64)
                .unwrap_or(-200);
            let x_height = font
                .font_info
                .x_height
                .map(|v| v.round() as i64)
                .unwrap_or(500);
            let italic_angle = font.font_info.italic_angle.map(|v| -v);

            other_stuff.insert("ascender".into(), ascender.into());
            other_stuff.insert("capHeight".into(), cap_height.into());
            other_stuff.insert("descender".into(), descender.into());
            other_stuff.insert("xHeight".into(), x_height.into());

            let mut custom_parameters: Vec<Plist> = Vec::new();
            custom_parameters.push(
                hashmap! {
                    "name".into() => String::from("Axis Location").into(),
                    "value".into() => context.axis_location(source),
                }
                .into(),
            );
            // The "Master Name" custom parameter is the only place where it
            // stays safe, because Glyphs leaves out fields in FontMaster it
            // thinks it can regenerate. GlyphsLib uses the style name rather
            // than source name for it.
            let source_name = source
                .stylename
                .as_ref()
                .expect("Source must have a stylename");
            custom_parameters.push(
                hashmap! {
                    "name".into() => String::from("Master Name").into(),
                    "value".into() => source_name.to_string().into(),
                }
                .into(),
            );
            other_stuff.insert("customParameters".into(), custom_parameters.into());

            font_master.push(glyphs_plist::FontMaster {
                id: id.clone(),
                italic_angle,
                weight_value: Some(weight_value),
                width_value,
                custom_value,
                custom_value1,
                custom_value2,
                custom_value3,
                other_stuff,
            });
        }

        let ufo_layer = if source.layer.is_some() {
            font.layers
                .get(source.layer.as_ref().unwrap())
                .expect("Cannot find layer.")
        } else {
            font.default_layer()
        };

        for glyph in ufo_layer.iter() {
            let converted_glyph = glyphs
                .entry(glyph.name().to_string())
                .or_insert_with(|| new_glyph_from(glyph));

            let (associated_master_id, layer_id) = match &layer_id {
                LayerId::Master(id) => (None, id.clone()),
                LayerId::AssociatedWithMaster(parent_id, child_id) => {
                    (Some(parent_id.clone()), child_id.clone())
                }
            };

            let paths: Vec<glyphs_plist::Path> = glyph
                .contours
                .iter()
                .map(|contour| contour.into())
                .collect();

            let components: Vec<glyphs_plist::Component> = glyph
                .components
                .iter()
                .map(|component| component.into())
                .collect();

            let anchors: Vec<glyphs_plist::Anchor> = glyph
                .anchors
                .iter()
                .filter(|anchor| anchor.name.is_some())
                .map(|anchor| anchor.into())
                .collect();

            converted_glyph.layers.push(Layer {
                name: source.layer.clone(),
                associated_master_id,
                layer_id,
                width: glyph.width,
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
                guide_lines: None,
                other_stuff: Default::default(),
            });
        }
    }

    let mut instances: Vec<glyphs_plist::Instance> = Vec::new();
    for instance in context.designspace.instances.iter() {
        let name = instance.stylename.clone().unwrap_or_default();
        let (
            interpolation_weight,
            interpolation_width,
            interpolation_custom,
            interpolation_custom1,
            interpolation_custom2,
            interpolation_custom3,
        ) = DesignspaceContext::design_location_float(&instance.location);

        // TODO: make norad::designspace use proper ufo type
        let (is_bold, is_italic) = match &instance.stylemapstylename {
            Some(style) => match style.as_str() {
                "regular" => (false, false),
                "bold" => (true, false),
                "italic" => (false, true),
                "bold italic" => (true, true),
                _ => panic!("Unrecognized style map style name"),
            },
            None => (false, false),
        };

        let link_style = instance.stylemapfamilyname.clone();
        let other_stuff: HashMap<String, Plist> = HashMap::new();

        instances.push(glyphs_plist::Instance {
            name,
            interpolation_weight: Some(interpolation_weight),
            interpolation_width,
            interpolation_custom,
            interpolation_custom1,
            interpolation_custom2,
            interpolation_custom3,
            is_bold: Some(is_bold),
            is_italic: Some(is_italic),
            link_style,
            other_stuff,
        })
    }

    other_stuff.insert(".appVersion".into(), String::from("1361").into());

    let mut custom_parameters: Vec<Plist> = Vec::new();
    custom_parameters.push(
        hashmap! {
            "name".into() => String::from("Axes").into(),
            "value".into() => context.global_axes(),
        }
        .into(),
    );
    if let Some(glyph_order) = &glyph_order {
        let glyph_order_plist: Vec<Plist> =
            glyph_order.iter().map(|n| n.to_string().into()).collect();
        let glyph_order_plist = HashMap::from([
            ("name".into(), String::from("glyphOrder").into()),
            ("value".into(), glyph_order_plist.into()),
        ]);
        custom_parameters.push(glyph_order_plist.into());
    }
    other_stuff.insert("customParameters".into(), custom_parameters.into());

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

    if disables_automatic_alignment.is_none() {
        disables_automatic_alignment = Some(true);
    }

    glyphs_plist::Font {
        glyphs,
        font_master,
        other_stuff,
        disables_automatic_alignment,
        instances: Some(instances),
        family_name: family_name.unwrap_or(String::from("New Font")).into(),
        version_major: version_major.unwrap_or(1).into(),
        version_minor: version_minor.unwrap_or(0).into(),
        units_per_em: units_per_em.unwrap_or(1000),
    }
}

fn new_glyph_from(glyph: &norad::Glyph) -> glyphs_plist::Glyph {
    glyphs_plist::Glyph {
        unicode: Some(glyph.codepoints.clone()),
        glyphname: glyph.name().clone(),
        layers: Default::default(),
        other_stuff: Default::default(),
        left_kerning_group: None,
        right_kerning_group: None,
    }
}
