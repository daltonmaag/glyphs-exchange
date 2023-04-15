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
struct FontProperties {
    disables_automatic_alignment: bool,
    family_name: String,
    glyph_order: Vec<String>,
    units_per_em: i64,
    version_major: i64,
    version_minor: i64,
}

#[derive(Debug, Clone)]
enum LayerId {
    Master(String),
    AssociatedWithMaster(String, String, String),
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

        let unique_filenames: HashSet<String> = designspace
            .sources
            .iter()
            .map(|source| source.filename.to_string())
            .collect();
        let designspace_dir = designspace_path.parent().unwrap();
        let ufos: HashMap<String, norad::Font> = unique_filenames
            .into_iter()
            .map(|filename| {
                (
                    filename.clone(),
                    norad::Font::load(designspace_dir.join(filename)).expect("Could not load UFO"),
                )
            })
            .collect();

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
                source.layer.clone().unwrap(),
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
        source
            .location
            .iter()
            .map(|dim| {
                let axis = self.axis_by_name(&dim.name);
                let value = Self::map_axis_value_backwards(axis, dim.xvalue.unwrap_or(0.0));
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

    fn map_axis_value_backwards(axis: &designspace::Axis, value: f32) -> f32 {
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
    }

    fn map_axis_value_forwards(axis: &designspace::Axis, value: f32) -> f32 {
        if let Some(mapping) = &axis.map {
            mapping
                .iter()
                .find(|map| map.input == value)
                .map(|map| map.output)
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
    }

    fn default_source(&self) -> &designspace::Source {
        let default_location: Vec<designspace::Dimension> = self
            .designspace
            .axes
            .iter()
            .map(|a| designspace::Dimension {
                name: a.name.clone(),
                xvalue: Some(Self::map_axis_value_forwards(a, a.default)),
                ..Default::default()
            })
            .collect();
        self.designspace
            .sources
            .iter()
            .find(|source| source.location == default_location)
            .expect("Could not find default source")
    }
}

impl FontProperties {
    fn from_context(context: &DesignspaceContext) -> Self {
        let default_source = context.default_source();
        let default_ufo = context.ufos.get(&default_source.filename).unwrap();

        let family_name: String = default_ufo
            .font_info
            .family_name
            .clone()
            .unwrap_or(String::from("New Font"));
        let units_per_em: i64 = default_ufo
            .font_info
            .units_per_em
            .map(|v| v.round() as i64)
            .unwrap_or(1000);
        let version_major: i64 = default_ufo
            .font_info
            .version_major
            .map(|v| v as i64)
            .unwrap_or(1);
        let version_minor: i64 = default_ufo
            .font_info
            .version_minor
            .map(|v| v as i64)
            .unwrap_or(0);
        let disables_automatic_alignment = default_ufo
            .lib
            .get("com.schriftgestaltung.customParameter.GSFont.disablesAutomaticAlignment")
            .map(|v| v.as_boolean().unwrap_or(true))
            .unwrap_or(true);

        let all_glyphs_set: HashSet<String> = default_ufo
            .layers
            .default_layer()
            .iter()
            .map(|glyph| glyph.name().to_string())
            .collect();
        let glyph_order: Vec<String> =
            if let Some(glyph_order) = default_ufo.lib.get("public.glyphOrder") {
                let mut glyph_order: Vec<String> = glyph_order
                    .as_array()
                    .expect("glyphOrder must be list of strings.")
                    .iter()
                    .map(|v| v.as_string().unwrap().to_string())
                    .collect();

                let glyph_order_set = HashSet::from_iter(&glyph_order);
                let mut leftovers: Vec<String> = all_glyphs_set
                    .iter()
                    .collect::<HashSet<&String>>()
                    .difference(&glyph_order_set)
                    .map(|n| n.to_string())
                    .collect();
                leftovers.sort();
                glyph_order.extend(leftovers);

                glyph_order
            } else {
                let mut all_glyphs: Vec<String> = Vec::from_iter(all_glyphs_set);
                all_glyphs.sort();
                all_glyphs
            };

        Self {
            disables_automatic_alignment,
            family_name,
            glyph_order,
            units_per_em,
            version_major,
            version_minor,
        }
    }
}

pub fn command_to_glyphs(designspace_path: &Path) -> glyphs_plist::Font {
    let context = DesignspaceContext::from_path(designspace_path);

    let font_properties = FontProperties::from_context(&context);
    let font_master: Vec<glyphs_plist::FontMaster> = context
        .designspace
        .sources
        .iter()
        .filter(|source| source.layer.is_none())
        .map(|source| master_from(&context, source))
        .collect();
    let instances: Vec<glyphs_plist::Instance> = context
        .designspace
        .instances
        .iter()
        .map(instance_from)
        .collect();

    let mut glyphs: Vec<HashMap<norad::Name, glyphs_plist::Layer>> = context
        .designspace
        .sources
        .iter()
        .map(|source| {
            let layer_id = context.id_for_source_name(source);
            let font = &context.ufos[&source.filename];
            let ufo_layer = match &layer_id {
                LayerId::Master(_) => font.default_layer(),
                LayerId::AssociatedWithMaster(_, _, layer_name) => {
                    font.layers.get(layer_name).unwrap_or_else(|| {
                        panic!("Cannot find layer {} in {}.", layer_name, &source.filename)
                    })
                }
            };
            (layer_id, ufo_layer)
        })
        // NOTE: Running this loop in parallel is not faster, or I'm holding
        // rayon wrong...
        .map(|(layer_id, ufo_layer)| {
            ufo_layer
                .iter()
                .map(|glyph| (glyph.name().clone(), layer_from(&layer_id, glyph)))
                .collect()
        })
        .collect();

    // Glyphs need to be sorted like the glyphOrder.
    let default_source = context.default_source();
    let default_ufo = context.ufos.get(&default_source.filename).unwrap();
    let default_ufo_layer = default_ufo.default_layer();
    let glyphs: Vec<glyphs_plist::Glyph> = font_properties
        .glyph_order
        .iter()
        .filter_map(|name| default_ufo_layer.get_glyph(name))
        .map(|glyph| {
            let mut converted_glyph = new_glyph_from(glyph);
            converted_glyph.layers.extend(
                glyphs
                    .iter_mut()
                    .filter_map(|layers| layers.remove(glyph.name())),
            );
            converted_glyph
        })
        .collect();

    let glyph_order_plist: Vec<Plist> = font_properties
        .glyph_order
        .iter()
        .map(|n| n.to_string().into())
        .collect();
    let other_stuff: HashMap<String, Plist> = hashmap! {
        ".appVersion".into() => String::from("1361").into(),
        "customParameters".into() => vec![
            hashmap! {
                "name".into() => String::from("Axes").into(),
                "value".into() => context.global_axes(),
            }.into(),
            hashmap! {
                "name".into() => String::from("glyphOrder").into(),
                "value".into() => glyph_order_plist.into(),
            }.into(),
        ].into(),
    };

    glyphs_plist::Font {
        disables_automatic_alignment: Some(font_properties.disables_automatic_alignment),
        family_name: font_properties.family_name,
        font_master,
        glyphs,
        instances: Some(instances),
        other_stuff,
        units_per_em: font_properties.units_per_em,
        version_major: font_properties.version_major,
        version_minor: font_properties.version_minor,
    }
}

fn master_from(
    context: &DesignspaceContext,
    source: &designspace::Source,
) -> glyphs_plist::FontMaster {
    let layer_id = context.id_for_source_name(source);
    let font = &context.ufos[&source.filename];

    let LayerId::Master(id) = &layer_id else {
        panic!("Master does not seem to be a master?!")
    };

    let (weight_value, width_value, custom_value, custom_value1, custom_value2, custom_value3) =
        DesignspaceContext::design_location(&source.location);

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

    let source_name = source
        .stylename
        .as_ref()
        .expect("Source must have a stylename");

    let other_stuff = hashmap! {
        "customParameters".into() => vec![
            hashmap! {
                "name".into() => String::from("Axis Location").into(),
                "value".into() => context.axis_location(source),
            }.into(),
            hashmap! {
                "name".into() => String::from("Master Name").into(),
                "value".into() => source_name.to_string().into(),
            }.into(),
        ].into(),
    };

    glyphs_plist::FontMaster {
        ascender: Some(ascender),
        cap_height: Some(cap_height),
        custom_value,
        custom_value1,
        custom_value2,
        custom_value3,
        descender: Some(descender),
        id: id.clone(),
        italic_angle,
        other_stuff,
        weight_value: Some(weight_value),
        width_value,
        x_height: Some(x_height),
    }
}

fn instance_from(instance: &designspace::Instance) -> glyphs_plist::Instance {
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

    glyphs_plist::Instance {
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
    }
}

fn layer_from(layer_id: &LayerId, glyph: &norad::Glyph) -> Layer {
    let (associated_master_id, layer_id, layer_name) = match layer_id {
        LayerId::Master(id) => (None, id.clone(), None),
        LayerId::AssociatedWithMaster(parent_id, child_id, layer_name) => (
            Some(parent_id.clone()),
            child_id.clone(),
            Some(layer_name.clone()),
        ),
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

    let layer = Layer {
        name: layer_name,
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
    };
    layer
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
