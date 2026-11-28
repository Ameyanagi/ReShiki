//! Readable editable projections of several hydrogens bridging a metal pair.
use super::Model3d;
use crate::document::{Document, Point};
use std::collections::BTreeMap;

/// Separate overlapping bridge labels in the 2D drawing only. Frozen XYZ and
/// frame order stay on the model. Crowded terminal metal–H labels of the same
/// core move away from it; ordinary C–H and uncrowded terminal H stay put.
pub fn lay_out_bridges(doc: &mut Document, model: &Model3d) {
    let mut groups = BTreeMap::<(u64, u64), Vec<u64>>::new();
    let mut terminals = BTreeMap::<u64, Vec<u64>>::new();
    for hydrogen in doc
        .atoms
        .iter()
        .filter(|a| a.element == "H" && model.contains(a.id))
    {
        let mut neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == hydrogen.id {
                    Some(b.b)
                } else if b.b == hydrogen.id {
                    Some(b.a)
                } else {
                    None
                }
            })
            .collect();
        neighbors.sort_unstable();
        if let [metal] = neighbors.as_slice()
            && doc
                .atom(*metal)
                .is_some_and(crate::joining::coordination_metal)
        {
            terminals.entry(*metal).or_default().push(hydrogen.id);
        }
        let [a, b] = neighbors.as_slice() else {
            continue;
        };
        if [a, b].into_iter().all(|id| {
            doc.atom(*id)
                .is_some_and(crate::joining::coordination_metal)
        }) {
            groups.entry((*a, *b)).or_default().push(hydrogen.id);
        }
    }
    let clearance = doc.drawing_style.font_size() * 1.6;
    for ((a, b), mut hydrogens) in groups {
        if hydrogens.len() < 2 {
            continue;
        }
        let (a_id, b_id) = (a, b);
        let Some((a, b)) = doc.atom(a).zip(doc.atom(b)) else {
            continue;
        };
        let (a, b) = (a.position, b.position);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length = dx.hypot(dy);
        // Looking directly down the metal axis is a deliberate view, not a
        // planar bridge diagram. Do not invent an axis or move either metal.
        if length < clearance {
            continue;
        }
        let crowded = hydrogens.iter().enumerate().any(|(i, id)| {
            let Some(p) = doc.atom(*id).map(|a| a.position) else {
                return false;
            };
            [a, b]
                .into_iter()
                .any(|q| (p.x - q.x).hypot(p.y - q.y) < clearance)
                || hydrogens
                    .iter()
                    .skip(i + 1)
                    .filter_map(|id| doc.atom(*id))
                    .any(|q| (p.x - q.position.x).hypot(p.y - q.position.y) < clearance)
        });
        if crowded {
            // Keep front/back pairing stable across repeated flatten operations.
            hydrogens.sort_unstable();
            let middle = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
            for (i, id) in hydrogens.iter().enumerate() {
                let row = (i / 2 + 1) as f32;
                let distance = clearance * row * if i % 2 == 0 { -1. } else { 1. };
                if let Some(atom) = doc.atom_mut(*id) {
                    atom.position = Point::new(
                        middle.x - dy / length * distance,
                        middle.y + dx / length * distance,
                    );
                }
            }
        }
        for (metal, position, side) in [(a_id, a, -1.), (b_id, b, 1.)] {
            for (i, id) in terminals.get(&metal).into_iter().flatten().enumerate() {
                let Some(p) = doc.atom(*id).map(|a| a.position) else {
                    continue;
                };
                let crowded = [a, b].into_iter().any(|q| p.distance(q) < clearance)
                    || hydrogens
                        .iter()
                        .chain(terminals.values().flatten())
                        .filter(|other| *other != id)
                        .filter_map(|id| doc.atom(*id))
                        .any(|a| p.distance(a.position) < clearance);
                if !crowded {
                    continue;
                }
                let row = (i / 2 + 2) as f32;
                let offset = clearance * row * side * if i % 2 == 0 { 1. } else { -1. };
                if let Some(atom) = doc.atom_mut(*id) {
                    atom.position = Point::new(
                        position.x + side * dx / length * clearance * 0.3 - dy / length * offset,
                        position.y + side * dy / length * clearance * 0.3 + dx / length * offset,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models3d::{ForceField, Placed};

    #[test]
    fn bridge_layout_separates_labels_without_changing_geometry_or_other_hydrogens() {
        for count in [2, 4] {
            let mut doc = Document::default();
            let a = doc.add_atom("Ru", Point::new(-25., 0.));
            let b = doc.add_atom("Ru", Point::new(25., 0.));
            let mut bridges = Vec::new();
            for i in 0..count {
                let h = doc.add_atom("H", Point::new(0., i as f32 * 0.1));
                doc.add_bond(a, h, 1, "plain");
                doc.add_bond(b, h, 1, "plain");
                bridges.push(h);
            }
            let terminal = doc.add_atom("H", Point::new(-110., 0.));
            doc.add_bond(a, terminal, 1, "plain");
            let crowded_terminal = doc.add_atom("H", Point::new(24., 0.));
            doc.add_bond(b, crowded_terminal, 1, "plain");
            let carbon = doc.add_atom("C", Point::new(100., 0.));
            let ordinary = doc.add_atom("H", Point::new(130., 0.));
            doc.add_bond(carbon, ordinary, 1, "plain");
            let placed = doc
                .atoms
                .iter()
                .map(|a| Placed {
                    id: a.id,
                    xyz: [a.position.x, a.position.y, a.depth],
                })
                .collect();
            let model = Model3d::new(&doc, placed, vec![], ForceField::default());
            let before = doc.clone();
            lay_out_bridges(&mut doc, &model);
            assert_eq!(doc.bonds, before.bonds);
            assert_ne!(doc.atom(crowded_terminal), before.atom(crowded_terminal));
            assert!(
                doc.atom(crowded_terminal)
                    .unwrap()
                    .position
                    .distance(doc.atom(b).unwrap().position)
                    >= doc.drawing_style.font_size() * 1.6
            );
            for id in [a, b, terminal, carbon, ordinary] {
                assert_eq!(doc.atom(id), before.atom(id));
            }
            for (i, id) in bridges.iter().enumerate() {
                let p = doc.atom(*id).unwrap().position;
                assert!(bridges.iter().skip(i + 1).all(|id| {
                    let q = doc.atom(*id).unwrap().position;
                    (p.x - q.x).hypot(p.y - q.y) >= doc.drawing_style.font_size() * 1.6 - 1e-4
                }));
                assert_eq!(
                    doc.atom(*id).unwrap().depth,
                    before.atom(*id).unwrap().depth
                );
            }
            let once = doc.atoms.clone();
            lay_out_bridges(&mut doc, &model);
            assert_eq!(doc.atoms, once);
            assert_eq!(model.frame_positions().len(), before.atoms.len());
        }
    }
}
