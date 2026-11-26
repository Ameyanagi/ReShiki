//! Molecular-orbital levels: spin columns, names around the frontier,
//! and degenerate groups, shared by result views and drawing objects.
use reshiki_qcdata::{Beta, Orbitals, Spin};
use serde::{Deserialize, Serialize};

/// CODATA 2022, as the worker converts its gap.
pub const EV_PER_HARTREE: f64 = 27.211386245981;
/// Levels closer than this (eV) are one degenerate group.
pub const DEGENERATE_EV: f64 = 0.005;

/// One spin's molecular-orbital spectrum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    /// The spin whose coefficient rows hold these orbitals: alpha unless
    /// the result has its own beta orbitals.
    pub spin: Spin,
    /// "" for a closed shell, else "α" or "β".
    pub label: String,
    pub energies_ev: Vec<f64>,
    /// Electrons in each level (0 to 2 for a closed shell, else 0 to 1);
    /// empty when the source gave none.
    pub occupations: Vec<f64>,
    /// Occupied levels; the HOMO is the one below.
    pub occupied: Option<usize>,
}

impl Column {
    /// "HOMO", "HOMO−1", "LUMO+2", or "MO 7" without occupations.
    pub fn name(&self, index: usize) -> String {
        let name = match self.occupied {
            None => format!("MO {}", index + 1),
            Some(n) => match index.checked_sub(n) {
                None if index + 1 == n => "HOMO".into(),
                None => format!("HOMO−{}", n - 1 - index),
                Some(0) => "LUMO".into(),
                Some(above) => format!("LUMO+{above}"),
            },
        };
        if self.label.is_empty() {
            name
        } else {
            format!("{} {name}", self.label)
        }
    }

    pub fn homo(&self) -> Option<usize> {
        self.occupied?.checked_sub(1)
    }

    pub fn lumo(&self) -> Option<usize> {
        self.occupied.filter(|&n| n < self.energies_ev.len())
    }

    /// The degenerate group of `index`, `index` included.
    pub fn group(&self, index: usize) -> std::ops::RangeInclusive<usize> {
        let e = &self.energies_ev;
        let mut lo = index;
        while let Some(previous) = lo.checked_sub(1)
            && let Some((here, before)) = e.get(lo).zip(e.get(previous))
            && (here - before).abs() < DEGENERATE_EV
        {
            lo = previous;
        }
        let mut hi = index;
        while let Some(next) = hi.checked_add(1)
            && let Some((after, here)) = e.get(next).zip(e.get(hi))
            && (after - here).abs() < DEGENERATE_EV
        {
            hi = next;
        }
        lo..=hi
    }
}

/// The spin columns of `orbitals`: one for a closed shell, α and β for an
/// open shell (sharing alpha's orbitals unless beta has its own).
pub fn columns(orbitals: &Orbitals) -> Vec<Column> {
    let alpha = &orbitals.alpha;
    let ev = |levels: &[f64]| {
        levels
            .iter()
            .map(|e| e * EV_PER_HARTREE)
            .collect::<Vec<_>>()
    };
    // Occupied levels from the electron count, as the Summary counts them:
    // thermal smearing does not move the frontier.
    let count = |occupations: &[f64], levels: usize, closed: bool| {
        (occupations.len() == levels).then(|| {
            let electrons = occupations.iter().sum::<f64>().round() as usize;
            if closed {
                electrons.div_ceil(2)
            } else {
                electrons
            }
        })
    };
    let n = alpha.energies_hartree.len();
    let column = |spin, label, energies: &[f64], occupations: &[f64], closed| Column {
        spin,
        label: String::from(label),
        energies_ev: ev(energies),
        occupations: occupations.to_vec(),
        occupied: count(occupations, energies.len(), closed),
    };
    let mut columns = match &orbitals.beta {
        Beta::Restricted => vec![column(
            Spin::Alpha,
            "",
            &alpha.energies_hartree,
            &alpha.occupations,
            true,
        )],
        Beta::SameOrbitals { occupations } => vec![
            column(
                Spin::Alpha,
                "α",
                &alpha.energies_hartree,
                &alpha.occupations,
                false,
            ),
            Column {
                occupied: count(occupations, n, false),
                ..column(
                    Spin::Alpha,
                    "β",
                    &alpha.energies_hartree,
                    occupations,
                    false,
                )
            },
        ],
        Beta::Unrestricted(beta) => vec![
            column(
                Spin::Alpha,
                "α",
                &alpha.energies_hartree,
                &alpha.occupations,
                false,
            ),
            column(
                Spin::Beta,
                "β",
                &beta.energies_hartree,
                &beta.occupations,
                false,
            ),
        ],
    };
    // A spectrum without levels (or with energies too large to draw) has
    // nothing to choose.
    columns.retain(|c| !c.energies_ev.is_empty() && c.energies_ev.iter().all(|e| e.is_finite()));
    columns
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki_qcdata::OrbitalSet;

    fn set(energies: &[f64], occupations: &[f64]) -> OrbitalSet {
        OrbitalSet {
            energies_hartree: energies.to_vec(),
            occupations: occupations.to_vec(),
            symmetries: vec![],
            rows: vec![],
            coefficients: vec![],
        }
    }

    fn closed(energies: &[f64], occupations: &[f64]) -> Orbitals {
        Orbitals {
            basis: None,
            core_electrons: vec![],
            alpha: set(energies, occupations),
            beta: Beta::Restricted,
        }
    }

    #[test]
    fn a_closed_shell_names_levels_around_the_gap_and_groups_degenerate_ones() {
        let columns = columns(&closed(
            &[-0.6, -0.4, -0.4, 0.1, 0.1, 0.3],
            &[2., 2., 2., 0., 0., 0.],
        ));
        assert_eq!(columns.len(), 1);
        let column = &columns[0];
        assert_eq!((column.homo(), column.lumo()), (Some(2), Some(3)));
        assert_eq!(column.name(2), "HOMO");
        assert_eq!(column.name(1), "HOMO−1");
        assert_eq!(column.name(4), "LUMO+1");
        assert_eq!(column.group(2), 1..=2);
        assert_eq!(column.group(3), 3..=4);
        assert_eq!(column.group(0), 0..=0);
    }

    #[test]
    fn an_open_shell_has_alpha_and_beta_columns() {
        // A doublet sharing orbitals: alpha fills one more level.
        let orbitals = Orbitals {
            beta: Beta::SameOrbitals {
                occupations: vec![1., 0., 0.],
            },
            ..closed(&[-0.5, -0.3, 0.2], &[1., 1., 0.])
        };
        let split = columns(&orbitals);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].name(1), "α HOMO");
        assert_eq!(split[1].name(1), "β LUMO");
        assert_eq!(split[1].spin, Spin::Alpha, "beta shares alpha's rows");
        let unrestricted = Orbitals {
            beta: Beta::Unrestricted(set(&[-0.45, 0.1, 0.25], &[1., 0., 0.])),
            ..orbitals
        };
        let split = columns(&unrestricted);
        assert_eq!(split[1].spin, Spin::Beta);
        assert_eq!(split[1].homo(), Some(0));
        assert_eq!(split[1].energies_ev[0], -0.45 * EV_PER_HARTREE);
    }

    #[test]
    fn levels_without_occupations_are_numbered() {
        let column = &columns(&closed(&[-0.5, 0.2], &[]))[0];
        assert_eq!(column.name(0), "MO 1");
        assert_eq!(column.homo(), None);
    }
}
