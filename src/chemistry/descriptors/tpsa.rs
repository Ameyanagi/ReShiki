//! Adapted from RDKit MolSurf.cpp getTPSAAtomContribs (2026.03.6).
//! Copyright (C) 2007-2023 Greg Landrum. BSD-3-Clause; see licenses/rdkit/.
use super::{Target, at};

pub(super) fn contributions(target: &Target, include_s_p: bool) -> Result<Vec<f64>, String> {
    let mut result = vec![0.; target.nodes.len()];
    for (id, node) in target.nodes.iter().enumerate() {
        if !(matches!(node.number, 7 | 8) || include_s_p && matches!(node.number, 15 | 16)) {
            continue;
        }
        let (mut neighbors, mut single, mut double, mut triple, mut aromatic) =
            (0usize, 0u32, 0u32, 0u32, 0u32);
        for &(other, edge) in at(&target.adjacent, id)? {
            if at(&target.nodes, other)?.number == 1 {
                continue;
            }
            neighbors += 1;
            let edge = at(&target.edges, edge)?;
            if edge.aromatic {
                aromatic += 1;
            } else {
                match edge.order {
                    1 => single += 1,
                    2 => double += 1,
                    3 => triple += 1,
                    _ => {}
                }
            }
        }
        let h = node.hydrogens;
        let charge = node.charge;
        let triangle = *at(&target.triangles, id)?;
        let value = match node.number {
            7 => match (neighbors, h, charge, single, double, triple, aromatic) {
                (1, 0, 0, _, _, 1, _) => 23.79,
                (1, 1, 0, _, 1, _, _) => 23.85,
                (1, 2, 0, 1, _, _, _) => 26.02,
                (1, 2, 1, _, 1, _, _) => 25.59,
                (1, 3, 1, 1, _, _, _) => 27.64,
                (2, 0, 0, 1, 1, _, _) => 12.36,
                (2, 0, 0, _, 1, 1, _) => 13.60,
                (2, 1, 0, 2, _, _, _) => {
                    if triangle {
                        21.94
                    } else {
                        12.03
                    }
                }
                (2, 0, 1, 1, _, 1, _) => 4.36,
                (2, 1, 1, 1, 1, _, _) => 13.97,
                (2, 2, 1, 2, _, _, _) => 16.61,
                (2, 0, 0, _, _, _, 2) => 12.89,
                (2, 1, 0, _, _, _, 2) => 15.79,
                (2, 1, 1, _, _, _, 2) => 14.14,
                (3, 0, 0, 3, _, _, _) => {
                    if triangle {
                        3.01
                    } else {
                        3.24
                    }
                }
                (3, 0, 0, 1, 2, _, _) => 11.68,
                (3, 0, 1, 2, 1, _, _) => 3.01,
                (3, 1, 1, 3, _, _, _) => 4.44,
                (3, 0, 0, _, _, _, 3) => 4.41,
                (3, 0, 0, 1, _, _, 2) => 4.93,
                (3, 0, 0, _, 1, _, 2) => 8.39,
                (3, 0, 1, _, _, _, 3) => 4.10,
                (3, 0, 1, 1, _, _, 2) => 3.88,
                (4, 0, 1, 4, _, _, _) => 0.,
                _ => (30.5 - neighbors as f64 * 8.2 + f64::from(h) * 1.5).max(0.),
            },
            8 => match (neighbors, h, charge, single, double, aromatic) {
                (1, 0, 0, _, 1, _) => 17.07,
                (1, 1, 0, 1, _, _) => 20.23,
                (1, 0, -1, 1, _, _) => 23.06,
                (2, 0, 0, 2, _, _) => {
                    if triangle {
                        12.53
                    } else {
                        9.23
                    }
                }
                (2, 0, 0, _, _, 2) => 13.14,
                _ => (28.5 - neighbors as f64 * 8.6 + f64::from(h) * 1.5).max(0.),
            },
            15 => match (neighbors, h, charge, single, double) {
                (2, 0, 0, 1, 1) => 34.14,
                (3, 0, 0, 3, _) => 13.59,
                (3, 1, 0, 2, 1) => 23.47,
                (4, 0, 0, 3, 1) => 9.81,
                _ => 0.,
            },
            16 => match (neighbors, h, charge, single, double, aromatic) {
                (1, 0, 0, _, 1, _) => 32.09,
                (1, 1, 0, 1, _, _) => 38.80,
                (2, 0, 0, 2, _, _) => 25.30,
                (2, 0, 0, _, _, 2) => 28.24,
                (3, 0, 0, _, 1, 2) => 21.70,
                (3, 0, 0, 2, 1, _) => 19.21,
                (4, 0, 0, 2, 2, _) => 8.38,
                _ => 0.,
            },
            _ => return Err("Invalid TPSA atom selection".into()),
        };
        *result.get_mut(id).ok_or("Missing TPSA atom")? = value;
    }
    Ok(result)
}
