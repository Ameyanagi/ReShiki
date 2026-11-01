//! Default ACS structure settings, shared with the exchange worker.
pub mod units;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, PoisonError, RwLock, RwLockReadGuard},
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub center_chain_double_bonds: bool,
    pub png_dpi: u32,
}
impl Default for DrawingStyle {
    fn default() -> Self {
        (*DEFAULT).clone()
    }
}
pub static DEFAULT: LazyLock<DrawingStyle> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../assets/drawing_style.json")).unwrap_or_else(|_| {
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
            center_chain_double_bonds: false,
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
        for dimension in units::Dimension::ALL {
            dimension.validate(dimension.get(self), units::Unit::Points)?;
        }
        let spacing = self.bond_spacing_ratio * 100.;
        if !spacing.is_finite() || !(5. ..=40.).contains(&spacing) {
            return Err("Bond spacing must be between 5 and 40%.".into());
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

struct Fonts {
    /// Shared with every export's usvg tree, so parsing an SVG neither scans
    /// the system fonts again nor copies the database.
    db: Arc<resvg::usvg::fontdb::Database>,
    names: &'static [&'static str],
    /// Families of the fonts added at runtime, searched after the usual
    /// fallbacks for glyphs a label's own font lacks.
    added: Vec<&'static str>,
}

static FONTS: LazyLock<RwLock<Fonts>> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    load_fonts(&mut db);
    #[cfg(not(target_arch = "wasm32"))]
    font::keep_resident(&mut db, &[&DEFAULT.font_family]);
    let names = family_names(&db);
    RwLock::new(Fonts {
        db: Arc::new(db),
        names,
        added: Vec::new(),
    })
});

fn fonts() -> RwLockReadGuard<'static, Fonts> {
    FONTS.read().unwrap_or_else(PoisonError::into_inner)
}

/// The fonts text is measured with, for drawing and export: one database for
/// the whole process, so exports reuse it instead of loading their own.
pub fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    fonts().db.clone()
}

/// Fonts a page added at runtime, after the system's or the bundled ones.
static ADDED: Mutex<Vec<resvg::usvg::fontdb::Source>> = Mutex::new(Vec::new());

/// Load the fonts that text is measured and drawn with: the system's on the
/// desktop and the bundled ones in the browser, then any added at runtime.
pub fn load_fonts(db: &mut resvg::usvg::fontdb::Database) {
    #[cfg(not(target_arch = "wasm32"))]
    db.load_system_fonts();
    #[cfg(target_arch = "wasm32")]
    font::load_bundled(db);
    for source in added_fonts(0) {
        db.load_font_source(source);
    }
}

/// Add a TrueType or OpenType font (or collection) for measuring, drawing and
/// export, as the browser does with a page's fonts. Labels whose own family
/// lacks a glyph fall back to it. Returns how many faces it holds.
pub fn add_font(bytes: Vec<u8>) -> Result<usize, String> {
    let source = resvg::usvg::fontdb::Source::Binary(Arc::new(bytes));
    let mut fonts = FONTS.write().unwrap_or_else(PoisonError::into_inner);
    // Trees parsed earlier keep the database they were laid out with.
    let ids = Arc::make_mut(&mut fonts.db).load_font_source(source.clone());
    if ids.is_empty() {
        return Err("Not a TrueType or OpenType font".into());
    }
    fonts.names = family_names(&fonts.db);
    for id in &ids {
        let families = fonts.db.face(*id).map(|face| face.families.clone());
        for (family, _) in families.unwrap_or_default() {
            let name = fonts.names.iter().find(|name| **name == family).copied();
            if let Some(name) = name
                && !fonts.added.contains(&name)
            {
                fonts.added.push(name);
            }
        }
    }
    ADDED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(source);
    let faces = ids.len();
    drop(fonts);
    GLYPHS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    GLYPH_INK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    GLYPH_REACH
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    Ok(faces)
}

/// The fonts added since the first `from`; their count changes with each
/// [`add_font`], so a renderer with its own font database can catch up.
pub fn added_fonts(from: usize) -> Vec<resvg::usvg::fontdb::Source> {
    let added = ADDED.lock().unwrap_or_else(PoisonError::into_inner);
    added.get(from..).unwrap_or_default().to_vec()
}

/// How many fonts were added at runtime; caches of laid-out text compare it.
pub fn font_generation() -> usize {
    ADDED.lock().unwrap_or_else(PoisonError::into_inner).len()
}

/// Whether any loaded face draws `c`.
pub fn has_glyph(c: char) -> bool {
    let fonts = fonts();
    fonts.db.faces().any(|face| {
        fonts
            .db
            .with_face_data(face.id, |bytes, index| {
                ttf_parser::Face::parse(bytes, index)
                    .ok()
                    .and_then(|face| face.glyph_index(c))
            })
            .flatten()
            .is_some()
    })
}

mod font;

#[cfg(test)]
#[path = "style/added_font_tests.rs"]
mod added_font_tests;

#[cfg(test)]
#[path = "style/tests.rs"]
mod tests;

/// Measure advances in the same font used by the drawing renderers.
pub fn text_width(text: &str, size: f32) -> f32 {
    styled_text_width(text, size, &crate::typography::TextStyle::default())
}

pub fn text_ascent(style: &crate::typography::TextStyle) -> f32 {
    use resvg::usvg::fontdb::{Family, Query};
    let fonts = fonts();
    fonts
        .db
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
            fonts.db.with_face_data(id, |bytes, index| {
                let face = font::face(bytes, index, if style.bold { 700 } else { 400 })?;
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
            fonts()
                .names
                .iter()
                .find(|available| *available == name)
                .copied()
        })
        .unwrap_or("Arial")
}

/// The families a label's glyph is looked up in, in order: its own, the
/// sans-serif fallbacks, then the fonts added at runtime.
fn fallback_families<'a>(family: &'a str, fonts: &'a Fonts) -> impl Iterator<Item = &'a str> {
    std::iter::once(family)
        .chain(SANS_FALLBACKS.iter().copied())
        .chain(fonts.added.iter().copied())
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
    let fonts = fonts();
    let result = fallback_families(&style.family, &fonts)
        .find_map(|name| {
            let family = fonts.names.iter().find(|n| **n == name).copied()?;
            let id = fonts.db.query(&Query {
                families: &[Family::Name(family)],
                weight: resvg::usvg::fontdb::Weight(if style.bold { 700 } else { 400 }),
                style: if style.italic {
                    resvg::usvg::fontdb::Style::Italic
                } else {
                    resvg::usvg::fontdb::Style::Normal
                },
                ..Default::default()
            })?;
            fonts
                .db
                .with_face_data(id, |bytes, index| {
                    let face = font::face(bytes, index, if style.bold { 700 } else { 400 })?;
                    let advance = face.glyph_hor_advance(face.glyph_index(c)?)?;
                    Some((family, advance as f32 / face.units_per_em() as f32))
                })
                .flatten()
        })
        .unwrap_or_else(|| {
            let family = fonts.names.iter().find(|n| **n == style.family);
            (family.copied().unwrap_or("Arial"), 0.6)
        });
    drop(fonts);
    if let Ok(mut cache) = GLYPHS.lock() {
        if cache.len() >= 16384 {
            cache.clear();
        }
        cache.insert(key, result);
    }
    result
}

static GLYPH_REACH: LazyLock<Mutex<HashMap<String, Option<f32>>>> = LazyLock::new(Default::default);

/// An upper bound, in ems, on any glyph's advance and ink extent in a label
/// set in `family`, counting the fallback faces its missing glyphs come from.
/// `None` when a face has variation axes: its instances can be wider.
pub fn label_glyph_reach(family: &str) -> Option<f32> {
    if let Ok(cache) = GLYPH_REACH.lock()
        && let Some(reach) = cache.get(family)
    {
        return *reach;
    }
    let fonts = fonts();
    // Glyphs no face has are measured 0.6 em wide and 1 em tall.
    let mut reach = Some(1f32);
    for name in fallback_families(family, &fonts) {
        for face in fonts.db.faces() {
            if !face.families.iter().any(|(n, _)| n == name) {
                continue;
            }
            let extent = fonts
                .db
                .with_face_data(face.id, |bytes, index| {
                    let face = ttf_parser::Face::parse(bytes, index).ok()?;
                    if face.is_variable() {
                        return None;
                    }
                    let advance = (0..face.number_of_glyphs())
                        .filter_map(|g| face.glyph_hor_advance(ttf_parser::GlyphId(g)))
                        .max()
                        .unwrap_or(0);
                    let abs = |v: i16| i32::from(v).unsigned_abs();
                    let b = face.global_bounding_box();
                    // Ink is placed from the ascent down, as in `text_ink_shapes`.
                    let ink = abs(b.x_min)
                        .max(abs(b.x_max))
                        .max(abs(face.ascender()) + abs(b.y_min).max(abs(b.y_max)));
                    Some(u32::from(advance).max(ink) as f32 / f32::from(face.units_per_em()))
                })
                .flatten();
            reach = reach.zip(extent).map(|(r, e)| r.max(e));
        }
    }
    drop(fonts);
    if let Ok(mut cache) = GLYPH_REACH.lock() {
        cache.insert(family.to_owned(), reach);
    }
    reach
}

type InkBox = (crate::document::Point, crate::document::Point);
/// A glyph's ink box and the corner radius (em) its outline leaves free.
type InkShape = (InkBox, f32);
static GLYPH_INK: LazyLock<Mutex<HashMap<GlyphKey, Option<InkShape>>>> =
    LazyLock::new(Default::default);

/// Ink boxes relative to the text's top-left origin, excluding blank line height.
/// Keep individual glyphs separate so superscripts do not mask empty corners.
pub fn text_ink_boxes(text: &str, size: f32, style: &crate::typography::TextStyle) -> Vec<InkBox> {
    text_ink_shapes(text, size, style)
        .into_iter()
        .map(|(bounds, _)| bounds)
        .collect()
}

/// `text_ink_boxes` with each glyph's ink as a rounded rectangle: the largest
/// corner radius (up to half the shorter side) that still holds the outline,
/// so O is nearly round while N and H keep the corners their stems reach.
pub fn text_ink_shapes(
    text: &str,
    size: f32,
    style: &crate::typography::TextStyle,
) -> Vec<InkShape> {
    use crate::document::Point;
    use resvg::usvg::fontdb::{Family, Query};
    let mut x = 0.;
    let mut shapes = Vec::new();
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
            let fonts = fonts();
            let ink = fonts
                .db
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
                    fonts.db.with_face_data(id, |bytes, index| {
                        let face = font::face(bytes, index, if style.bold { 700 } else { 400 })?;
                        let glyph = face.glyph_index(c)?;
                        let bbox = face.glyph_bounding_box(glyph)?;
                        let em = face.units_per_em() as f32;
                        let ascent = face.ascender() as f32;
                        let mut outline = font::Outline::default();
                        face.outline_glyph(glyph, &mut outline);
                        Some((
                            (
                                Point::new(
                                    bbox.x_min as f32 / em,
                                    (ascent - bbox.y_max as f32) / em,
                                ),
                                Point::new(
                                    bbox.x_max as f32 / em,
                                    (ascent - bbox.y_min as f32) / em,
                                ),
                            ),
                            outline.corner_radius(bbox) / em,
                        ))
                    })
                })
                .flatten()
                .or_else(|| {
                    (!c.is_whitespace())
                        .then_some(((Point::default(), Point::new(advance, 1.)), 0.))
                });
            if let Ok(mut cache) = GLYPH_INK.lock() {
                if cache.len() >= 16384 {
                    cache.clear();
                }
                cache.insert(key, ink);
            }
            ink
        };
        if let Some(((lo, hi), radius)) = ink {
            shapes.push((
                (
                    Point::new(x + lo.x * size, lo.y * size),
                    Point::new(x + hi.x * size, hi.y * size),
                ),
                radius * size,
            ));
        }
        if style.underline {
            // Underline metrics differ between backends. Reserve its full band.
            shapes.push((
                (
                    Point::new(x, size * 0.8),
                    Point::new(x + advance * size, size * 1.1),
                ),
                0.,
            ));
        }
        x += advance * size;
    }
    shapes
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

fn family_names(db: &resvg::usvg::fontdb::Database) -> &'static [&'static str] {
    let names: std::collections::BTreeSet<_> = db
        .faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
        .filter(|n| !n.starts_with('.') && !n.trim().is_empty())
        .collect();
    // Leaked once at start and once per added font.
    names
        .into_iter()
        .map(|s| &*Box::leak(s.into_boxed_str()))
        .collect::<Vec<_>>()
        .leak()
}
pub fn font_families() -> &'static [&'static str] {
    fonts().names
}
pub fn font_name(name: &str) -> &'static str {
    fonts()
        .names
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
    let fonts = fonts();
    fonts
        .db
        .query(&Query {
            families: &[Family::Name(&DEFAULT.font_family), Family::SansSerif],
            ..Query::default()
        })
        .and_then(|id| {
            fonts.db.with_face_data(id, |bytes, index| {
                let face = font::face(bytes, index, 400)?;
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
