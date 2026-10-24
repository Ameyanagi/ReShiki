/// Resolve the requested weight before reading advances, bounds or outlines.
/// A variable font's built-in default can be Thin even for a normal CSS request.
/// Static faces retain their existing metrics because they have no weight axis.
pub(super) fn face(bytes: &[u8], index: u32, weight: u16) -> Option<ttf_parser::Face<'_>> {
    let mut face = ttf_parser::Face::parse(bytes, index).ok()?;
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), f32::from(weight));
    Some(face)
}

/// Read the files behind `families` (and the generic sans-serif) once and keep
/// them in memory. A face backed only by its path maps the file again for
/// every glyph lookup, which dominated export time; labels rarely use others.
/// The crates forbid `unsafe`, so this reads rather than shares a mapping.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn keep_resident(db: &mut resvg::usvg::fontdb::Database, families: &[&str]) {
    use resvg::usvg::fontdb::{Family, Source};
    let sans = db.family_name(&Family::SansSerif).to_owned();
    let ids: Vec<_> = db
        .faces()
        .filter(|face| {
            face.families
                .iter()
                .any(|(name, _)| *name == sans || families.contains(&name.as_str()))
        })
        .map(|face| face.id)
        .collect();
    let mut files = std::collections::HashMap::new();
    for id in ids {
        let Some(mut face) = db.face(id).cloned() else {
            continue;
        };
        let Source::File(path) = &face.source else {
            continue;
        };
        let data = files
            .entry(path.clone())
            .or_insert_with(|| std::fs::read(path).ok().map(std::sync::Arc::new))
            .clone();
        let Some(data) = data else {
            continue;
        };
        face.source = Source::Binary(data);
        // The face takes back the slot it just left, so faces keep their
        // order and every query resolves as before.
        db.remove_face(id);
        db.push_face_info(face);
    }
}

/// The browser has no system fonts. Liberation Sans has Arial's metrics, so
/// its faces also answer to Arial, the default label family.
/// Source and license: licenses/liberation-fonts.
#[cfg(target_arch = "wasm32")]
pub(super) fn load_bundled(db: &mut resvg::usvg::fontdb::Database) {
    use resvg::usvg::fontdb::{Language, Source};
    const FACES: [&[u8]; 4] = [
        include_bytes!("../../fonts/LiberationSans-Regular.ttf"),
        include_bytes!("../../fonts/LiberationSans-Bold.ttf"),
        include_bytes!("../../fonts/LiberationSans-Italic.ttf"),
        include_bytes!("../../fonts/LiberationSans-BoldItalic.ttf"),
    ];
    for bytes in FACES {
        for id in db.load_font_source(Source::Binary(std::sync::Arc::new(bytes))) {
            if let Some(mut face) = db.face(id).cloned() {
                db.remove_face(id);
                face.families
                    .insert(0, ("Arial".into(), Language::English_UnitedStates));
                db.push_face_info(face);
            }
        }
    }
}

/// Points along a glyph outline, in font units, with curves sampled.
#[derive(Default)]
pub(super) struct Outline {
    points: Vec<(f32, f32)>,
}

impl Outline {
    fn last(&self) -> (f32, f32) {
        self.points.last().copied().unwrap_or_default()
    }

    /// The largest corner radius, up to half the shorter side, whose rounded
    /// rectangle over `bbox` still holds every outline point.
    pub(super) fn corner_radius(&self, bbox: ttf_parser::Rect) -> f32 {
        let (x0, y0, x1, y1) = (
            f32::from(bbox.x_min),
            f32::from(bbox.y_min),
            f32::from(bbox.x_max),
            f32::from(bbox.y_max),
        );
        let mut radius = 0.5 * (x1 - x0).min(y1 - y0);
        for &(x, y) in &self.points {
            let (left, right) = ((x - x0).max(0.), (x1 - x).max(0.));
            let (low, high) = ((y - y0).max(0.), (y1 - y).max(0.));
            for (u, v) in [(left, low), (right, low), (left, high), (right, high)] {
                // A point u, v in from a corner lies inside the rounded
                // rectangle exactly when the radius is at most this.
                radius = radius.min(u + v + (2. * u * v).sqrt());
            }
        }
        radius.max(0.)
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.points.push((x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.points.push((x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        for i in 1..=8 {
            let t = i as f32 / 8.;
            let (a, b, c) = ((1. - t) * (1. - t), 2. * t * (1. - t), t * t);
            self.points
                .push((a * x0 + b * x1 + c * x, a * y0 + b * y1 + c * y));
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        for i in 1..=8 {
            let t = i as f32 / 8.;
            let s = 1. - t;
            let (a, b, c, d) = (s * s * s, 3. * s * s * t, 3. * s * t * t, t * t * t);
            self.points.push((
                a * x0 + b * x1 + c * x2 + d * x,
                a * y0 + b * y1 + c * y2 + d * y,
            ));
        }
    }
    fn close(&mut self) {}
}

#[cfg(test)]
#[path = "font/tests.rs"]
mod tests;
