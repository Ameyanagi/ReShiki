//! Figure panels, rectangles with a tint, rounded corners, titles and dividers.
//! Panels are artwork; reaction detection never reads them.
use super::{Graphic, GraphicKind, GraphicLabel, PathCommand};
use crate::{
    document::{Document, Point},
    palette::Color,
    scientific::Part,
    style,
    typography::TextStyle,
};
use serde::{Deserialize, Serialize};

/// Where a panel's titles sit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitlePlace {
    /// On a band across the top, in the outline color.
    #[default]
    Bar,
    /// Set into the top edge, on the page color.
    Border,
}
impl std::fmt::Display for TitlePlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Bar => "Title bar",
            Self::Border => "In border",
        })
    }
}

/// A line across the panel at a fraction of its width (vertical) or height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Divider {
    pub vertical: bool,
    pub at: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panel {
    /// One title per column between vertical dividers, left to right.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<String>,
    pub place: TitlePlace,
    pub border: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dividers: Vec<Divider>,
}
impl Default for Panel {
    fn default() -> Self {
        Self {
            titles: vec![],
            place: TitlePlace::Bar,
            border: true,
            dividers: vec![],
        }
    }
}
impl Panel {
    /// The corner radius of a rounded panel, as the shape's `corner_pt`.
    pub const ROUNDED_PT: f32 = 8.;
    pub fn validate(&self) -> Result<(), String> {
        if self.dividers.len() > 16
            || self
                .dividers
                .iter()
                .any(|d| !(d.at.is_finite() && d.at > 0. && d.at < 1.))
        {
            return Err("Panel dividers must lie inside the panel".into());
        }
        if self.titles.len() > self.columns().len()
            || self.titles.iter().any(|t| t.chars().count() > 200)
        {
            return Err("A panel has at most one title of 200 characters per column".into());
        }
        Ok(())
    }
    /// Column edges between vertical dividers, as fractions from 0 to 1.
    pub fn columns(&self) -> Vec<(f32, f32)> {
        let mut edges: Vec<f32> = self
            .dividers
            .iter()
            .filter(|d| d.vertical)
            .map(|d| d.at)
            .collect();
        edges.sort_by(f32::total_cmp);
        let mut columns = vec![];
        let mut left = 0.;
        for edge in edges.into_iter().chain([1.]) {
            columns.push((left, edge));
            left = edge;
        }
        columns
    }
    fn has_titles(&self) -> bool {
        self.titles.iter().any(|t| !t.trim().is_empty())
    }
}

/// Title text size and the band below a title bar, in world units.
fn metrics(text_style: &TextStyle) -> (f32, f32) {
    let size = text_style.size();
    (size, size * 1.8)
}

impl Graphic {
    /// The panel's frame: top-left corner, unit axes and size in world units.
    fn panel_frame(&self) -> (Point, Point, Point, f32, f32) {
        let w = self.axis_x.distance(Point::default());
        let h = self.axis_y.distance(Point::default());
        let unit = |v: Point, length: f32, fallback: Point| {
            if length > 0. {
                Point::new(v.x / length, v.y / length)
            } else {
                fallback
            }
        };
        (
            self.origin,
            unit(self.axis_x, w, Point::new(1., 0.)),
            unit(self.axis_y, h, Point::new(0., 1.)),
            w,
            h,
        )
    }
    /// After a flip, start the frame again from the panel's top left and
    /// mirror its dividers and column titles, so titles stay upright and the
    /// dividers stay between what they divided.
    pub(crate) fn normalize_panel(&mut self) {
        let Some(panel) = &mut self.panel else {
            return;
        };
        if self.axis_x.x < 0. {
            self.origin = self.origin.offset(self.axis_x.x, self.axis_x.y);
            self.axis_x = Point::new(-self.axis_x.x, -self.axis_x.y);
            let columns = panel.columns().len();
            panel.titles.resize(columns, String::new());
            panel.titles.reverse();
            while panel.titles.last().is_some_and(|t| t.is_empty()) {
                panel.titles.pop();
            }
            for d in panel.dividers.iter_mut().filter(|d| d.vertical) {
                d.at = 1. - d.at;
            }
        }
        if self.axis_y.y < 0. {
            self.origin = self.origin.offset(self.axis_y.x, self.axis_y.y);
            self.axis_y = Point::new(-self.axis_y.x, -self.axis_y.y);
            for d in panel.dividers.iter_mut().filter(|d| !d.vertical) {
                d.at = 1. - d.at;
            }
        }
    }
    /// The corner radius in world units: a rounded rectangle's, else none.
    fn panel_radius(&self) -> f32 {
        if self.kind == GraphicKind::RoundedRectangle {
            self.corner_radius()
        } else {
            0.
        }
    }
    /// Height of the title band, 0 without one.
    pub fn panel_bar_height(&self, text_style: &TextStyle) -> f32 {
        match &self.panel {
            Some(panel) if panel.place == TitlePlace::Bar && panel.has_titles() => {
                metrics(text_style).1
            }
            _ => 0.,
        }
    }
    /// Title band, page-colored title backings and dividers, drawn over the
    /// panel's body in this order.
    pub fn panel_parts(&self, text_style: &TextStyle) -> Vec<Part> {
        let Some(panel) = &self.panel else {
            return vec![];
        };
        let (o, ex, ey, w, h) = self.panel_frame();
        let at = |u: f32, v: f32| o.offset(ex.x * u + ey.x * v, ex.y * u + ey.y * v);
        let flat = |commands, color| Part {
            commands,
            style: super::GraphicStyle {
                stroke: color,
                fill: Some(color),
                width_pt: 0.,
                ..self.style.clone()
            },
            filled: true,
        };
        let (size, band) = metrics(text_style);
        let bar = self.panel_bar_height(text_style).min(h);
        let mut parts = vec![];
        if bar > 0. {
            let r = self.panel_radius().min(bar);
            let k = 0.552_284_8 * r;
            use PathCommand::*;
            parts.push(flat(
                vec![
                    Move(at(0., bar)),
                    Line(at(0., r)),
                    Cubic(at(0., r - k), at(r - k, 0.), at(r, 0.)),
                    Line(at(w - r, 0.)),
                    Cubic(at(w - r + k, 0.), at(w, r - k), at(w, r)),
                    Line(at(w, bar)),
                    Close,
                ],
                self.style.stroke,
            ));
        }
        if panel.place == TitlePlace::Border {
            for label in self.panel_titles(text_style) {
                let gap = size * 0.3;
                let (lo, hi) = (
                    label.position.offset(-gap, 0.),
                    label.position.offset(
                        style::styled_text_width(label.text, size, &label.style) + gap,
                        size,
                    ),
                );
                parts.push(flat(
                    vec![
                        PathCommand::Move(lo),
                        PathCommand::Line(Point::new(hi.x, lo.y)),
                        PathCommand::Line(hi),
                        PathCommand::Line(Point::new(lo.x, hi.y)),
                        PathCommand::Close,
                    ],
                    Color::Custom([255; 3]),
                ));
            }
        }
        let inset = size * 0.8;
        let top = if bar > 0. { bar } else { band * 0.5 };
        for divider in &panel.dividers {
            // A divider too short to see, or under the title band, is left out.
            let (a, b) = if divider.vertical && h - inset > top {
                let u = divider.at * w;
                (at(u, top), at(u, h - inset))
            } else if !divider.vertical && divider.at * h > bar && w > 2. * inset {
                let v = divider.at * h;
                (at(inset, v), at(w - inset, v))
            } else {
                continue;
            };
            parts.push(Part {
                commands: vec![PathCommand::Move(a), PathCommand::Line(b)],
                style: self.style.clone(),
                filled: false,
            });
        }
        parts
    }
    /// Upright titles, one per column, in bold at the drawing's text size.
    pub fn panel_titles(&self, text_style: &TextStyle) -> Vec<GraphicLabel<'_>> {
        let Some(panel) = &self.panel else {
            return vec![];
        };
        let (o, ex, ey, w, _) = self.panel_frame();
        let (size, band) = metrics(text_style);
        let r = self.panel_radius();
        let bar = panel.place == TitlePlace::Bar;
        let color = if bar {
            // Titles on the band read in page color, or ink on a pale band.
            if crate::color_contrast::luminance(self.style.stroke.rgb()) > 0.45 {
                Color::Ink
            } else {
                Color::Custom([255; 3])
            }
        } else {
            self.style.stroke
        };
        let style = TextStyle {
            bold: true,
            color,
            script: crate::typography::Script::Normal,
            formula: false,
            ..text_style.clone()
        };
        let v = if bar {
            (band - size) * 0.5
        } else {
            -size * 0.5
        };
        panel
            .titles
            .iter()
            .zip(panel.columns())
            .filter(|(text, _)| !text.trim().is_empty())
            .map(|(text, (left, _))| {
                let inset = if left == 0. {
                    r.max(size * 0.6) + size * 0.2
                } else {
                    size * 0.8
                };
                let u = left * w + inset;
                GraphicLabel {
                    text,
                    position: o.offset(ex.x * u + ey.x * v, ex.y * u + ey.y * v),
                    size,
                    style: style.clone(),
                }
            })
            .collect()
    }
}

/// Space between a panel's edge and what it holds, in points.
pub const PADDING_PT: f32 = 10.;

/// Refit a panel around what it is grouped with, or else around what lies
/// inside it, keeping room for its title band.
pub fn fit(doc: &mut Document, panel: u64) {
    let held: Vec<u64> = doc
        .groups
        .iter()
        .filter(|g| g.members.contains(&panel))
        .min_by_key(|g| g.members.len())
        .map(|g| g.members.iter().copied().filter(|m| *m != panel).collect())
        .unwrap_or_else(|| contents(doc, panel));
    // Captions linked to molecules it holds, even ones added later.
    let mut held = held;
    held.extend(
        crate::layouts::caption_links(doc)
            .into_iter()
            .filter(|(objects, annotation)| {
                !objects.is_empty()
                    && !held.contains(annotation)
                    && objects.iter().all(|id| held.contains(id))
            })
            .map(|(_, annotation)| annotation)
            .collect::<Vec<_>>(),
    );
    let Some((lo, hi)) = crate::scene::selection_bounds(doc, &held) else {
        return;
    };
    let text = doc.drawing_style.text_style();
    let pad = style::DEFAULT.world(PADDING_PT);
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == panel) {
        let band = g.panel_bar_height(&text);
        g.origin = lo.offset(-pad, -pad - band);
        g.axis_x = Point::new(hi.x - lo.x + 2. * pad, 0.);
        g.axis_y = Point::new(0., hi.y - lo.y + 2. * pad + band);
    }
}

/// Panels in fitting order: inner ones first, so an outer panel fits around
/// their new frames.
pub fn inner_first(doc: &Document, mut panels: Vec<u64>) -> Vec<u64> {
    let depth = |id: u64| {
        doc.groups
            .iter()
            .filter(|g| g.members.contains(&id))
            .count()
    };
    panels.sort_by_key(|id| std::cmp::Reverse(depth(*id)));
    panels
}

/// Panels whose own group holds every one of `ids`: they wrap them.
pub fn around(doc: &Document, ids: &[u64]) -> Vec<u64> {
    doc.graphics
        .iter()
        .filter(|g| g.panel.is_some() && !ids.contains(&g.id))
        .filter(|g| {
            doc.groups
                .iter()
                .filter(|outer| outer.members.contains(&g.id))
                .min_by_key(|outer| outer.members.len())
                .is_some_and(|outer| ids.iter().all(|m| outer.members.contains(m)))
        })
        .map(|g| g.id)
        .collect()
}

/// Whole molecules and other objects that lie entirely inside the panel,
/// for scoping what a panel holds. Empty when `panel` is not a panel.
pub fn contents(doc: &Document, panel: u64) -> Vec<u64> {
    content_units(doc, panel)
        .into_iter()
        .flat_map(|(unit, _)| unit)
        .collect()
}

/// What the panel holds, each whole molecule or other object with its bounds.
fn content_units(doc: &Document, panel: u64) -> Vec<(Vec<u64>, (Point, Point))> {
    let Some(frame) = doc
        .graphics
        .iter()
        .find(|g| g.id == panel && g.panel.is_some())
    else {
        return vec![];
    };
    let (lo, hi) = frame.shape_bounds();
    let atoms: Vec<u64> = doc.atoms.iter().map(|a| a.id).collect();
    let mut units = crate::reactions::molecules(doc, &atoms);
    units.extend(
        doc.object_ids()
            .filter(|id| *id != panel && doc.atom(*id).is_none())
            .map(|id| vec![id]),
    );
    let bounds = crate::scene::selections_bounds(doc, &units);
    units
        .into_iter()
        .zip(bounds)
        .filter_map(|(unit, bounds)| {
            let (a, b) = bounds?;
            (a.x >= lo.x && a.y >= lo.y && b.x <= hi.x && b.y <= hi.y).then_some((unit, (a, b)))
        })
        .collect()
}

/// Where a new divider best splits the panel's contents: the middle of the
/// widest empty band between them along x (vertical) or y, as a fraction.
/// The panel's middle when its contents leave no gap.
pub fn divider_position(doc: &Document, panel: u64, vertical: bool) -> f32 {
    let Some(frame) = doc.graphics.iter().find(|g| g.id == panel) else {
        return 0.5;
    };
    // Measured along the panel's own axis, so a rotated panel splits right.
    let (o, ex, ey, w, h) = frame.panel_frame();
    let along = if vertical { ex } else { ey };
    let axis = |p: Point| (p.x - o.x) * along.x + (p.y - o.y) * along.y;
    let (start, end) = (0., if vertical { w } else { h });
    // Each object on its own: the panel's group would join them all.
    let mut spans: Vec<(f32, f32)> = content_units(doc, panel)
        .into_iter()
        .map(|(_, (a, b))| {
            let corners = [a, Point::new(b.x, a.y), b, Point::new(a.x, b.y)].map(axis);
            corners
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(*v), hi.max(*v))
                })
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let taken: Vec<f32> = frame
        .panel
        .iter()
        .flat_map(|p| &p.dividers)
        .filter(|d| d.vertical == vertical)
        .map(|d| start + d.at * (end - start))
        .collect();
    let mut best: Option<(f32, f32)> = None;
    let mut reach = f32::NEG_INFINITY;
    for (a, b) in spans.iter().zip(spans.iter().skip(1)) {
        reach = reach.max(a.1);
        let gap = b.0 - reach;
        let middle = (reach + b.0) * 0.5;
        if gap > 0.
            && best.is_none_or(|(width, _)| gap > width)
            && !taken.iter().any(|t| *t > reach && *t < b.0)
        {
            best = Some((gap, middle));
        }
    }
    let middle = best.map_or((start + end) * 0.5, |(_, m)| m);
    if end > start {
        ((middle - start) / (end - start)).clamp(0.05, 0.95)
    } else {
        0.5
    }
}

impl GraphicKind {
    /// Shapes that can carry panel titles and dividers.
    pub fn panel_capable(self) -> bool {
        matches!(self, Self::Rectangle | Self::RoundedRectangle)
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
