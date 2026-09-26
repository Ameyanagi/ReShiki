//! Default ACS structure settings, shared with the exchange worker.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
impl Default for DrawingStyle {
    fn default() -> Self {
        (*DEFAULT).clone()
    }
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
    pub fn is_default(&self) -> bool {
        self == &*DEFAULT
    }
    pub fn text_style(&self) -> crate::typography::TextStyle {
        crate::typography::TextStyle {
            family: self.font_family.clone(),
            size_pt: self.font_size_pt,
            ..Default::default()
        }
    }
    pub fn set_bond_length(&mut self, points: f32) {
        self.bond_length_pt = points;
        self.bond_length_world = self.world(points);
    }
    pub fn validate(&self) -> Result<(), String> {
        self.text_style().validate()?;
        if self.name.trim().is_empty() || self.name.len() > 120 {
            return Err("Give the drawing style a name of 1–120 characters.".into());
        }
        for (name, value, min, max) in [
            ("Bond length", self.bond_length_pt, 5., 100.),
            ("Line width", self.line_width_pt, 0.1, 6.),
            ("Bold width", self.bold_width_pt, 0.1, 12.),
            ("Label margin", self.margin_width_pt, 0., 12.),
            ("Hash spacing", self.hash_spacing_pt, 0.3, 12.),
            ("Bond spacing", self.bond_spacing_ratio * 100., 5., 40.),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("{name} must be between {min} and {max}."));
            }
        }
        if self.bold_width_pt < self.line_width_pt {
            return Err("Bold width must be at least the line width.".into());
        }
        if !self.bond_length_world.is_finite()
            || (self.bond_length_world - self.world(self.bond_length_pt)).abs() > 0.001
        {
            return Err("Drawing style uses incompatible coordinate units.".into());
        }
        if self.png_dpi != 1200 {
            return Err("Drawing styles currently use 1200 dpi PNG output.".into());
        }
        Ok(())
    }
    pub fn points_per_world(&self) -> f32 {
        // Document coordinates retain physical size when style defaults change.
        14.4 / 42.
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

type InkBox = (crate::document::Point, crate::document::Point);
static GLYPH_INK: LazyLock<Mutex<HashMap<GlyphKey, Option<InkBox>>>> =
    LazyLock::new(Default::default);

/// Ink boxes relative to the text's top-left origin, excluding blank line height.
/// Keep individual glyphs separate so superscripts do not mask empty corners.
pub fn text_ink_boxes(text: &str, size: f32, style: &crate::typography::TextStyle) -> Vec<InkBox> {
    use crate::document::Point;
    use resvg::usvg::fontdb::{Family, Query};
    let mut x = 0.;
    let mut boxes = Vec::new();
    for c in text.chars() {
        let (family, advance) = glyph_metrics(c, style);
        let key = (family.to_owned(), style.bold, style.italic, c);
        let cached = GLYPH_INK
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).copied());
        let ink = if let Some(ink) = cached {
            ink
        } else {
            let ink = FONTS
                .query(&Query {
                    families: &[Family::Name(family)],
                    weight: resvg::usvg::fontdb::Weight(if style.bold { 700 } else { 400 }),
                    style: if style.italic {
                        resvg::usvg::fontdb::Style::Italic
                    } else {
                        resvg::usvg::fontdb::Style::Normal
                    },
                    ..Default::default()
                })
                .and_then(|id| {
                    FONTS.with_face_data(id, |bytes, index| {
                        let face = ttf_parser::Face::parse(bytes, index).ok()?;
                        let glyph = face.glyph_index(c)?;
                        let bbox = face.glyph_bounding_box(glyph)?;
                        let em = face.units_per_em() as f32;
                        let ascent = face.ascender() as f32;
                        Some((
                            Point::new(bbox.x_min as f32 / em, (ascent - bbox.y_max as f32) / em),
                            Point::new(bbox.x_max as f32 / em, (ascent - bbox.y_min as f32) / em),
                        ))
                    })
                })
                .flatten()
                .or_else(|| {
                    (!c.is_whitespace()).then_some((Point::default(), Point::new(advance, 1.)))
                });
            if let Ok(mut cache) = GLYPH_INK.lock() {
                if cache.len() >= 16384 {
                    cache.clear();
                }
                cache.insert(key, ink);
            }
            ink
        };
        if let Some((lo, hi)) = ink {
            boxes.push((
                Point::new(x + lo.x * size, lo.y * size),
                Point::new(x + hi.x * size, hi.y * size),
            ));
        }
        if style.underline {
            // Underline metrics differ between backends. Reserve its full band.
            boxes.push((
                Point::new(x, size * 0.8),
                Point::new(x + advance * size, size * 1.1),
            ));
        }
        x += advance * size;
    }
    boxes
}

/// Center the visible chemical symbol, rather than its line-height box.
pub fn label_vertical_center(text: &str, size: f32, style: &crate::typography::TextStyle) -> f32 {
    let mut font = style.clone();
    font.underline = false;
    let boxes = text_ink_boxes(text, size, &font);
    let top = boxes.iter().map(|(lo, _)| lo.y).reduce(f32::min);
    let bottom = boxes.iter().map(|(_, hi)| hi.y).reduce(f32::max);
    match (top, bottom) {
        (Some(top), Some(bottom)) => (top + bottom) / 2.,
        _ => size / 2.,
    }
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
