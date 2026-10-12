//! Session-only NMR assignments. These labels never enter the chemical graph,
//! persistent atom labels, the retained document scene or figure exports.
use super::overlay::ACCENT;
use super::{Camera, MoleculeCanvas, layered, rgb};
use iced::widget::canvas::{Path, Stroke, Text};
use iced::{Point, Rectangle, Size, Vector};
use reshiki::document::Document;
use reshiki::nmr::Report;
use std::{collections::HashMap, rc::Rc};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LabelMode {
    #[default]
    Atom,
    Shift,
    Both,
}
#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub report: &'a Report,
    pub mode: LabelMode,
    /// Sorted supported owners retained in the report's presentation.
    pub supported: Option<&'a [u64]>,
}
impl<'a> Context<'a> {
    pub(super) fn supported_ids(self) -> Vec<u64> {
        self.report
            .rows
            .iter()
            .filter(|row| row.shift().is_some())
            .map(|row| row.atom_id)
            .collect()
    }

    pub(super) fn marker_ids(self) -> std::borrow::Cow<'a, [u64]> {
        if let Some(ids) = self.supported {
            return std::borrow::Cow::Borrowed(ids);
        }
        let mut ids = self.supported_ids();
        ids.sort_unstable();
        ids.dedup();
        std::borrow::Cow::Owned(ids)
    }
}
#[derive(Debug, PartialEq)]
pub(crate) struct Assignment {
    pub atom_id: u64,
    pub anchor: Point,
    pub bounds: Rectangle,
    pub text: String,
    pub leader: bool,
}

const TEXT_SIZE: f32 = 11.;
const TEXT_HEIGHT: f32 = TEXT_SIZE * 1.3;
const MAX_WIDTHS: usize = 512;

#[derive(Default)]
pub(super) struct WidthMemo {
    widths: HashMap<String, f32>,
}

impl WidthMemo {
    fn width(&mut self, text: &str) -> f32 {
        if let Some(&width) = self.widths.get(text) {
            return width;
        }
        let width = crate::app::text_width(text, TEXT_SIZE);
        if self.widths.len() == MAX_WIDTHS {
            self.widths.clear();
        }
        self.widths.insert(text.to_owned(), width);
        width
    }
}

fn padded(rect: Rectangle, amount: f32) -> Rectangle {
    Rectangle {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + amount * 2.,
        height: rect.height + amount * 2.,
    }
}

fn nearest_point(rect: Rectangle, anchor: Point) -> Point {
    Point::new(
        anchor.x.clamp(rect.x, rect.x + rect.width),
        anchor.y.clamp(rect.y, rect.y + rect.height),
    )
}

pub(crate) fn placements(
    doc: &Document,
    context: Context<'_>,
    camera: Camera,
    bounds: Rectangle,
) -> Vec<Assignment> {
    placements_with_width(doc, context, camera, bounds, |text| {
        crate::app::text_width(text, TEXT_SIZE)
    })
}

pub(super) fn memoized_placements(
    doc: &Document,
    context: Context<'_>,
    camera: Camera,
    bounds: Rectangle,
    widths: &mut WidthMemo,
) -> Vec<Assignment> {
    placements_with_width(doc, context, camera, bounds, |text| widths.width(text))
}

fn placements_with_width(
    doc: &Document,
    context: Context<'_>,
    camera: Camera,
    bounds: Rectangle,
    mut width: impl FnMut(&str) -> f32,
) -> Vec<Assignment> {
    let mut occupied: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| doc.atom_visible(a.id))
        .map(|a| {
            if let Some((lo, hi)) = reshiki::scene::atom_label_bounds(a, doc) {
                let lo = camera.screen(lo, bounds);
                let hi = camera.screen(hi, bounds);
                Rectangle {
                    x: lo.x - 5.,
                    y: lo.y - 5.,
                    width: hi.x - lo.x + 10.,
                    height: hi.y - lo.y + 10.,
                }
            } else {
                let point = camera.screen(a.position, bounds);
                Rectangle {
                    x: point.x - 7.,
                    y: point.y - 7.,
                    width: 14.,
                    height: 14.,
                }
            }
        })
        .collect();
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| doc.bond_visible(b.a, b.b))
        .filter_map(|b| {
            Some((
                camera.screen(doc.atom(b.a)?.position, bounds),
                camera.screen(doc.atom(b.b)?.position, bounds),
            ))
        })
        .collect();
    let mut result = Vec::new();
    for row in &context.report.rows {
        let Some(shift) = row.shift() else {
            continue;
        };
        let Some(atom) = doc.atom(row.atom_id) else {
            continue;
        };
        let collapsed = (!doc.atom_visible(atom.id))
            .then(|| doc.abbreviation_containing(atom.id))
            .flatten();
        let anchor = if let Some(group) = collapsed {
            let Some(anchor) = doc.atom(group.anchor) else {
                continue;
            };
            camera.screen(anchor.position, bounds)
        } else {
            camera.screen(atom.position, bounds)
        };
        if !anchor.x.is_finite()
            || !anchor.y.is_finite()
            || !Rectangle::with_size(bounds.size()).contains(anchor)
        {
            continue;
        }
        let text = match context.mode {
            LabelMode::Atom => format!("#{}", row.atom_id),
            LabelMode::Shift => format!("{shift:.3} ppm"),
            LabelMode::Both => format!("#{} · {shift:.3} ppm", row.atom_id),
        };
        // The nucleus and unresolved-H/abbreviation ownership remain in the
        // report and accessible site descriptions, rather than every label.
        let size = Size::new(width(&text).min((bounds.width - 8.).max(1.)), TEXT_HEIGHT);
        let mut best = None;
        for distance in [8., 18., 32., 48., 64.] {
            for offset in [
                Vector::new(distance, -size.height - distance),
                Vector::new(distance, distance),
                Vector::new(-size.width - distance, -size.height - distance),
                Vector::new(-size.width - distance, distance),
                Vector::new(-size.width / 2., -size.height - distance),
                Vector::new(-size.width / 2., distance),
                Vector::new(distance, -size.height / 2.),
                Vector::new(-size.width - distance, -size.height / 2.),
            ] {
                let point = anchor + offset;
                let rect = Rectangle {
                    x: point.x.clamp(4., (bounds.width - size.width - 4.).max(4.)),
                    y: point
                        .y
                        .clamp(4., (bounds.height - size.height - 4.).max(4.)),
                    ..Rectangle::with_size(size)
                };
                let collisions = occupied
                    .iter()
                    .filter(|other| padded(rect, 2.).intersects(other))
                    .count();
                let crossings = bonds
                    .iter()
                    .filter(|&&(a, b)| {
                        segment_crosses(
                            padded(rect, doc.drawing_style.line_width() * camera.zoom / 2. + 1.),
                            a,
                            b,
                        )
                    })
                    .count();
                let score = collisions as f32 * 10_000.
                    + crossings as f32 * 1_000.
                    + anchor.distance(rect.center());
                if best.is_none_or(|(_, previous)| score < previous) {
                    best = Some((rect, score));
                }
            }
        }
        if let Some((rect, _)) = best {
            occupied.push(rect);
            result.push(Assignment {
                atom_id: row.atom_id,
                anchor,
                bounds: rect,
                text,
                leader: anchor.distance(nearest_point(rect, anchor)) > 22.,
            });
        }
    }
    result
}

fn segment_crosses(rect: Rectangle, a: Point, b: Point) -> bool {
    let mut low = 0_f32;
    let mut high = 1_f32;
    let delta = b - a;
    for (start, direction, min, max) in [
        (a.x, delta.x, rect.x, rect.x + rect.width),
        (a.y, delta.y, rect.y, rect.y + rect.height),
    ] {
        if direction.abs() < f32::EPSILON {
            if start < min || start > max {
                return false;
            }
        } else {
            let first = (min - start) / direction;
            let last = (max - start) / direction;
            low = low.max(first.min(last));
            high = high.min(first.max(last));
            if low > high {
                return false;
            }
        }
    }
    true
}

impl MoleculeCanvas<'_> {
    pub(super) fn draw_nmr_assignments(
        &self,
        frame: &mut layered::Frame<'_>,
        preview: &Document,
        selected: &[u64],
        bounds: Rectangle,
        cache: Option<&std::cell::RefCell<super::cache::SceneCache>>,
    ) {
        let Some(context) = self.nmr else {
            return;
        };
        let assignments: Rc<[Assignment]> = if let Some(cache) = cache {
            cache
                .borrow_mut()
                .nmr_assignments(preview, context, self.camera, bounds)
        } else {
            placements(preview, context, self.camera, bounds).into()
        };
        for assignment in assignments.iter() {
            let selected = selected.contains(&assignment.atom_id);
            let color = if selected { rgb([19, 105, 89]) } else { ACCENT };
            // Close labels need no callout. A displaced crowded label still
            // indicates its real atom or visible abbreviation anchor.
            if assignment.leader {
                frame.stroke(
                    &Path::line(
                        assignment.anchor,
                        nearest_point(assignment.bounds, assignment.anchor),
                    ),
                    Stroke::default()
                        .with_width(0.7)
                        .with_color(color.scale_alpha(0.45)),
                );
            }
            frame.fill_text(Text {
                content: assignment.text.clone(),
                position: assignment.bounds.position(),
                size: TEXT_SIZE.into(),
                color,
                font: iced::Font::with_name(reshiki::style::ui_font_family()),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
mod tests;
