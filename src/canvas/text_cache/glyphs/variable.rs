// Adapted from cosmic-text 0.15.0 swash_outline_commands (MIT).
// Copyright (c) 2022 System76. See licenses/cosmic-text/NOTICE and LICENSE.
use super::text::cosmic_text::{CacheKey, CacheKeyFlags, FontSystem, SwashCache};
use swash::scale::ScaleContext;
use swash::zeno::{Angle, PathData as _, Transform};

/// Prepare the nine Iced weights before any GUI paragraph or editor is shaped.
/// cosmic-text's family fallback requires an exact metadata weight, otherwise a
/// variable face's OS/2 default (often 100) can hide it from normal/bold requests.
/// Query all weights before mutation and restrict each alias to the family names
/// that selected its physical face. Mixed families can also need static aliases:
/// these preserve CSS weight selection when variable aliases add exact matches.
/// Static-only families and every physical font program remain unchanged.
pub(super) fn prepare_system_weights(fonts: &mut FontSystem) -> usize {
    use super::text::cosmic_text::fontdb;
    use std::collections::{HashMap, HashSet};
    let db = fonts.db();
    // Parse each original face once, rather than once per requested weight.
    let variable_ids: HashSet<_> = db
        .faces()
        .filter_map(|info| {
            db.with_face_data(info.id, |bytes, index| {
                ttf_parser::Face::parse(bytes, index)
                    .ok()
                    .is_some_and(|face| {
                        face.variation_axes()
                            .into_iter()
                            .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wght"))
                    })
            })
            .unwrap_or(false)
            .then_some(info.id)
        })
        .collect();
    let mut prepared = HashMap::new();
    let mut aliases: Vec<fontdb::FaceInfo> = Vec::new();
    for info in db.faces().filter(|info| variable_ids.contains(&info.id)) {
        for (family, _) in &info.families {
            for weight in [100, 200, 300, 400, 500, 600, 700, 800, 900] {
                let Some(id) = db.query(&fontdb::Query {
                    families: &[fontdb::Family::Name(family)],
                    weight: fontdb::Weight(weight),
                    style: info.style,
                    stretch: info.stretch,
                }) else {
                    continue;
                };
                let Some(selected) = db.face(id) else {
                    continue;
                };
                if selected.weight.0 == weight {
                    continue;
                }
                let index = *prepared.entry((id, weight)).or_insert_with(|| {
                    let index = aliases.len();
                    let mut alias = selected.clone();
                    alias.id = fontdb::ID::dummy();
                    alias.weight = fontdb::Weight(weight);
                    alias.families.clear();
                    aliases.push(alias);
                    index
                });
                let Some(alias) = aliases.get_mut(index) else {
                    continue;
                };
                for name in selected.families.iter().filter(|(name, _)| name == family) {
                    if !alias.families.contains(name) {
                        alias.families.push(name.clone());
                    }
                }
            }
        }
    }
    let count = aliases.len();
    if !aliases.is_empty() {
        let db = fonts.db_mut();
        for alias in aliases {
            db.push_face_info(alias);
        }
    }
    count
}

/// cosmic-text 0.15 applies the requested weight when shaping and rasterizing,
/// but its outline scaler leaves a variable face at the built-in default.
/// Populate the same bounded cache using the exact shaped face and weight.
/// Static fonts keep cosmic-text's original path, including bitmap fallback.
pub(super) fn cache_outline(
    fonts: &mut FontSystem,
    cache: &mut SwashCache,
    context: &mut ScaleContext,
    key: CacheKey,
) {
    let Some(font) = fonts.get_font(key.font_id, key.font_weight) else {
        return;
    };
    let face = font.as_swash();
    let weight_tag = swash::Tag::from_be_bytes(*b"wght");
    let Some(axis) = face.variations().find_by_tag(weight_tag) else {
        return;
    };
    let mut scaler = context
        .builder(face)
        .size(f32::from_bits(key.font_size_bits))
        .hint(!key.flags.contains(CacheKeyFlags::DISABLE_HINTING))
        .variations([swash::Setting {
            tag: weight_tag,
            value: f32::from(key.font_weight.0).clamp(axis.min_value(), axis.max_value()),
        }])
        .build();
    let commands = scaler
        .scale_outline(key.glyph_id)
        .or_else(|| scaler.scale_color_outline(key.glyph_id))
        .map(|mut outline| {
            if key.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
                outline.transform(&Transform::skew(
                    Angle::from_degrees(14.0),
                    Angle::from_degrees(0.0),
                ));
            }
            outline.path().commands().collect()
        });
    cache.outline_command_cache.insert(key, commands);
}

#[cfg(test)]
mod tests;
