//! Placement of automatic charges around label ink and incident bonds.

use super::{
    Atom, Document, LabelFrame, Point, Primitive, atom_label_runs_with_charge_layout, label_end,
    label_ink_boxes, label_trims_bonds,
};

#[derive(Clone, Copy)]
pub(super) enum ChargeLayout {
    Automatic,
    Unplaced,
    Omitted,
}

/// Keep the usual superscript unless its ink extends into an incident shaft.
/// Charge and any automatic radical dots travel together; explicit marks and
/// the chemical document are never moved. If no nearby raised slot clears the
/// bonds and label ink, retain the original position and genuine bond clipping.
pub(super) fn place_automatic_charge(
    a: &Atom,
    doc: &Document,
    frame: LabelFrame<'_>,
    core: &[Primitive],
    charge: &mut [Primitive],
) {
    let core_boxes = label_ink_boxes(core);
    let charge_boxes = label_ink_boxes(charge);
    let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
    let neighbors: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| (b.a == a.id || b.b == a.id) && doc.bond_visible(b.a, b.b))
        .filter_map(|b| doc.atom(if b.a == a.id { b.b } else { b.a }))
        .collect();
    let rays: Vec<_> = neighbors
        .iter()
        .copied()
        .filter_map(|other| {
            let length = a.position.distance(other.position);
            if length < 0.1 {
                return None;
            }
            let ux = (other.position.x - a.position.x) / length;
            let uy = (other.position.y - a.position.y) / length;
            let end = label_end(a.position, ux, uy, &core_boxes, length, margin);
            let trim = (end.x - a.position.x) * ux + (end.y - a.position.y) * uy;
            let other_core = if label_trims_bonds(other, doc, None) {
                label_ink_boxes(&atom_label_runs_with_charge_layout(
                    other,
                    doc,
                    ChargeLayout::Omitted,
                ))
            } else {
                vec![]
            };
            let other_end = label_end(other.position, -ux, -uy, &other_core, length, margin);
            let other_trim =
                (other.position.x - other_end.x) * ux + (other.position.y - other_end.y) * uy;
            // Both endpoint cores participate in genuine bond clipping.
            // Their jointly covered span has no shaft for charge to rescue.
            (length - trim - other_trim > 0.1).then_some((ux, uy, length, trim))
        })
        .collect();
    let blocks_shaft = |boxes: &[(Point, Point)]| {
        rays.iter().any(|&(ux, uy, length, trim)| {
            let end = label_end(a.position, ux, uy, boxes, length, margin);
            (end.x - a.position.x) * ux + (end.y - a.position.y) * uy > trim + 0.1
        })
    };
    if !blocks_shaft(&charge_boxes) {
        return;
    }
    let extent = |boxes: &[(Point, Point)]| {
        boxes.iter().copied().reduce(|(lo, hi), (a, b)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        })
    };
    let (Some((core_lo, core_hi)), Some((charge_lo, charge_hi))) =
        (extent(&core_boxes), extent(&charge_boxes))
    else {
        return;
    };
    let mut occupied = core_boxes;
    let mut reserve_marks = |atom: &Atom| {
        for part in crate::scientific::styled_mark_parts(atom, &doc.drawing_style) {
            let points: Vec<_> = part
                .commands
                .iter()
                .flat_map(crate::graphics::PathCommand::iter_points)
                .map(|p| (p, p))
                .collect();
            if let Some((lo, hi)) = extent(&points) {
                let pad = part.style.width() * 0.5;
                occupied.push((lo.offset(-pad, -pad), hi.offset(pad, pad)));
            }
        }
    };
    reserve_marks(a);
    // Even a core-covered bond's visible neighbor still paints its label and
    // user-positioned marks; only its shaft constraint was discarded above.
    for other in &neighbors {
        reserve_marks(other);
    }
    for other in &neighbors {
        // Unplaced neighbor runs avoid recursive charge/indicator placement.
        occupied.extend(label_ink_boxes(&atom_label_runs_with_charge_layout(
            other,
            doc,
            ChargeLayout::Unplaced,
        )));
    }
    let gap = frame.size * 0.12;
    let left = core_lo.x - gap - charge_hi.x;
    let center = (core_lo.x + core_hi.x - charge_lo.x - charge_hi.x) * 0.5;
    let mut best: Option<(f32, Point)> = None;
    // At most 18 nearby slots. Raise rather than lowering a formal charge
    // into a subscript position; ties prefer the familiar right-hand side.
    for rise in 0..=5 {
        for dx in [0., left, center] {
            let offset = Point::new(dx, -(rise as f32) * frame.size * 0.25);
            let boxes: Vec<_> = charge_boxes
                .iter()
                .map(|(lo, hi)| (lo.offset(offset.x, offset.y), hi.offset(offset.x, offset.y)))
                .collect();
            if blocks_shaft(&boxes)
                || boxes.iter().any(|(lo, hi)| {
                    occupied.iter().any(|(a, b)| {
                        hi.x > a.x - gap && lo.x < b.x + gap && hi.y > a.y - gap && lo.y < b.y + gap
                    })
                })
            {
                continue;
            }
            let distance = offset.x * offset.x + offset.y * offset.y;
            if best.is_none_or(|(previous, _)| distance < previous) {
                best = Some((distance, offset));
            }
        }
    }
    if let Some((_, offset)) = best {
        for run in charge {
            if let Primitive::Text { position, .. } = run {
                *position = position.offset(offset.x, offset.y);
            }
        }
    }
}
