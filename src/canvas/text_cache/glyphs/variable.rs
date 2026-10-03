// Adapted from cosmic-text 0.15.0 swash_outline_commands (MIT).
// Copyright (c) 2022 System76. See licenses/cosmic-text/NOTICE and LICENSE.
use super::text::cosmic_text::{CacheKey, CacheKeyFlags, FontSystem, SwashCache};
use swash::scale::ScaleContext;
use swash::zeno::{Angle, PathData as _, Transform};

/// cosmic-text's family fallback requires an exact metadata weight. A variable
/// face's OS/2 default (often 100) otherwise hides it from normal/bold requests.
/// Alias only the selected variable face at the requested weight; source bytes,
/// collection index, style and family are unchanged. Iced has nine weights, so
/// each installed face can acquire at most nine aliases for this process.
pub(super) fn prepare_weight(fonts: &mut FontSystem, attrs: &super::text::cosmic_text::Attrs<'_>) {
    use super::text::cosmic_text::fontdb;
    let db = fonts.db();
    let Some(id) = db.query(&fontdb::Query {
        families: &[attrs.family],
        weight: attrs.weight,
        stretch: attrs.stretch,
        style: attrs.style,
    }) else {
        return;
    };
    let Some(info) = db.face(id) else {
        return;
    };
    if info.weight == attrs.weight {
        return;
    }
    let has_weight_axis = db
        .with_face_data(id, |bytes, index| {
            ttf_parser::Face::parse(bytes, index)
                .ok()
                .is_some_and(|face| {
                    face.variation_axes()
                        .into_iter()
                        .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wght"))
                })
        })
        .unwrap_or(false);
    if !has_weight_axis {
        return;
    }
    let mut alias = info.clone();
    alias.id = fontdb::ID::dummy();
    alias.weight = attrs.weight;
    fonts.db_mut().push_face_info(alias);
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
mod tests {
    use super::super::text::cosmic_text::{Command, fontdb};
    use super::*;

    const TT: &[u8] = include_bytes!(
        "../../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf"
    );
    const CFF2: &[u8] = include_bytes!(
        "../../../../tests/fixtures/font-export-103/cff2-variable-default-100.subset.otf"
    );

    fn font_system(bytes: &[u8]) -> (FontSystem, fontdb::ID) {
        let mut db = fontdb::Database::new();
        db.load_font_data(bytes.to_vec());
        let id = db.faces().next().unwrap().id;
        (FontSystem::new_with_locale_and_db("en-US".into(), db), id)
    }

    fn outline(bytes: &[u8], weight: u16, c: char, flags: CacheKeyFlags) -> Vec<Command> {
        let (mut fonts, id) = font_system(bytes);
        let glyph_id = ttf_parser::Face::parse(bytes, 0)
            .unwrap()
            .glyph_index(c)
            .unwrap()
            .0;
        let key = CacheKey::new(id, glyph_id, 1000., (0., 0.), fontdb::Weight(weight), flags).0;
        let mut cache = SwashCache::new();
        cache_outline(&mut fonts, &mut cache, &mut ScaleContext::new(), key);
        cache
            .get_outline_commands(&mut fonts, key)
            .unwrap()
            .to_vec()
    }

    fn points(commands: &[Command]) -> Vec<(f32, f32)> {
        let mut points = Vec::new();
        for command in commands {
            match command {
                Command::MoveTo(p) | Command::LineTo(p) => points.push((p.x, p.y)),
                Command::QuadTo(a, p) => points.extend([(a.x, a.y), (p.x, p.y)]),
                Command::CurveTo(a, b, p) => points.extend([(a.x, a.y), (b.x, b.y), (p.x, p.y)]),
                Command::Close => (),
            }
        }
        points
    }

    #[test]
    fn variable_canvas_ink_matches_independent_weight_controls() {
        for (variable, normal, bold) in [
            (
                TT,
                include_bytes!("../../../../tests/fixtures/font-export-103/static-400.subset.ttf")
                    .as_slice(),
                include_bytes!("../../../../tests/fixtures/font-export-103/static-700.subset.ttf")
                    .as_slice(),
            ),
            (
                CFF2,
                include_bytes!(
                    "../../../../tests/fixtures/font-export-103/cff-static-400.subset.otf"
                )
                .as_slice(),
                include_bytes!(
                    "../../../../tests/fixtures/font-export-103/cff-static-700.subset.otf"
                )
                .as_slice(),
            ),
        ] {
            for (weight, reference) in [(400, normal), (700, bold)] {
                for c in "HNO".chars() {
                    let actual = outline(variable, weight, c, CacheKeyFlags::DISABLE_HINTING);
                    let expected =
                        if variable == CFF2 {
                            // Static CFF instances cumulatively round relative deltas.
                            // Compare against the committed independent unrounded oracle.
                            include_str!(
                            "../../../../tests/fixtures/font-export-103/cff2-outline-controls.tsv"
                        )
                        .lines()
                        .filter_map(|line| {
                            let mut fields = line.split_whitespace();
                            let w: u16 = fields.next()?.parse().ok()?;
                            let character = fields.next()?.chars().next()?;
                            let _kind = fields.next()?;
                            if w == weight && character == c {
                                Some(fields.map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>())
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .collect::<Vec<_>>()
                        .chunks_exact(2)
                        .map(|p| (p[0], p[1]))
                        .collect()
                        } else {
                            let expected =
                                outline(reference, weight, c, CacheKeyFlags::DISABLE_HINTING);
                            assert_eq!(actual.len(), expected.len());
                            points(&expected)
                        };
                    let actual = points(&actual);
                    assert_eq!(actual.len(), expected.len());
                    for ((ax, ay), (ex, ey)) in actual.into_iter().zip(expected) {
                        assert!(
                            (ax - ex).abs() <= 1.01 && (ay - ey).abs() <= 1.01,
                            "{weight}/{c}: ({ax}, {ay}) != ({ex}, {ey})"
                        );
                    }
                }
            }
            let thin = outline(variable, 100, 'H', CacheKeyFlags::empty());
            let normal = outline(variable, 400, 'H', CacheKeyFlags::empty());
            let bold = outline(variable, 700, 'H', CacheKeyFlags::empty());
            assert_ne!(points(&thin), points(&normal));
            assert_ne!(points(&normal), points(&bold));
        }
    }

    #[test]
    fn variable_family_selection_keeps_exact_source_and_requested_weight() {
        use super::super::text::cosmic_text::{Attrs, Buffer, Metrics, Shaping};
        for bytes in [TT, CFF2] {
            let (mut fonts, original_id) = font_system(bytes);
            let family = fonts.db().face(original_id).unwrap().families[0].0.clone();
            let mut distractors = fontdb::Database::new();
            for data in [
                include_bytes!("../../../../tests/fixtures/font-export-103/static-400.subset.ttf")
                    .as_slice(),
                include_bytes!("../../../../tests/fixtures/font-export-103/static-700.subset.ttf")
                    .as_slice(),
            ] {
                distractors.load_font_data(data.to_vec());
            }
            for info in distractors.faces() {
                let mut info = info.clone();
                info.families[0].0 = "Fallback fixture".into();
                fonts.db_mut().push_face_info(info);
            }
            fonts.db_mut().set_sans_serif_family("Fallback fixture");
            let initial_count = fonts.db().len();
            for weight in [400, 700] {
                let attrs = Attrs::new()
                    .family(fontdb::Family::Name(&family))
                    .weight(fontdb::Weight(weight));
                prepare_weight(&mut fonts, &attrs);
                let count = fonts.db().len();
                prepare_weight(&mut fonts, &attrs);
                assert_eq!(fonts.db().len(), count, "Alias creation must be idempotent");
                let mut buffer = Buffer::new(&mut fonts, Metrics::new(1000., 1200.));
                buffer.set_text(&mut fonts, "HNO", &attrs, Shaping::Advanced, None);
                buffer.shape_until_scroll(&mut fonts, false);
                let glyphs: Vec<_> = buffer.layout_runs().flat_map(|run| run.glyphs).collect();
                assert_eq!(glyphs.len(), 3);
                let mut reference = ttf_parser::Face::parse(bytes, 0).unwrap();
                reference.set_variation(ttf_parser::Tag::from_bytes(b"wght"), f32::from(weight));
                for (glyph, c) in glyphs.into_iter().zip("HNO".chars()) {
                    let font = fonts.get_font(glyph.font_id, glyph.font_weight).unwrap();
                    assert_eq!(
                        font.data(),
                        bytes,
                        "A same-weight fallback is not the requested variable family"
                    );
                    assert_eq!(fonts.db().face(glyph.font_id).unwrap().index, 0);
                    assert_eq!(glyph.font_weight.0, weight);
                    let expected = reference
                        .glyph_hor_advance(reference.glyph_index(c).unwrap())
                        .unwrap();
                    assert!((glyph.w - f32::from(expected)).abs() <= 0.51);
                }
            }
            assert_eq!(fonts.db().len(), initial_count + 2);
        }
    }

    #[test]
    fn static_family_selection_and_outlines_remain_unchanged() {
        use super::super::text::cosmic_text::Attrs;
        let bytes =
            include_bytes!("../../../../tests/fixtures/font-export-103/static-400.subset.ttf");
        let (mut fonts, id) = font_system(bytes);
        let family = fonts.db().face(id).unwrap().families[0].0.clone();
        let attrs = Attrs::new().family(fontdb::Family::Name(&family));
        prepare_weight(&mut fonts, &attrs);
        assert_eq!(fonts.db().len(), 1);
        for size in [8., 14., 28., 59.08, 120.] {
            let glyph_id = ttf_parser::Face::parse(bytes, 0)
                .unwrap()
                .glyph_index('H')
                .unwrap()
                .0;
            let key = CacheKey::new(
                id,
                glyph_id,
                size,
                (0., 0.),
                attrs.weight,
                CacheKeyFlags::empty(),
            )
            .0;
            let mut original = SwashCache::new();
            let expected = original
                .get_outline_commands(&mut fonts, key)
                .unwrap()
                .to_vec();
            let mut adapted = SwashCache::new();
            cache_outline(&mut fonts, &mut adapted, &mut ScaleContext::new(), key);
            assert!(adapted.outline_command_cache.is_empty());
            assert_eq!(
                points(adapted.get_outline_commands(&mut fonts, key).unwrap()),
                points(&expected)
            );
        }
    }

    #[test]
    fn variable_canvas_weights_clamp_and_preserve_synthetic_italic() {
        for bytes in [TT, CFF2] {
            let flags = CacheKeyFlags::DISABLE_HINTING;
            assert_eq!(
                points(&outline(bytes, 1, 'H', flags)),
                points(&outline(bytes, 100, 'H', flags))
            );
            assert_eq!(
                points(&outline(bytes, 1000, 'H', flags)),
                points(&outline(bytes, 900, 'H', flags))
            );
            let normal = points(&outline(bytes, 700, 'H', flags));
            let italic = points(&outline(
                bytes,
                700,
                'H',
                flags | CacheKeyFlags::FAKE_ITALIC,
            ));
            assert_ne!(normal, italic);
            for ((_, y), (_, italic_y)) in normal.into_iter().zip(italic) {
                assert_eq!(y, italic_y);
            }
        }
    }
}
