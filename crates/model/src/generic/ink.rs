//! Measure saved and refreshed drawing ink and normalize detached arrows.
use super::{
    Work,
    budget::{Presentation, charge_clearance},
    geometry::{at, finite, include},
    variable::variable_ink_clearance,
};
use crate::document::{Document, Point};

pub(super) fn measured_bounds(
    doc: &Document,
    boxes: &[Option<(Point, Point)>],
) -> Result<(Point, Point), String> {
    let mut bounds = object_bounds(doc)?;
    for (i, a) in doc.atoms.iter().enumerate() {
        include(&mut bounds, a.position, a.position);
        if let Some((lo, hi)) = *at(boxes, i)? {
            include(&mut bounds, lo, hi);
        }
        if crate::highlights::atom_color(doc, a).is_some() {
            let radius = highlight_radius(&doc.drawing_style);
            // These cached boxes contain the renderer's label/mark ink
            // (and may also contain indicators). A larger box only grows
            // its elliptical capsule; it cannot exclude the real pill.
            let (left, right, rx, ry) = if let Some((lo, hi)) = *at(boxes, i)? {
                let ry = (hi.y - lo.y + radius) * 0.5;
                let y = (lo.y + hi.y) * 0.5;
                (Point::new(lo.x, y), Point::new(hi.x, y), ry * 0.5, ry)
            } else {
                (a.position, a.position, radius, radius)
            };
            include(
                &mut bounds,
                left.offset(-rx * CAPSULE_SCALE, -ry * CAPSULE_SCALE),
                right.offset(rx * CAPSULE_SCALE, ry * CAPSULE_SCALE),
            );
        }
    }
    let (lo, hi) = bounds.ok_or("Expanded structure has no positioned ink")?;
    let pad = bond_paint_radius(&doc.drawing_style);
    let lo = lo.offset(-pad, -pad);
    let hi = hi.offset(pad, pad);
    finite(lo)?;
    finite(hi)?;
    Ok((lo, hi))
}

/// Normalize detached arrow geometry and certify variable-branch ink clearance.
/// This is the same native reconciliation rule, paid before the preview copy.
/// Arrow targets/styles and all other native fields remain unchanged. Endpoint,
/// neighboring control, offset and legacy quadratic-to-cubic normalization are
/// explicitly allowed generated-output changes; the live source stays intact.
pub(super) fn normalize_arrows(
    doc: &mut Document,
    hydrogens: &[u32],
    source: &Document,
    presentation: &Presentation,
    work: &mut Work,
) -> Result<(), String> {
    let variables = source
        .atoms
        .iter()
        .filter(|a| a.is_variable_attachment())
        .count();
    let arrows = doc.arrows.iter().any(|a| a.is_attached());
    if !arrows && variables == 0 {
        return Ok(());
    }
    if variables != 0 {
        work.charge(source.atoms.len())?;
        for _ in 0..variables {
            charge_clearance(doc, work)?;
        }
    }
    presentation.charge_measurement(source, doc, work)?;
    let mut preview = refreshed_preview(doc, hydrogens)?;
    if arrows {
        normalize_native_states(doc, &mut preview)?;
    }
    if variables != 0 {
        // Reuse the one already-prepared chemical state. Saved label caches
        // and exact refreshed H labels both contribute; no guessed H state or
        // second chemistry preparation is allowed at this geometry seam.
        let boxes = saved_and_refreshed_boxes(doc, &preview)?;
        variable_ink_clearance(source, doc, &boxes)?;
    }
    Ok(())
}

fn normalize_native_states(saved: &mut Document, refreshed: &mut Document) -> Result<(), String> {
    // Public from_json resolves the saved label_h before from_native_file
    // clears only CIP. A later label refresh may show different H ink. Take
    // outward clearance across both states, then prove neither can move again.
    crate::arrow_anchors::reconcile(refreshed);
    saved.arrows = std::mem::take(&mut refreshed.arrows);
    crate::arrow_anchors::reconcile(saved);
    crate::arrow_anchors::certify_resolved(saved)?;
    refreshed.arrows = saved.arrows.clone();
    crate::arrow_anchors::certify_resolved(refreshed)
}

pub(super) fn saved_and_refreshed_boxes(
    saved: &Document,
    refreshed: &Document,
) -> Result<Vec<Option<(Point, Point)>>, String> {
    if saved.atoms.len() != refreshed.atoms.len() {
        return Err("Prepared label snapshots differ in size".into());
    }
    saved
        .atoms
        .iter()
        .zip(&refreshed.atoms)
        .map(|(a, b)| {
            let mut bounds = atom_bounds(a, saved)?;
            if let Some((lo, hi)) = atom_bounds(b, refreshed)? {
                include(&mut bounds, lo, hi);
            }
            Ok(bounds)
        })
        .collect()
}

fn refreshed_preview(doc: &Document, hydrogens: &[u32]) -> Result<Document, String> {
    if hydrogens.len() != doc.atoms.len() {
        return Err("Prepared tile lost its cached hydrogen state".into());
    }
    let mut preview = doc.clone();
    for (a, &h) in preview.atoms.iter_mut().zip(hydrogens) {
        a.label_h = h;
    }
    Ok(preview)
}

/// Bound actual installed-tile ink, after its actual f32 remap/translation and
/// refreshed native arrow normalization. No cached bounds are translated as a
/// substitute, and no molecule/identifier preparation runs again.
pub(super) fn measure_translated(
    doc: &mut Document,
    hydrogens: &[u32],
    source: &Document,
    presentation: &Presentation,
    work: &mut Work,
) -> Result<(Point, Point), String> {
    presentation.charge_measurement(source, doc, work)?;
    let mut preview = refreshed_preview(doc, hydrogens)?;
    normalize_native_states(doc, &mut preview)?;
    let boxes = saved_and_refreshed_boxes(doc, &preview)?;
    measured_bounds(doc, &boxes)
}

// The renderer uses four cubic circle segments. Their maximum radial
// overshoot is 0.0273%; highlights.rs reserves the same outward allowance.
pub(super) const CAPSULE_SCALE: f32 = 1.0003;

pub(super) fn highlight_radius(style: &crate::style::DrawingStyle) -> f32 {
    style.bond_length_world * style.bond_spacing_ratio * 1.5 + style.line_width() * 0.5
}

pub(super) fn bond_paint_radius(style: &crate::style::DrawingStyle) -> f32 {
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let bold = style.world(style.bold_width_pt);
    let line = style.line_width();
    // Quadruple rails are at most 1.5 spacings from their centerline, waves
    // reach 1.25 line widths, and Joins::cap clamps every miter to four half
    // widths. Bold width >= line width is a validated document invariant.
    // Junction far points stay along their incident segment with <=bold/2
    // transverse width, so the same endpoint/segment envelope contains them.
    let envelope = (spacing * 1.5 + bold * 0.5)
        .max(line * 1.25 + bold * 0.5)
        .max(bold * 2.);
    let highlight = highlight_radius(style).max(envelope + line) * CAPSULE_SCALE;
    // The dative arrowhead's fixed 10 by 3.5 world-unit wings also fit here.
    highlight.max(10_f32.hypot(3.5) + line)
}

pub(super) fn atom_bounds(
    a: &crate::document::Atom,
    doc: &Document,
) -> Result<Option<(Point, Point)>, String> {
    let mut bounds = crate::scene::atom_label_ink_bounds(a, doc);
    for part in crate::scientific::styled_mark_parts(a, &doc.drawing_style) {
        for p in part
            .commands
            .iter()
            .flat_map(crate::graphics::PathCommand::iter_points)
        {
            let pad = part.style.width() * 0.5;
            include(&mut bounds, p.offset(-pad, -pad), p.offset(pad, pad));
        }
    }
    if let Some(number) = &a.display.number {
        indicator_bounds(
            &mut bounds,
            a.position,
            &number.text,
            number.offset,
            &number.style,
        );
    }
    if a.map_num != 0 && a.display.mapping.show.unwrap_or(doc.atom_labels.maps) {
        indicator_bounds(
            &mut bounds,
            a.position,
            &a.map_num.to_string(),
            a.display.mapping.offset,
            &a.display.mapping.style,
        );
    }
    if a.display.stereo.show.unwrap_or(doc.atom_labels.stereo) {
        cip_bounds(
            &mut bounds,
            a.position,
            a.cip_label.as_deref(),
            a.display.stereo.offset,
            &a.display.stereo.style,
        );
    }
    if let Some((lo, hi)) = bounds {
        finite(lo)?;
        finite(hi)?;
    }
    Ok(bounds)
}

// Completed chemistry descriptors are the fixed single-character set emitted
// by stereo/cip/label.rs. Native refresh may restore them after caches are
// cleared, including on unrelated fixed components. Bound every enabled style
// without assigning CIP, inventing a label, or altering persisted caches.
pub(super) const FUTURE_CIP_LABELS: [&str; 12] = [
    "(R)", "(S)", "(r)", "(s)", "(E)", "(Z)", "(e)", "(z)", "(M)", "(P)", "(m)", "(p)",
];

fn cip_bounds(
    bounds: &mut Option<(Point, Point)>,
    anchor: Point,
    saved: Option<&str>,
    offset: Option<Point>,
    style: &crate::typography::TextStyle,
) {
    if let Some(label) = saved {
        indicator_bounds(bounds, anchor, &format!("({label})"), offset, style);
    }
    for label in FUTURE_CIP_LABELS {
        indicator_bounds(bounds, anchor, label, offset, style);
    }
}

fn indicator_bounds(
    bounds: &mut Option<(Point, Point)>,
    anchor: Point,
    text: &str,
    offset: Option<Point>,
    style: &crate::typography::TextStyle,
) {
    let height = crate::style::DEFAULT.world(style.size_pt);
    let width = crate::style::styled_text_width(text, height, style);
    // Automatic indicator candidates have five fixed radii, ending at 3.25h+6.
    // Their conservative envelope avoids a second label-placement pass.
    let radius = offset.map_or(height * 3.25 + 6., |_| 0.);
    let center = offset.map_or(anchor, |p| anchor.offset(p.x, p.y));
    include(
        bounds,
        center.offset(-width / 2. - radius, -height / 2. - radius),
        center.offset(width / 2. + radius, height / 2. + radius),
    );
    // Actual font ink may overhang its logical advance (italic/underline).
    // Automatic placement can choose any of the fixed candidate centers; grow
    // every actual ink box by the same maximum radius in each direction.
    let origin = center.offset(-width / 2., -height / 2.);
    for (lo, hi) in crate::style::text_ink_boxes(text, height, style) {
        include(
            bounds,
            origin.offset(lo.x - radius, lo.y - radius),
            origin.offset(hi.x + radius, hi.y + radius),
        );
    }
}

pub(super) fn reach(p: Point, bounds: Option<(Point, Point)>, ux: f32, uy: f32) -> f32 {
    // Projection of the positioned ink toward a neighbor. Using its direction
    // avoids needlessly lengthening bonds for hydrogens on the opposite side.
    bounds.map_or(0., |(lo, hi)| {
        let x = if ux >= 0. { hi.x } else { lo.x };
        let y = if uy >= 0. { hi.y } else { lo.y };
        ((x - p.x) * ux + (y - p.y) * uy).max(0.)
    })
}

pub(super) fn move_box(bounds: &mut Option<(Point, Point)>, old: Point, new: Point) {
    if let Some((lo, hi)) = bounds {
        *lo = lo.offset(new.x - old.x, new.y - old.y);
        *hi = hi.offset(new.x - old.x, new.y - old.y);
    }
}

pub(super) fn object_bounds(doc: &Document) -> Result<Option<(Point, Point)>, String> {
    let mut bounds = None;
    for a in &doc.annotations {
        // Exactly one shared typography layout: wrapping, rich scripts,
        // alignment and family fallback match annotation/caption painting.
        let layout = crate::typography::layout(&a.text, &a.format);
        let hi = a.position.offset(layout.width, layout.height);
        finite(hi)?;
        include(&mut bounds, a.position, hi);
        for fragment in &layout.fragments {
            let origin = a.position.offset(fragment.position.x, fragment.position.y);
            for (lo, hi) in
                crate::style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)
            {
                let lo = origin.offset(lo.x, lo.y);
                let hi = origin.offset(hi.x, hi.y);
                finite(lo)?;
                finite(hi)?;
                include(&mut bounds, lo, hi);
            }
        }
    }
    for a in &doc.arrows {
        // Bound the actual emitted cubic/Hermite control hull and head/mark
        // paths, including preset defaults. Round stroke joins/caps fit half
        // their width. Calling Arrow::bounds would adaptively flatten curves.
        for part in a.paths() {
            let pad = part.style.width() * 0.5;
            for p in part
                .commands
                .iter()
                .flat_map(crate::graphics::PathCommand::iter_points)
            {
                let lo = p.offset(-pad, -pad);
                let hi = p.offset(pad, pad);
                finite(lo)?;
                finite(hi)?;
                include(&mut bounds, lo, hi);
            }
        }
    }
    for b in &doc.bonds {
        if b.indicator.show.unwrap_or(doc.atom_labels.stereo) {
            let a = doc.atom(b.a).ok_or("Missing indicator endpoint")?.position;
            let z = doc.atom(b.b).ok_or("Missing indicator endpoint")?.position;
            cip_bounds(
                &mut bounds,
                Point::new((a.x + z.x) * 0.5, (a.y + z.y) * 0.5),
                b.cip_label.as_deref(),
                b.indicator.offset,
                &b.indicator.style,
            );
        }
    }
    for g in &doc.graphics {
        let (lo, hi) = g.bounds();
        finite(lo)?;
        finite(hi)?;
        include(&mut bounds, lo, hi);
    }
    Ok(bounds)
}

#[cfg(test)]
mod tests;
