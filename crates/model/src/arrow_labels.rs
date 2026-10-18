//! Condition text attached above or below a straight reaction arrow.
//!
//! The visible label is an ordinary annotation. The native link makes the
//! arrow own it: the label stays centred on its side of the arrow, and the
//! arrow is kept at least as long as its widest label plus padding. Export
//! formats that cannot retain the link still retain the text and the arrow.
use crate::document::{Annotation, Arrow, Document, Point};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Visually above the arrow; left of a vertical arrow.
    Above,
    Below,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrowLabel {
    pub arrow: u64,
    pub annotation: u64,
    pub side: Side,
}

pub const EXPORT_NOTICE: &str = "Condition text and arrow lengths are preserved; the links that keep labels attached to their arrows are omitted from this external format. Use a native ReShiki file to retain them.";

pub fn export_notice(doc: &Document) -> Option<String> {
    (!doc.arrow_labels.is_empty()).then(|| EXPORT_NOTICE.into())
}

/// Journal spacing, shared with assistant-composed schemes: padding at each
/// arrow end, the clearance between arrow ink and text, and the shortest
/// arrow a label resynchronises to.
struct Spacing {
    pad: f32,
    gap: f32,
    floor: f32,
}
fn spacing(doc: &Document) -> Spacing {
    let bond = doc.drawing_style.bond_length_world;
    Spacing {
        pad: bond * 0.2,
        gap: bond * 0.45,
        floor: bond * 2.8,
    }
}

/// Midpoint, unit direction, unit "above" normal and length of a labelable arrow.
struct Frame {
    mid: Point,
    dir: Point,
    up: Point,
    len: f32,
}
fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
/// Only straight, free arrows carry labels. **Straighten** keeps a control
/// point on the shaft, which still counts as straight.
pub fn supports(arrow: &Arrow) -> bool {
    arrow.cubic.is_none()
        && !arrow.is_attached()
        && arrow.control_point().is_none_or(|c| {
            let len = arrow.start.distance(arrow.end);
            let off_axis = if len > 1e-3 {
                ((arrow.end.x - arrow.start.x) * (c.y - arrow.start.y)
                    - (arrow.end.y - arrow.start.y) * (c.x - arrow.start.x))
                    .abs()
                    / len
            } else {
                c.distance(arrow.start)
            };
            off_axis <= 1e-3
        })
}
fn frame(arrow: &Arrow) -> Option<Frame> {
    let len = arrow.start.distance(arrow.end);
    if !supports(arrow) || len <= 1e-3 {
        return None;
    }
    Some(frame_along(
        arrow,
        Point::new(
            (arrow.end.x - arrow.start.x) / len,
            (arrow.end.y - arrow.start.y) / len,
        ),
    ))
}
fn frame_along(arrow: &Arrow, dir: Point) -> Frame {
    let len = arrow.start.distance(arrow.end);
    // Screen y points down. "Above" is up, or left for a vertical arrow, for
    // either arrow direction.
    let mut up = Point::new(dir.y, -dir.x);
    if up.y > 1e-6 || (up.y.abs() <= 1e-6 && up.x > 0.) {
        up = Point::new(-up.x, -up.y);
    }
    Frame {
        mid: arrow.start.lerp(arrow.end, 0.5),
        dir,
        up,
        len,
    }
}
/// Extent of a text box measured along a unit vector.
fn extent(size: (f32, f32), along: Point) -> f32 {
    size.0 * along.x.abs() + size.1 * along.y.abs()
}
fn centre(a: &Annotation) -> Point {
    let (w, h) = a.size();
    a.position.offset(w / 2., h / 2.)
}
/// Half the arrow ink's thickness across its axis, including heads.
fn half_thickness(arrow: &Arrow, frame: &Frame) -> f32 {
    let mut half: f32 = 0.;
    for path in arrow.paths() {
        let pad = path.style.width() * 0.5;
        for p in crate::graphics::flattened(&path.commands)
            .into_iter()
            .flatten()
        {
            half = half.max(dot(sub(p, frame.mid), frame.up).abs() + pad);
        }
    }
    half
}
/// The side whose slot contains this text, if any: its centre lies within the
/// arrow span, or the text spans the arrow midpoint, no further from the axis
/// than about a bond length past the attached position.
fn slot(doc: &Document, arrow: &Arrow, text: &Annotation) -> Option<(Side, f32)> {
    let frame = frame(arrow)?;
    let offset = sub(centre(text), frame.mid);
    let along = dot(offset, frame.dir).abs();
    if along > frame.len / 2. && along > extent(text.size(), frame.dir) / 2. {
        return None;
    }
    let across = dot(offset, frame.up);
    let reach = half_thickness(arrow, &frame)
        + spacing(doc).gap
        + extent(text.size(), frame.up) / 2.
        + doc.drawing_style.bond_length_world;
    if across.abs() > reach {
        return None;
    }
    let side = if across >= 0. {
        Side::Above
    } else {
        Side::Below
    };
    Some((side, across.abs()))
}

/// Labels attached to this arrow.
pub fn of(doc: &Document, arrow: u64) -> impl Iterator<Item = &ArrowLabel> {
    doc.arrow_labels.iter().filter(move |l| l.arrow == arrow)
}
fn widest(doc: &Document, arrow: u64, dir: Point) -> Option<f32> {
    of(doc, arrow)
        .filter_map(|l| doc.annotations.iter().find(|a| a.id == l.annotation))
        .map(|a| extent(a.size(), dir))
        .reduce(f32::max)
}
/// The shortest arrow that fits its labels, and the length an arrow that
/// follows its text takes, or `None` without labels.
fn lengths(doc: &Document, arrow: u64, dir: Point) -> Option<(f32, f32)> {
    let widest = widest(doc, arrow, dir)?;
    let s = spacing(doc);
    Some((widest + 2. * s.pad, widest.max(s.floor) + 2. * s.pad))
}
/// The shortest arrow that fits its labels, or `None` without labels.
pub fn minimum_length(doc: &Document, arrow: u64) -> Option<f32> {
    let frame = frame(doc.arrows.iter().find(|a| a.id == arrow)?)?;
    lengths(doc, arrow, frame.dir).map(|(minimum, _)| minimum)
}

/// Remove links whose arrow or text is gone, or whose arrow can no longer
/// carry labels. The text itself stays.
pub fn prune(doc: &mut Document) {
    if doc.arrow_labels.is_empty() {
        return;
    }
    let arrows: HashSet<_> = doc
        .arrows
        .iter()
        .filter(|a| supports(a))
        .map(|a| a.id)
        .collect();
    let texts: HashSet<_> = doc.annotations.iter().map(|a| a.id).collect();
    let captions: HashSet<_> = doc.molecule_names.iter().map(|n| n.annotation).collect();
    doc.arrow_labels.retain(|l| {
        arrows.contains(&l.arrow)
            && texts.contains(&l.annotation)
            && !captions.contains(&l.annotation)
    });
}

/// Called in the same transaction as every edit, after `before`:
/// 1. Text dragged on its own re-snaps within its slot, changes side, or
///    detaches once it leaves both slots.
/// 2. New or moved text dropped into a free slot of an unchanged arrow attaches.
/// 3. Each labelled arrow is lengthened to fit, follows edited text when it
///    was fitted to it, and its labels are centred on their sides.
pub fn reconcile(doc: &mut Document, before: &Document) {
    prune(doc);
    if doc.arrows.is_empty() {
        return;
    }
    let old_arrows: HashMap<u64, (Point, Point)> = before
        .arrows
        .iter()
        .map(|a| (a.id, (a.start, a.end)))
        .collect();
    let old_texts: HashMap<u64, Point> = before
        .annotations
        .iter()
        .map(|a| (a.id, a.position))
        .collect();
    let arrow_unchanged = |doc: &Document, id: u64| {
        doc.arrows
            .iter()
            .find(|a| a.id == id)
            .is_some_and(|a| old_arrows.get(&id) == Some(&(a.start, a.end)))
    };
    let moved = |a: &Annotation| old_texts.get(&a.id) != Some(&a.position);
    let retyped = |a: &Annotation| {
        before
            .annotations
            .iter()
            .find(|b| b.id == a.id)
            .is_some_and(|b| b.text != a.text || b.format != a.format)
    };
    // 1. Independently dragged labels.
    let labels = std::mem::take(&mut doc.arrow_labels);
    let mut kept: Vec<ArrowLabel> = Vec::with_capacity(labels.len());
    let mut dragged: Vec<ArrowLabel> = Vec::new();
    for mut label in labels {
        let text = doc.annotations.iter().find(|a| a.id == label.annotation);
        if text.is_some_and(moved) && arrow_unchanged(doc, label.arrow) {
            let arrow = doc.arrows.iter().find(|a| a.id == label.arrow);
            match arrow.zip(text).and_then(|(a, t)| slot(doc, a, t)) {
                Some((side, _)) => label.side = side,
                None => continue,
            }
            dragged.push(label);
        } else {
            kept.push(label);
        }
    }
    // A side holds one label; a label dragged onto an occupied side detaches
    // and the label already there stays.
    let mut sides: HashSet<_> = kept.iter().map(|l| (l.arrow, l.side)).collect();
    kept.extend(
        dragged
            .into_iter()
            .filter(|l| sides.insert((l.arrow, l.side))),
    );
    doc.arrow_labels = kept;
    // 2. Newly dropped or typed text.
    let attached: HashSet<_> = doc.arrow_labels.iter().map(|l| l.annotation).collect();
    let captions: HashSet<_> = doc.molecule_names.iter().map(|n| n.annotation).collect();
    let candidates: Vec<u64> = doc
        .annotations
        .iter()
        .filter(|a| {
            !attached.contains(&a.id)
                && !captions.contains(&a.id)
                && !a.text.trim().is_empty()
                && (moved(a) || retyped(a))
        })
        .map(|a| a.id)
        .collect();
    if candidates.is_empty() && doc.arrow_labels.is_empty() {
        return;
    }
    let stable: HashSet<u64> = doc
        .arrows
        .iter()
        .map(|a| a.id)
        .filter(|id| arrow_unchanged(doc, *id))
        .collect();
    for id in candidates {
        let Some(text) = doc.annotations.iter().find(|a| a.id == id) else {
            continue;
        };
        let best = doc
            .arrows
            .iter()
            .filter(|a| stable.contains(&a.id))
            .filter_map(|a| slot(doc, a, text).map(|(side, distance)| (a.id, side, distance)))
            .filter(|(arrow, side, _)| {
                !doc.arrow_labels
                    .iter()
                    .any(|l| l.arrow == *arrow && l.side == *side)
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((arrow, side, _)) = best {
            doc.arrow_labels.push(ArrowLabel {
                arrow,
                annotation: id,
                side,
            });
        }
    }
    // 3. Length and placement.
    let mut arrows: Vec<u64> = doc.arrow_labels.iter().map(|l| l.arrow).collect();
    arrows.sort_unstable();
    arrows.dedup();
    for id in arrows {
        layout(doc, before, id);
    }
}

fn layout(doc: &mut Document, before: &Document, id: u64) {
    let Some(arrow) = doc.arrows.iter().find(|a| a.id == id).cloned() else {
        return;
    };
    let old = before.arrows.iter().find(|a| a.id == id);
    let old_frame = old.and_then(frame);
    // An end dragged onto the other keeps the previous direction.
    let Some(f) = frame(&arrow).or_else(|| old_frame.as_ref().map(|o| frame_along(&arrow, o.dir)))
    else {
        return;
    };
    let Some((minimum, fitted)) = lengths(doc, id, f.dir) else {
        return;
    };
    // Edited text resynchronises an arrow that was fitted to its labels:
    // one at its follow length keeps following, one at its minimum keeps
    // hugging the text. Other arrows only grow.
    let edited = of(doc, id).any(|l| {
        let now = doc.annotations.iter().find(|a| a.id == l.annotation);
        let then = before.annotations.iter().find(|a| a.id == l.annotation);
        now.zip(then)
            .is_some_and(|(a, b)| a.text != b.text || a.format != b.format)
    });
    let mut target = f.len;
    if edited
        && let Some((o, (old_minimum, old_fitted))) = old_frame
            .as_ref()
            .and_then(|o| lengths(before, id, o.dir).map(|l| (o, l)))
    {
        let near = |a: f32, b: f32| (a - b).abs() <= doc.drawing_style.bond_length_world * 0.01;
        if near(o.len, old_fitted) {
            target = fitted;
        } else if near(o.len, old_minimum) {
            target = minimum;
        }
    }
    let target = target.max(minimum);
    if (target - f.len).abs() > 1e-4 {
        // Keep the end the user did not drag, or the midpoint otherwise.
        let (start, end) = match old {
            Some(o) if o.start == arrow.start && o.end != arrow.end => (
                arrow.start,
                arrow.start.offset(f.dir.x * target, f.dir.y * target),
            ),
            Some(o) if o.end == arrow.end && o.start != arrow.start => (
                arrow.end.offset(-f.dir.x * target, -f.dir.y * target),
                arrow.end,
            ),
            _ => (
                f.mid.offset(-f.dir.x * target / 2., -f.dir.y * target / 2.),
                f.mid.offset(f.dir.x * target / 2., f.dir.y * target / 2.),
            ),
        };
        if let Some(a) = doc.arrows.iter_mut().find(|a| a.id == id) {
            a.start = start;
            a.end = end;
            // A straightened arrow keeps its control on the shaft.
            if a.control.is_some() {
                a.control = Some(start.lerp(end, 0.5));
            }
        }
    }
    let Some(arrow) = doc.arrows.iter().find(|a| a.id == id) else {
        return;
    };
    let Some(f) = frame(arrow) else {
        return;
    };
    let clearance = half_thickness(arrow, &f) + spacing(doc).gap;
    let placements: Vec<(u64, Point)> = of(doc, id)
        .filter_map(|l| {
            let text = doc.annotations.iter().find(|a| a.id == l.annotation)?;
            let size = text.size();
            let normal = match l.side {
                Side::Above => f.up,
                Side::Below => Point::new(-f.up.x, -f.up.y),
            };
            let distance = clearance + extent(size, normal) / 2.;
            let centre = f.mid.offset(normal.x * distance, normal.y * distance);
            Some((l.annotation, centre.offset(-size.0 / 2., -size.1 / 2.)))
        })
        .collect();
    for (annotation, position) in placements {
        if let Some(text) = doc.annotations.iter_mut().find(|a| a.id == annotation) {
            text.position = position;
        }
    }
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let mut texts = HashSet::new();
    let mut sides = HashSet::new();
    for label in &doc.arrow_labels {
        if !texts.insert(label.annotation)
            || !sides.insert((label.arrow, label.side))
            || !doc.annotations.iter().any(|a| a.id == label.annotation)
            || crate::molecule_names::is_caption(doc, label.annotation)
            || !doc
                .arrows
                .iter()
                .any(|a| a.id == label.arrow && supports(a))
        {
            return Err("Invalid arrow condition label".into());
        }
    }
    Ok(())
}

/// Copying an arrow also copies its labels.
pub fn include_labels(doc: &Document, ids: &mut HashSet<u64>) {
    for label in &doc.arrow_labels {
        if ids.contains(&label.arrow) {
            ids.insert(label.annotation);
        }
    }
}

/// `ids` plus the labels their arrows own, for Delete and Cut: an arrow takes
/// its condition text with it, as Copy does.
pub fn with_labels(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut all = ids.to_vec();
    for label in &doc.arrow_labels {
        if ids.contains(&label.arrow) && !all.contains(&label.annotation) {
            all.push(label.annotation);
        }
    }
    all
}

#[cfg(test)]
mod tests;
