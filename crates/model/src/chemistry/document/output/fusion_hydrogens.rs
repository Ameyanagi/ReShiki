//! Clear generated fusion H labels before assigning their stereobonds.
use super::*;
use crate::{atom_labels::segment_hits_rect, scene::atom_label_ink_bounds};

fn distance(p: Point, a: Point, b: Point) -> f32 {
    let (x, y) = (b.x - a.x, b.y - a.y);
    let t = (((p.x - a.x) * x + (p.y - a.y) * y) / (x * x + y * y).max(1e-20)).clamp(0., 1.);
    (p.x - a.x - t * x).hypot(p.y - a.y - t * y)
}

fn cross(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

pub(super) fn place(
    molecule: &mut Molecule,
    parents: &[usize],
    dummy_labels: &[Option<String>],
    options: DrawingOptions,
) -> Result<(), Error> {
    // Measure the same label ink used by the final drawing, including inline
    // hydrogens, charges and named dummy atoms. No stereo assignment runs here.
    let mut doc = reconstruct(
        molecule.clone(),
        molecule,
        None,
        &HashMap::new(),
        Some(dummy_labels),
        DrawingOptions {
            wavy_unknown_alkenes: false,
            ..options
        },
    )?
    .document;
    let first = doc.atoms.len() - parents.len();
    let bond_length = (1.5 * WORLD_PER_DEPICT_UNIT) as f32;
    let pad = doc.drawing_style.world(doc.drawing_style.bold_width_pt) * 0.5;
    let spacing = bond_length * doc.drawing_style.bond_spacing_ratio;
    for (index, &parent) in parents.iter().enumerate() {
        // Remeasure: earlier H placements can move inline H labels.
        let boxes = doc
            .atoms
            .iter()
            .map(|a| atom_label_ink_bounds(a, &doc))
            .collect::<Vec<_>>();
        let hydrogen = first + index;
        let center = at(&doc.atoms, parent)?.position;
        let original = at(&doc.atoms, hydrogen)?.position;
        let vector = Point::new(original.x - center.x, original.y - center.y);
        let (lo, hi) = at(&boxes, hydrogen)?.ok_or_else(|| invalid("Missing fusion H label"))?;
        let mut best = (false, f32::INFINITY, f32::INFINITY, original);
        for length in [1., 0.9, 0.8, 0.7, 0.6, 0.5] {
            for step in 0_i32..=16 {
                let degrees = ((step + 1) / 2) * if step % 2 == 0 { -10 } else { 10 };
                let (sin, cos) = (degrees as f32).to_radians().sin_cos();
                let point = center.offset(
                    length * (vector.x * cos - vector.y * sin),
                    length * (vector.x * sin + vector.y * cos),
                );
                let (dx, dy) = (point.x - original.x, point.y - original.y);
                let (low, high) = (lo.offset(dx, dy), hi.offset(dx, dy));
                let mut clear = true;
                let mut penalty = 0.;
                // Reserve unplaced H labels too, so an earlier H leaves room.
                for (other, (atom, bounds)) in doc.atoms.iter().zip(&boxes).enumerate() {
                    if other == hydrogen {
                        continue;
                    }
                    let p = atom.position;
                    let separation = (point.x - p.x).hypot(point.y - p.y) / bond_length;
                    clear &= separation >= 0.5 - 1e-6;
                    penalty += (0.5 - separation).max(0.).powi(2);
                    if p.x >= low.x && p.x <= high.x && p.y >= low.y && p.y <= high.y {
                        clear = false;
                        penalty += 1.;
                    }
                    if let Some((a, b)) = *bounds {
                        // Labels need a visible gap, or neighbouring H read as "HH".
                        let gap = bond_length * 0.25;
                        let separation = (low.x - b.x)
                            .max(a.x - high.x)
                            .max(0.)
                            .hypot((low.y - b.y).max(a.y - high.y).max(0.));
                        clear &= separation >= gap;
                        penalty += ((gap - separation) / bond_length).max(0.).powi(2);
                        if other != parent
                            && segment_hits_rect(
                                center,
                                point,
                                a.offset(-pad, -pad),
                                b.offset(pad, pad),
                            )
                        {
                            clear = false;
                            penalty += 1.;
                        }
                    }
                    if other != parent {
                        penalty += (0.15 - distance(p, center, point) / bond_length)
                            .max(0.)
                            .powi(2);
                    }
                }
                for bond in &molecule.state.graph.bonds {
                    // Only the original molecule and hydrogens already placed
                    // are obstacles; reserve each selected H before the next.
                    if bond.a >= hydrogen || bond.b >= hydrogen {
                        continue;
                    }
                    let (a, b) = (
                        at(&doc.atoms, bond.a)?.position,
                        at(&doc.atoms, bond.b)?.position,
                    );
                    let bond_pad = pad.max(
                        spacing
                            * f32::from(bond.order.saturating_sub(1).max(u8::from(bond.aromatic))),
                    );
                    if segment_hits_rect(
                        a,
                        b,
                        low.offset(-bond_pad, -bond_pad),
                        high.offset(bond_pad, bond_pad),
                    ) {
                        clear = false;
                        penalty += 1.;
                    }
                    if bond.a != parent && bond.b != parent {
                        let crossing = cross(a, b, center) * cross(a, b, point) < 0.
                            && cross(center, point, a) * cross(center, point, b) < 0.;
                        let gap = if crossing {
                            0.
                        } else {
                            distance(center, a, b)
                                .min(distance(point, a, b))
                                .min(distance(a, center, point))
                                .min(distance(b, center, point))
                        };
                        penalty += ((pad + bond_pad - gap) / bond_length).max(0.).powi(2);
                    }
                }
                // Clear atoms, labels and bonds are mandatory when possible; otherwise
                // keep the least-bad placement, then prefer the exterior ray.
                // Shorten a crowded terminal bond before making a large turn.
                let preference = (degrees as f32).abs() / 90. + (1. - length) * 0.2;
                if clear && !best.0
                    || clear == best.0
                        && (penalty < best.1 - 1e-7
                            || (penalty - best.1).abs() <= 1e-7 && preference < best.2)
                {
                    best = (clear, penalty, preference, point);
                }
            }
        }
        doc.atoms
            .get_mut(hydrogen)
            .ok_or_else(|| invalid("Missing fusion H atom"))?
            .position = best.3;
        let position = molecule
            .positions
            .get_mut(hydrogen)
            .ok_or_else(|| invalid("Missing fusion H position"))?;
        position.x = f64::from(best.3.x) / WORLD_PER_DEPICT_UNIT;
        position.y = -f64::from(best.3.y) / WORLD_PER_DEPICT_UNIT;
    }
    Ok(())
}
