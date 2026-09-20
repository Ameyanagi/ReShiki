//! Default ACS structure settings, shared with the exchange worker.
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

#[derive(Debug, Deserialize)]
pub struct DrawingStyle {
    pub name: String,
    pub font_family: String,
    pub bond_length_world: f32,
    pub bond_length_pt: f32,
    pub font_size_pt: f32,
    pub line_width_pt: f32,
    pub bold_width_pt: f32,
    pub margin_width_pt: f32,
    pub hash_spacing_pt: f32,
    pub bond_spacing_ratio: f32,
    pub png_dpi: u32,
}
pub static DEFAULT: LazyLock<DrawingStyle> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../engine/drawing_style.json")).unwrap_or_else(|_| {
        DrawingStyle {
            name: "JACS / ACS".into(),
            font_family: "Arial".into(),
            bond_length_world: 42.,
            bond_length_pt: 14.4,
            font_size_pt: 10.,
            line_width_pt: 0.6,
            bold_width_pt: 2.,
            margin_width_pt: 1.6,
            hash_spacing_pt: 2.5,
            bond_spacing_ratio: 0.18,
            png_dpi: 1200,
        }
    })
});
impl DrawingStyle {
    pub fn points_per_world(&self) -> f32 {
        self.bond_length_pt / self.bond_length_world
    }
    pub fn world(&self, points: f32) -> f32 {
        points / self.points_per_world()
    }
    pub fn font_size(&self) -> f32 {
        self.world(self.font_size_pt)
    }
    pub fn line_width(&self) -> f32 {
        self.world(self.line_width_pt)
    }
    pub fn line_height(&self) -> f32 {
        self.font_size() * 1.2
    }
}

static FONTS: LazyLock<resvg::usvg::fontdb::Database> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    db
});

/// Measure advances in the same font used by the drawing renderers.
pub fn text_width(text: &str, size: f32) -> f32 {
    styled_text_width(text, size, &crate::typography::TextStyle::default())
}

pub fn text_ascent(style: &crate::typography::TextStyle) -> f32 {
    use resvg::usvg::fontdb::{Family, Query};
    FONTS
        .query(&Query {
            families: &[Family::Name(&style.family), Family::SansSerif],
            weight: resvg::usvg::fontdb::Weight(if style.bold { 700 } else { 400 }),
            style: if style.italic {
                resvg::usvg::fontdb::Style::Italic
            } else {
                resvg::usvg::fontdb::Style::Normal
            },
            ..Query::default()
        })
        .and_then(|id| {
            FONTS.with_face_data(id, |bytes, index| {
                let face = ttf_parser::Face::parse(bytes, index).ok()?;
                Some(face.ascender() as f32 / face.units_per_em() as f32 * style.size())
            })
        })
        .flatten()
        .unwrap_or(style.size() * 0.9)
}

pub fn styled_text_width(text: &str, size: f32, style: &crate::typography::TextStyle) -> f32 {
    text.chars().map(|c| glyph_metrics(c, style).1 * size).sum()
}

const SANS_FALLBACKS: &[&str] = &[
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Yu Gothic",
    "Meiryo",
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "Arial",
    "DejaVu Sans",
];

/// Explicit Japanese sans-serif avoids the platform's generic CJK serif fallback.
/// This is an interface preference; chemical labels retain their document style.
pub fn ui_font_family() -> &'static str {
    SANS_FALLBACKS
        .iter()
        .find_map(|name| {
            FONT_NAMES
                .iter()
                .find(|available| *available == name)
                .copied()
        })
        .unwrap_or("Arial")
}

type GlyphKey = (String, bool, bool, char);
type GlyphMetrics = (&'static str, f32);
static GLYPHS: LazyLock<Mutex<HashMap<GlyphKey, GlyphMetrics>>> = LazyLock::new(Default::default);

/// Resolve a font that actually contains the glyph and measure that same face.
/// Cache bounded metrics so repeated scene layout never rereads every font file.
pub fn glyph_metrics(c: char, style: &crate::typography::TextStyle) -> GlyphMetrics {
    use resvg::usvg::fontdb::{Family, Query};
    let key = (style.family.clone(), style.bold, style.italic, c);
    if let Ok(cache) = GLYPHS.lock()
        && let Some(result) = cache.get(&key)
    {
        return *result;
    }
    let result = std::iter::once(style.family.as_str())
        .chain(SANS_FALLBACKS.iter().copied())
        .find_map(|name| {
            let family = FONT_NAMES.iter().find(|n| **n == name).copied()?;
            let id = FONTS.query(&Query {
                families: &[Family::Name(family)],
                weight: resvg::usvg::fontdb::Weight(if style.bold { 700 } else { 400 }),
                style: if style.italic {
                    resvg::usvg::fontdb::Style::Italic
                } else {
                    resvg::usvg::fontdb::Style::Normal
                },
                ..Default::default()
            })?;
            FONTS
                .with_face_data(id, |bytes, index| {
                    let face = ttf_parser::Face::parse(bytes, index).ok()?;
                    let advance = face.glyph_hor_advance(face.glyph_index(c)?)?;
                    Some((family, advance as f32 / face.units_per_em() as f32))
                })
                .flatten()
        })
        .unwrap_or((font_name(&style.family), 0.6));
    if let Ok(mut cache) = GLYPHS.lock() {
        if cache.len() >= 16384 {
            cache.clear();
        }
        cache.insert(key, result);
    }
    result
}

static FONT_NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let names: std::collections::BTreeSet<_> = FONTS
        .faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
        .filter(|n| !n.starts_with('.') && !n.trim().is_empty())
        .collect();
    names
        .into_iter()
        .map(|s| &*Box::leak(s.into_boxed_str()))
        .collect()
});
pub fn font_families() -> &'static [&'static str] {
    &FONT_NAMES
}
pub fn font_name(name: &str) -> &'static str {
    FONT_NAMES
        .iter()
        .find(|n| **n == name)
        .copied()
        .unwrap_or("Arial")
}

/// Outlines for compact scientific symbols, using the same default font as labels.
/// Coordinates use a baseline origin and retain counters as separate contours.
pub fn outline_text(
    text: &str,
    size: f32,
    origin: crate::document::Point,
) -> Vec<crate::graphics::PathCommand> {
    use crate::{document::Point, graphics::PathCommand};
    use resvg::usvg::fontdb::{Family, Query};
    struct Outline {
        commands: Vec<PathCommand>,
        at: Point,
        last: Point,
        scale: f32,
    }
    impl Outline {
        fn p(&self, x: f32, y: f32) -> Point {
            self.at.offset(x * self.scale, -y * self.scale)
        }
    }
    impl ttf_parser::OutlineBuilder for Outline {
        fn move_to(&mut self, x: f32, y: f32) {
            let p = self.p(x, y);
            self.commands.push(PathCommand::Move(p));
            self.last = p;
        }
        fn line_to(&mut self, x: f32, y: f32) {
            let p = self.p(x, y);
            self.commands.push(PathCommand::Line(p));
            self.last = p;
        }
        fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
            let c = self.p(x, y);
            let end = self.p(x1, y1);
            self.commands.push(PathCommand::Cubic(
                self.last
                    .offset((c.x - self.last.x) * 2. / 3., (c.y - self.last.y) * 2. / 3.),
                end.offset((c.x - end.x) * 2. / 3., (c.y - end.y) * 2. / 3.),
                end,
            ));
            self.last = end;
        }
        fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
            let end = self.p(x2, y2);
            self.commands
                .push(PathCommand::Cubic(self.p(x, y), self.p(x1, y1), end));
            self.last = end;
        }
        fn close(&mut self) {
            self.commands.push(PathCommand::Close);
        }
    }
    FONTS
        .query(&Query {
            families: &[Family::Name(&DEFAULT.font_family), Family::SansSerif],
            ..Query::default()
        })
        .and_then(|id| {
            FONTS.with_face_data(id, |bytes, index| {
                let face = ttf_parser::Face::parse(bytes, index).ok()?;
                let mut b = Outline {
                    commands: vec![],
                    at: origin,
                    last: origin,
                    scale: size / face.units_per_em() as f32,
                };
                for c in text.chars() {
                    let glyph = face.glyph_index(c)?;
                    face.outline_glyph(glyph, &mut b);
                    b.at.x += face.glyph_hor_advance(glyph).unwrap_or(0) as f32 * b.scale;
                }
                Some(b.commands)
            })
        })
        .flatten()
        .unwrap_or_default()
}
