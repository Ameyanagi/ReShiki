//! Text a closed shape holds: an ordinary annotation that the shape keeps
//! centred inside itself and wrapped to its inner width. The text moves and
//! resizes with the shape; dragging the text drags the shape
//! ([`with_holders`]). Formats without the link keep the text as a caption
//! over its shape.
use crate::document::{Annotation, Document, Point};
use crate::graphics::Graphic;
use std::collections::{HashMap, HashSet};

pub const EXPORT_NOTICE: &str = "Text in shapes is written as captions centred over their shapes; ChemDraw does not keep them together. Save a .rsk file to keep the text in its shape.";

pub fn export_notice(doc: &Document) -> Option<String> {
    doc.graphics
        .iter()
        .any(|g| g.text.is_some())
        .then(|| EXPORT_NOTICE.into())
}

/// Rectangles, rounded rectangles, ellipses and circles that are plain shapes.
pub fn supports(g: &Graphic) -> bool {
    g.kind.closed() && g.panel.is_none() && g.cycle.is_none() && g.frame_determinant().abs() > 1e-6
}

/// The shape's centre and the width its text wraps to, in world units: the
/// horizontal chord through the centre, inset so text clears the outline.
pub fn inner(g: &Graphic) -> (Point, f32) {
    let centre = g.origin.offset(
        (g.axis_x.x + g.axis_y.x) / 2.,
        (g.axis_x.y + g.axis_y.y) / 2.,
    );
    // One world unit along x, in the shape's unit-square frame.
    let d = g.frame_determinant();
    let v = Point::new(g.axis_y.y / d, -g.axis_x.y / d);
    let width = if g.kind == crate::graphics::GraphicKind::Ellipse {
        // The square inscribed in the ellipse's horizontal chord.
        0.65 / v.x.hypot(v.y)
    } else {
        0.85 / v.x.abs().max(v.y.abs())
    };
    (centre, width)
}

/// The width a shape's text wraps to, in points, at least the narrowest
/// text box: text in a small shape overflows it, centred.
pub fn width_pt(g: &Graphic) -> f32 {
    (inner(g).1 * crate::style::DEFAULT.points_per_world()).clamp(10., 2000.)
}

/// Where new text typed into `g` starts: one empty centred line.
pub fn start(g: &Graphic, format: &crate::typography::TextFormat) -> Point {
    let (centre, width) = inner(g);
    let height = crate::typography::layout("", format).height;
    centre.offset(-width / 2., -height / 2.)
}

/// The shape holding `annotation`.
pub fn holder(doc: &Document, annotation: u64) -> Option<&Graphic> {
    doc.graphics.iter().find(|g| g.text == Some(annotation))
}

/// `ids` plus the shapes holding any text in them, for moves: dragging the
/// text drags its shape.
pub fn with_holders(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut all = ids.to_vec();
    for g in &doc.graphics {
        if g.text.is_some_and(|t| ids.contains(&t)) && !all.contains(&g.id) {
            all.push(g.id);
        }
    }
    all
}

/// `ids` plus the text their shapes hold, for Copy, Delete and moves.
pub fn include_texts(doc: &Document, ids: &mut HashSet<u64>) {
    for g in &doc.graphics {
        if let Some(text) = g.text
            && ids.contains(&g.id)
        {
            ids.insert(text);
        }
    }
}

/// [`include_texts`] for a list.
pub fn with_texts(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut all = ids.to_vec();
    for g in &doc.graphics {
        if let Some(text) = g.text
            && ids.contains(&g.id)
            && !all.contains(&text)
        {
            all.push(text);
        }
    }
    all
}

/// Drop links whose text is gone, or whose shape can no longer hold text.
pub fn prune(doc: &mut Document) {
    let texts: HashSet<_> = doc.annotations.iter().map(|a| a.id).collect();
    for g in &mut doc.graphics {
        if g.text.is_some_and(|t| !texts.contains(&t)) || !supports(g) {
            g.text = None;
        }
    }
}

/// Called in the same transaction as every edit: every held text is wrapped
/// to its shape and centred in it.
pub fn reconcile(doc: &mut Document) {
    if doc.graphics.iter().all(|g| g.text.is_none()) {
        return;
    }
    prune(doc);
    let placements: HashMap<u64, (Point, f32)> = doc
        .graphics
        .iter()
        .filter_map(|g| Some((g.text?, (inner(g).0, width_pt(g)))))
        .collect();
    for a in &mut doc.annotations {
        if let Some((centre, width)) = placements.get(&a.id) {
            place(a, *centre, *width);
        }
    }
}

fn place(a: &mut Annotation, centre: Point, width_pt: f32) {
    a.format.width_pt = Some(width_pt);
    let (w, h) = a.size();
    a.position = centre.offset(-w / 2., -h / 2.);
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let texts: HashSet<_> = doc.annotations.iter().map(|a| a.id).collect();
    let owned: HashSet<_> = doc
        .molecule_names
        .iter()
        .map(|n| n.annotation)
        .chain(doc.property_captions.iter().map(|l| l.annotation))
        .chain(doc.arrow_labels.iter().map(|l| l.annotation))
        .collect();
    let mut held = HashSet::new();
    for g in &doc.graphics {
        let Some(text) = g.text else { continue };
        if !supports(g) || !held.insert(text) || !texts.contains(&text) || owned.contains(&text) {
            return Err("Invalid text in a shape".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
