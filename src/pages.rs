//! Physical publication pages, independent of viewport zoom and molecular scale.
use crate::{
    document::{Document, Point},
    style::DEFAULT as STYLE,
};
use serde::{Deserialize, Serialize};

pub const GAP_PT: f32 = 18.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    A4,
    A5,
    Letter,
    Legal,
    Custom,
}
impl Preset {
    pub const ALL: [Self; 5] = [Self::A4, Self::A5, Self::Letter, Self::Legal, Self::Custom];
    pub fn size(self) -> Option<(f32, f32)> {
        match self {
            Self::A4 => Some((210. * 72. / 25.4, 297. * 72. / 25.4)),
            Self::A5 => Some((148. * 72. / 25.4, 210. * 72. / 25.4)),
            Self::Letter => Some((612., 792.)),
            Self::Legal => Some((612., 1008.)),
            Self::Custom => None,
        }
    }
}
impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::A4 => "A4",
            Self::A5 => "A5",
            Self::Letter => "US Letter",
            Self::Legal => "US Legal",
            Self::Custom => "Custom",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
impl Default for Margins {
    fn default() -> Self {
        Self {
            top: 36.,
            right: 36.,
            bottom: 36.,
            left: 36.,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margins: Margins,
    pub columns: u8,
    pub rows: u8,
    /// Top-left corner of page 1 in drawing coordinates.
    pub origin: Point,
}
impl Default for Layout {
    fn default() -> Self {
        let (width_pt, height_pt) = Preset::A4.size().unwrap_or((595.276, 841.89));
        Self {
            width_pt,
            height_pt,
            margins: Margins::default(),
            columns: 1,
            rows: 1,
            origin: Point::default(),
        }
    }
}
impl Layout {
    pub fn around(doc: &Document) -> Self {
        let mut layout = Self::default();
        let (lo, hi) = crate::scene::selection_bounds(doc, &doc.all_ids())
            .unwrap_or((Point::default(), Point::default()));
        layout.origin = Point::new(
            (lo.x + hi.x - STYLE.world(layout.width_pt)) / 2.,
            (lo.y + hi.y - STYLE.world(layout.height_pt)) / 2.,
        );
        layout
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.width_pt.is_finite()
            || !self.height_pt.is_finite()
            || !(36. ..=2880.).contains(&self.width_pt)
            || !(36. ..=2880.).contains(&self.height_pt)
        {
            return Err("Paper width and height must be between 12.7 and 1016 mm.".into());
        }
        if !(1..=10).contains(&self.columns) || !(1..=10).contains(&self.rows) {
            return Err("Use 1–10 page columns and 1–10 page rows.".into());
        }
        let m = self.margins;
        if [m.top, m.right, m.bottom, m.left]
            .iter()
            .any(|n| !n.is_finite() || *n < 0.)
            || m.left + m.right > self.width_pt - 12.
            || m.top + m.bottom > self.height_pt - 12.
        {
            return Err("Margins must leave at least 12 pt of drawing space on each axis.".into());
        }
        if !self.origin.x.is_finite()
            || !self.origin.y.is_finite()
            || self.origin.x.abs() > 10_000_000.
            || self.origin.y.abs() > 10_000_000.
        {
            return Err("Page origin is outside the supported drawing area.".into());
        }
        Ok(())
    }
    pub fn count(&self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }
    pub fn preset(&self) -> Preset {
        Preset::ALL
            .into_iter()
            .find(|p| {
                p.size().is_some_and(|(w, h)| {
                    ((w - self.width_pt).abs() < 0.1 && (h - self.height_pt).abs() < 0.1)
                        || ((h - self.width_pt).abs() < 0.1 && (w - self.height_pt).abs() < 0.1)
                })
            })
            .unwrap_or(Preset::Custom)
    }
    pub fn bounds(&self, index: usize) -> Option<(Point, Point)> {
        if self.validate().is_err() || index >= self.count() {
            return None;
        }
        let columns = usize::from(self.columns);
        let lo = self.origin.offset(
            (index % columns) as f32 * STYLE.world(self.width_pt + GAP_PT),
            (index / columns) as f32 * STYLE.world(self.height_pt + GAP_PT),
        );
        Some((
            lo,
            lo.offset(STYLE.world(self.width_pt), STYLE.world(self.height_pt)),
        ))
    }
    pub fn content_bounds(&self, index: usize) -> Option<(Point, Point)> {
        let (lo, hi) = self.bounds(index)?;
        Some((
            lo.offset(
                STYLE.world(self.margins.left),
                STYLE.world(self.margins.top),
            ),
            hi.offset(
                -STYLE.world(self.margins.right),
                -STYLE.world(self.margins.bottom),
            ),
        ))
    }
    pub fn spread_bounds(&self) -> Option<(Point, Point)> {
        Some((
            self.bounds(0)?.0,
            self.bounds(self.count().checked_sub(1)?)?.1,
        ))
    }
    pub fn center(&self, doc: &mut Document, ids: &[u64], index: usize) -> Result<(), String> {
        self.validate()?;
        let (a, b) = self
            .content_bounds(index)
            .ok_or("Choose an existing page.")?;
        // Move whole molecules and integral groups; centering must not stretch a bond.
        let mut selected = doc.expand_integral_groups(ids);
        loop {
            let previous = selected.len();
            for bond in &doc.bonds {
                if selected.contains(&bond.a) || selected.contains(&bond.b) {
                    for id in [bond.a, bond.b] {
                        if !selected.contains(&id) {
                            selected.push(id);
                        }
                    }
                }
            }
            if previous == selected.len() {
                break;
            }
        }
        let (lo, hi) = crate::scene::selection_bounds(doc, &selected)
            .ok_or("Select content to center on the page.")?;
        let before = doc.clone();
        doc.translate(
            &selected,
            (a.x + b.x - lo.x - hi.x) / 2.,
            (a.y + b.y - lo.y - hi.y) / 2.,
        );
        if let Err(error) = doc.validate() {
            *doc = before;
            return Err(error);
        }
        Ok(())
    }
    /// Visible bounds are conservative: a mark outside an edge may be clipped.
    pub fn overflow(&self, doc: &Document) -> usize {
        if self.validate().is_err() {
            return 0;
        }
        let pages: Vec<_> = (0..self.count()).filter_map(|i| self.bounds(i)).collect();
        let padding = STYLE.world(4.);
        let outside = |primitive: &crate::scene::Primitive| {
            let (lo, hi) = crate::scene::bounds(std::slice::from_ref(primitive));
            let lo = lo.offset(padding, padding);
            let hi = hi.offset(-padding, -padding);
            !pages
                .iter()
                .any(|(a, b)| lo.x >= a.x && hi.x <= b.x && lo.y >= a.y && hi.y <= b.y)
        };
        crate::scene::primitives(doc)
            .iter()
            .filter(|primitive| {
                let crate::scene::Primitive::Path {
                    commands,
                    style,
                    filled,
                } = primitive
                else {
                    return outside(primitive);
                };
                // Rendering batches disconnected bond polygons into one path. Each
                // subpath must fit on a page; the batch can span several pages.
                let mut start = 0;
                commands
                    .iter()
                    .enumerate()
                    .skip(1)
                    .filter_map(|(i, command)| {
                        matches!(command, crate::graphics::PathCommand::Move(_)).then_some(i)
                    })
                    .chain(std::iter::once(commands.len()))
                    .any(|end| {
                        let Some(part_commands) = commands.get(start..end) else {
                            return true;
                        };
                        let part = crate::scene::Primitive::Path {
                            commands: part_commands.to_vec(),
                            style: style.clone(),
                            filled: *filled,
                        };
                        start = end;
                        outside(&part)
                    })
            })
            .count()
    }
}
