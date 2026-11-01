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
        let mut best = (f32::INFINITY, f32::INFINITY, original);
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
                let mut penalty = 0.;
                for (other, (atom, bounds)) in
                    doc.atoms.iter().zip(&boxes).take(hydrogen).enumerate()
                {
                    let p = atom.position;
                    penalty += (0.5 - (point.x - p.x).hypot(point.y - p.y) / bond_length)
                        .max(0.)
                        .powi(2);
                    if p.x >= low.x && p.x <= high.x && p.y >= low.y && p.y <= high.y {
                        penalty += 1.;
                    }
                    if let Some((a, b)) = *bounds {
                        let overlap = (high.x.min(b.x + pad) - low.x.max(a.x - pad)).max(0.)
                            * (high.y.min(b.y + pad) - low.y.max(a.y - pad)).max(0.);
                        penalty += overlap / bond_length.powi(2);
                        if other != parent
                            && segment_hits_rect(
                                center,
                                point,
                                a.offset(-pad, -pad),
                                b.offset(pad, pad),
                            )
                        {
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
                // Clearance wins, then stay near the natural exterior ray.
                // Shorten a crowded terminal bond before making a large turn.
                let preference = (degrees as f32).abs() / 90. + (1. - length) * 0.2;
                if penalty < best.0 - 1e-7
                    || (penalty - best.0).abs() <= 1e-7 && preference < best.1
                {
                    best = (penalty, preference, point);
                }
            }
        }
        doc.atoms
            .get_mut(hydrogen)
            .ok_or_else(|| invalid("Missing fusion H atom"))?
            .position = best.2;
        let position = molecule
            .positions
            .get_mut(hydrogen)
            .ok_or_else(|| invalid("Missing fusion H position"))?;
        position.x = f64::from(best.2.x) / WORLD_PER_DEPICT_UNIT;
        position.y = -f64::from(best.2.y) / WORLD_PER_DEPICT_UNIT;
    }
    Ok(())
}
