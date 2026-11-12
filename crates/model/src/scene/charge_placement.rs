//! Placement of automatic charges around label ink and incident bonds.

use super::{
    Atom, Document, HydrogenIsotopeLabels, LabelFrame, Point, Primitive, label_end,
    label_ink_boxes, label_runs, label_trims_bonds, visible,
};

#[derive(Clone, Copy)]
pub(super) enum ChargeLayout {
    Automatic,
    Unplaced,
    Omitted,
}

/// Keep the usual superscript while it clears incident shafts, labels and marks.
/// Charge and any automatic radical dots travel together; explicit marks and
/// the chemical document are never moved. If no nearby raised slot clears the
/// bonds and label ink, retain the original position and genuine bond clipping.
/// An atom with a dative contact first tries the gaps between its bonds,
/// right-hand gaps widest first, then any gap widest first, and falls back to
/// them when no nearby slot clears.
pub(super) fn place_automatic_charge(
    a: &Atom,
    doc: &Document,
    isotopes: &HydrogenIsotopeLabels,
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
                label_ink_boxes(&label_runs(
                    other,
                    doc,
                    ChargeLayout::Omitted,
                    isotopes,
                    || visible(other, doc),
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
    let mut occupied = Vec::new();
    let mut reserve_marks = |atom: &Atom| {
        for part in crate::scientific::document_mark_parts(atom, doc) {
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
        occupied.extend(label_ink_boxes(&label_runs(
            other,
            doc,
            ChargeLayout::Unplaced,
            isotopes,
            || visible(other, doc),
        )));
    }
    let gap = frame.size * 0.12;
    let crowds_ink = |boxes: &[(Point, Point)], occupied: &[(Point, Point)]| {
        boxes.iter().any(|(lo, hi)| {
            occupied.iter().any(|(a, b)| {
                hi.x > a.x - gap && lo.x < b.x + gap && hi.y > a.y - gap && lo.y < b.y + gap
            })
        })
    };
    let moved = |offset: Point| -> Vec<_> {
        charge_boxes
            .iter()
            .map(|(lo, hi)| (lo.offset(offset.x, offset.y), hi.offset(offset.x, offset.y)))
            .collect()
    };
    let shift = |charge: &mut [Primitive], offset: Point| {
        for run in charge {
            if let Primitive::Text { position, .. } = run {
                *position = position.offset(offset.x, offset.y);
            }
        }
    };
    let neighbor_cores: Vec<_> = neighbors
        .iter()
        .flat_map(|other| {
            label_ink_boxes(&label_runs(
                other,
                doc,
                ChargeLayout::Omitted,
                isotopes,
                || visible(other, doc),
            ))
        })
        .collect();
    let distance_to = |boxes: &[(Point, Point)], core: &[(Point, Point)]| {
        boxes
            .iter()
            .flat_map(|(lo, hi)| {
                core.iter().map(move |(a, b)| {
                    let dx = (a.x - hi.x).max(lo.x - b.x).max(0.);
                    let dy = (a.y - hi.y).max(lo.y - b.y).max(0.);
                    dx * dx + dy * dy
                })
            })
            .fold(f32::INFINITY, f32::min)
    };
    // Nearer a neighbor's label than its own, a charge reads as the neighbor's.
    let ambiguous = |boxes: &[(Point, Point)]| {
        distance_to(boxes, &core_boxes) >= distance_to(boxes, &neighbor_cores)
    };
    // Contacts crowd a metal from every side, so its charge goes along the
    // bisector of a gap between bonds that has room, preferring the right
    // side, as near as it stays clear and unambiguously its own.
    let dative = doc
        .bonds
        .iter()
        .any(|b| b.is_dative() && (b.a == a.id || b.b == a.id) && doc.bond_visible(b.a, b.b));
    let mut ambiguous_gap = None;
    if dative {
        let near: Vec<_> = occupied.iter().chain(&core_boxes).copied().collect();
        let center = Point::new(
            (charge_lo.x + charge_hi.x) * 0.5,
            (charge_lo.y + charge_hi.y) * 0.5,
        );
        let clear = gap_directions(a, &neighbors)
            .into_iter()
            .flat_map(|(dx, dy)| {
                (0..=20).map(move |step| {
                    let t = step as f32 * frame.size * 0.1;
                    Point::new(
                        a.position.x + dx * t - center.x,
                        a.position.y + dy * t - center.y,
                    )
                })
            })
            .filter(|&offset| {
                let boxes = moved(offset);
                !blocks_shaft(&boxes) && !crowds_ink(&boxes, &near)
            });
        for offset in clear {
            if !ambiguous(&moved(offset)) {
                shift(charge, offset);
                return;
            }
            ambiguous_gap.get_or_insert(offset);
        }
    }
    if !blocks_shaft(&charge_boxes) && !crowds_ink(&charge_boxes, &occupied) {
        return;
    }
    // The usual superscript has established spacing from its own core.
    // Replacement slots must leave the same gap as neighboring labels.
    occupied.extend_from_slice(&core_boxes);
    let left = core_lo.x - gap - charge_hi.x;
    let center = (core_lo.x + core_hi.x - charge_lo.x - charge_hi.x) * 0.5;
    let mut best: Option<((bool, f32), Point)> = None;
    // At most 18 nearby slots. Raise rather than lowering a formal charge
    // into a subscript position; ties prefer the familiar right-hand side.
    for rise in 0..=5 {
        for dx in [0., left, center] {
            let offset = Point::new(dx, -(rise as f32) * frame.size * 0.25);
            let boxes = moved(offset);
            if blocks_shaft(&boxes) || crowds_ink(&boxes, &occupied) {
                continue;
            }
            // Prefer an unambiguous attachment to this core; crowded complexes
            // can still use the nearest clear slot if every slot is ambiguous.
            let score = (ambiguous(&boxes), offset.x * offset.x + offset.y * offset.y);
            if best.is_none_or(|(previous, _)| score < previous) {
                best = Some((score, offset));
            }
        }
    }
    // A crowded metal still prefers any clear gap to a charge on a shaft.
    if let Some(offset) = best.map(|(_, offset)| offset).or(ambiguous_gap) {
        shift(charge, offset);
    }
}

/// Unit bisectors of the angles between `a`'s bonds. The right-hand parts of
/// the gaps come first (a charge there reads "Co3+"), widest first and upper
/// first on a tie; then every whole gap, widest first.
fn gap_directions(a: &Atom, neighbors: &[&Atom]) -> Vec<(f32, f32)> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let mut angles: Vec<f32> = neighbors
        .iter()
        .map(|o| (o.position.y - a.position.y).atan2(o.position.x - a.position.x))
        .collect();
    angles.sort_by(f32::total_cmp);
    let Some(&first) = angles.first() else {
        return vec![];
    };
    let mut gaps: Vec<_> = angles
        .iter()
        .zip(angles.iter().skip(1).chain([&(first + TAU)]))
        .map(|(from, to)| (*from, to - from))
        .collect();
    let unit = |angle: f32| {
        let (dy, dx) = angle.sin_cos();
        (dx, dy)
    };
    // Gaps start in [-π, π] and span at most a turn, so they meet the right
    // half-plane (-π/2, π/2) at most in this turn and the next.
    let mut right: Vec<_> = gaps
        .iter()
        .flat_map(|&(from, width)| {
            [0., TAU].into_iter().filter_map(move |turn| {
                let low = from.max(turn - FRAC_PI_2);
                let high = (from + width).min(turn + FRAC_PI_2);
                (high > low)
                    .then_some(((high - low).to_degrees().round() as i32, (low + high) * 0.5))
            })
        })
        .collect();
    right.sort_by(|x, y| y.0.cmp(&x.0).then(unit(x.1).1.total_cmp(&unit(y.1).1)));
    gaps.sort_by(|x, y| y.1.total_cmp(&x.1));
    right
        .into_iter()
        .map(|(_, middle)| unit(middle))
        .chain(
            gaps.into_iter()
                .map(|(from, width)| unit(from + width * 0.5)),
        )
        .collect()
}
