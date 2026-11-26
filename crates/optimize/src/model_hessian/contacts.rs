//! Weak nonbonded stretch curvature for the Lindh Hessian.
//!
//! Packwood et al., J. Chem. Phys. 144, 164109 (2016), Eq. (13),
//! doi:10.1063/1.4947024 \[Packwood2016Contacts\], give the exponential distance weight with A = 3.
//! For molecules we take R as the sum of the pair's van der Waals radii
//! and select k = 0.001 exp[−3(r/R − 1)] Eh/bohr² for r < 1.2 R.
//! The amplitude and hard cutoff are model parameters. The radial Wilson
//! term k b bᵀ preserves the rigid-motion null space (Lindh et al.,
//! doi:10.1016/0009-2614(95)00646-L) \[Lindh1995Hessian\].
//!
//! The TRIC covalent criterion excludes bonded pairs and pairs sharing a
//! bonded neighbor. Its cutoff is 1.3 times the sum of Pyykkö–Atsumi
//! single-bond radii (2009), doi:10.1002/chem.200800987 \[Pyykko2009Radii\]. The van der Waals
//! radii cover the 44 main-group elements tabulated by Mantina et al.,
//! J. Phys. Chem. A 113, 5806–5812 (2009), Table 12, doi:10.1021/jp8111556
//! \[Mantina2009Radii\]. Pairs involving other elements retain the base Lindh model.
//!
//! Full references: docs/references/xtb.md.

use crate::ANGSTROM_PER_BOHR;

const CONTACT_STIFFNESS: f64 = 1e-3;
const CONTACT_RANGE: f64 = 3.;
const CONTACT_CUTOFF: f64 = 1.2;

// Picometers, indexed by atomic number minus one. These are molecular
// single-bond radii, including the paper's predicted superheavy-element values.
const COVALENT_PM: [u16; 118] = [
    32, 46, // H–He
    133, 102, 85, 75, 71, 63, 64, 67, // Li–Ne
    155, 139, 126, 116, 111, 103, 99, 96, // Na–Ar
    196, 171, 148, 136, 134, 122, 119, 116, 111, 110, 112, 118, 124, 121, 121, 116, 114,
    117, // K–Kr
    210, 185, 163, 154, 147, 138, 128, 125, 125, 120, 128, 136, 142, 140, 140, 136, 133,
    131, // Rb–Xe
    232, 196, 180, 163, 176, 174, 173, 172, 168, 169, 168, 167, 166, 165, 164, 170, 162, 152, 146,
    137, 131, 129, 122, 123, 124, 133, 144, 144, 151, 145, 147, 142, // Cs–Rn
    223, 201, 186, 175, 169, 170, 171, 172, 166, 166, 168, 168, 165, 167, 173, 176, 161, 157, 149,
    143, 141, 134, 129, 128, 121, 122, 136, 143, 162, 175, 165, 157, // Fr–Og
];

pub(crate) const BOND_FACTOR: f64 = 1.3;

pub(crate) fn covalent_radius(number: u8) -> f64 {
    f64::from(COVALENT_PM[usize::from(number) - 1]) * 0.01 / ANGSTROM_PER_BOHR
}

/// A radial stretch row has -unit on a and +unit on b.
pub(crate) struct Contact {
    pub(crate) a: usize,
    pub(crate) b: usize,
    pub(crate) unit: [f64; 3],
    pub(crate) force: f64,
}

impl Contact {
    pub(crate) fn free_row(&self, free: &[usize]) -> Vec<f64> {
        free.iter()
            .map(|&i| {
                let sign = if i / 3 == self.a {
                    -1.
                } else if i / 3 == self.b {
                    1.
                } else {
                    0.
                };
                sign * self.unit[i % 3]
            })
            .collect()
    }
}

/// Inputs have already passed the coordinate-model validation. Neither these
/// auxiliary stretches nor their absence change the covalent fragment graph.
pub(crate) fn contact_stretches(numbers: &[u8], x: &[f64]) -> Vec<Contact> {
    let mut graph = vec![Vec::new(); numbers.len()];
    for a in 0..numbers.len() {
        for b in a + 1..numbers.len() {
            let r = distance(x, a, b);
            if r <= BOND_FACTOR * (covalent_radius(numbers[a]) + covalent_radius(numbers[b])) {
                graph[a].push(b);
                graph[b].push(a);
            }
        }
    }
    let mut contacts = Vec::new();
    for a in 0..numbers.len() {
        let Some(ra) = vdw_radius(numbers[a]) else {
            continue;
        };
        for b in a + 1..numbers.len() {
            let Some(rb) = vdw_radius(numbers[b]) else {
                continue;
            };
            if graph[a].contains(&b) || graph[a].iter().any(|&c| graph[c].contains(&b)) {
                continue;
            }
            let r = distance(x, a, b);
            let reference = ra + rb;
            if r >= CONTACT_CUTOFF * reference {
                continue;
            }
            contacts.push(Contact {
                a,
                b,
                unit: std::array::from_fn(|k| (x[3 * b + k] - x[3 * a + k]) / r),
                force: CONTACT_STIFFNESS * (-CONTACT_RANGE * (r / reference - 1.)).exp(),
            });
        }
    }
    contacts
}

fn distance(x: &[f64], a: usize, b: usize) -> f64 {
    (0..3)
        .map(|k| (x[3 * a + k] - x[3 * b + k]).powi(2))
        .sum::<f64>()
        .sqrt()
}

// Mantina et al. Table 12, in Å, converted once at the point of use.
fn vdw_radius(number: u8) -> Option<f64> {
    let radius = match number {
        1 => 1.10,
        2 => 1.40,
        3 => 1.81,
        4 => 1.53,
        5 => 1.92,
        6 => 1.70,
        7 => 1.55,
        8 => 1.52,
        9 => 1.47,
        10 => 1.54,
        11 => 2.27,
        12 => 1.73,
        13 => 1.84,
        14 => 2.10,
        15 => 1.80,
        16 => 1.80,
        17 => 1.75,
        18 => 1.88,
        19 => 2.75,
        20 => 2.31,
        31 => 1.87,
        32 => 2.11,
        33 => 1.85,
        34 => 1.90,
        35 => 1.83,
        36 => 2.02,
        37 => 3.03,
        38 => 2.49,
        49 => 1.93,
        50 => 2.17,
        51 => 2.06,
        52 => 2.06,
        53 => 1.98,
        54 => 2.16,
        55 => 3.43,
        56 => 2.68,
        81 => 1.96,
        82 => 2.02,
        83 => 2.07,
        84 => 1.97,
        85 => 2.02,
        86 => 2.20,
        87 => 3.48,
        88 => 2.83,
        _ => return None,
    };
    Some(radius / ANGSTROM_PER_BOHR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_radii_are_converted_to_bohr_without_extrapolation() {
        for (z, radius) in [(1, 1.10), (6, 1.70), (18, 1.88), (35, 1.83), (88, 2.83)] {
            assert_eq!(vdw_radius(z), Some(radius / ANGSTROM_PER_BOHR));
        }
        assert_eq!(vdw_radius(26), None);
        assert_eq!(vdw_radius(118), None);
        assert_eq!(covalent_radius(1), 0.32 / ANGSTROM_PER_BOHR);
        assert_eq!(covalent_radius(118), 1.57 / ANGSTROM_PER_BOHR);
    }

    #[test]
    fn contact_window_has_the_documented_amplitude_decay_and_cutoff() {
        let reference = 2. * 1.88 / ANGSTROM_PER_BOHR;
        for ratio in [0.9, 1., 1.1] {
            let x = [0., 0., 0., ratio * reference, 0., 0.];
            let contacts = contact_stretches(&[18, 18], &x);
            assert_eq!(contacts.len(), 1);
            assert!((contacts[0].force - 0.001 * (-3. * (ratio - 1.)).exp()).abs() < 1e-15);
            assert_eq!(contacts[0].unit, [1., 0., 0.]);
        }
        for ratio in [1.2, 1.3] {
            assert!(
                contact_stretches(&[18, 18], &[0., 0., 0., ratio * reference, 0., 0.]).is_empty()
            );
        }
        assert!(contact_stretches(&[26, 18], &[0., 0., 0., reference, 0., 0.]).is_empty());
    }

    #[test]
    fn covalent_and_one_three_pairs_are_excluded_but_one_four_contacts_remain() {
        // Three 2.8-bohr bonds in a zigzag, with every 1–3 pair inside
        // the contact window but outside the covalent bond cutoff.
        let height = 2.8 * 3_f64.sqrt() / 2.;
        let x = [0., 0., 0., 2.8, 0., 0., 4.2, height, 0., 7., height, 0.];
        let contacts = contact_stretches(&[6; 4], &x);
        assert_eq!(
            contacts.iter().map(|c| (c.a, c.b)).collect::<Vec<_>>(),
            [(0, 3)]
        );
        assert!(contacts[0].force > 0.);
    }
}
