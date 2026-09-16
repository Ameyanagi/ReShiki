//! Default ACS structure settings, shared with the exchange worker.
use serde::Deserialize;
use std::sync::LazyLock;

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
    serde_json::from_str(include_str!("../engine/drawing_style.json"))
        .expect("valid bundled drawing style")
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
    use resvg::usvg::fontdb::{Family, Query};
    FONTS
        .query(&Query {
            families: &[Family::Name(&DEFAULT.font_family), Family::SansSerif],
            ..Query::default()
        })
        .and_then(|id| {
            FONTS.with_face_data(id, |bytes, index| {
                let face = ttf_parser::Face::parse(bytes, index).ok()?;
                let advance: u32 = text
                    .chars()
                    .map(|c| {
                        face.glyph_index(c)
                            .and_then(|g| face.glyph_hor_advance(g))
                            .unwrap_or(face.units_per_em() / 2) as u32
                    })
                    .sum();
                Some(advance as f32 / face.units_per_em() as f32 * size)
            })
        })
        .flatten()
        .unwrap_or(text.chars().count() as f32 * size * 0.6)
}
