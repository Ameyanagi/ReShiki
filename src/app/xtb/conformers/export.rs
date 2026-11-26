//! Conformer ensembles as files: multi-frame XYZ and SDF with energies.

use std::collections::HashMap;

use reshiki::document::Document;
use reshiki::qc_input::{SdfEntry, write_sdf, write_xyz_trajectory};
use reshiki::qcdata::{CalcResult, Frame, Trajectory};

const KCAL_PER_HARTREE: f64 = 627.509_474_063;
const RT_KCAL: f64 = 0.001_987_204_258_640_832 * 298.15;
const NO_SAVED_POSITIONS: &str = "This result has no saved conformer positions.";

/// Drawing bonds in frame order, with each model hydrogen joined to its nearest linked atom.
pub(crate) fn frame_bonds(
    doc: &Document,
    atoms: &[Option<u64>],
    positions: &[[f64; 3]],
) -> Vec<(usize, usize, u8)> {
    let linked: HashMap<_, _> = atoms
        .iter()
        .enumerate()
        .filter_map(|(index, id)| id.map(|id| (id, index)))
        .collect();
    let mut bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|bond| {
            let (&a, &b) = (linked.get(&bond.a)?, linked.get(&bond.b)?);
            (a != b).then_some((a.min(b), a.max(b), bond.order))
        })
        .collect();

    let located_atoms = atoms.iter().zip(positions).enumerate();
    let linked_positions: Vec<_> = located_atoms
        .clone()
        .filter_map(|(index, (id, position))| id.is_some().then_some((index, position)))
        .collect();
    for (hydrogen, (_, position)) in located_atoms.filter(|(_, (id, _))| id.is_none()) {
        let nearest = linked_positions
            .iter()
            .map(|&(index, candidate)| {
                let squared_distance = position
                    .iter()
                    .zip(candidate)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>();
                (index, squared_distance)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((neighbor, _)) = nearest {
            bonds.push((hydrogen.min(neighbor), hydrogen.max(neighbor), 1));
        }
    }

    bonds.sort_by_key(|&(a, b, _)| (a, b));
    bonds.dedup_by_key(|bond| (bond.0, bond.1));
    bonds
}

/// Boltzmann fractions in input order for finite energies at 298.15 K.
pub(crate) fn populations(energies_hartree: &[f64]) -> Vec<f64> {
    let minimum = energies_hartree
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let mut weights: Vec<_> = energies_hartree
        .iter()
        .map(|energy| (-(energy - minimum) * KCAL_PER_HARTREE / RT_KCAL).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    for weight in &mut weights {
        *weight /= total;
    }
    weights
}

/// Export every saved conformer as XYZ, lowest energy first.
pub(crate) fn xyz(result: &CalcResult, title: &str) -> Result<String, String> {
    let (path, ranked) = ranked_trajectory(result)?;
    let labeled = Trajectory {
        numbers: path.numbers.clone(),
        isotopes: path.isotopes.clone(),
        masses_amu: path.masses_amu.clone(),
        charge: path.charge,
        multiplicity: path.multiplicity,
        positions_angstrom: ranked
            .iter()
            .flat_map(|conformer| conformer.frame.positions_angstrom.iter().flatten().copied())
            .collect(),
        // The label supplies the entire comment, including the energy.
        energies_hartree: vec![],
        labels: ranked
            .iter()
            .enumerate()
            .map(|(index, conformer)| {
                let rank = index + 1;
                let RankedConformer {
                    step,
                    energy,
                    delta_kcal,
                    percent,
                    ..
                } = conformer;
                let cluster = path
                    .cluster_sizes
                    .get(*step)
                    .map_or_else(String::new, |size| format!(" · cluster {size}"));
                // charge= and mult= as extended XYZ reads them back; the
                // energy last, as ReShiki and xtb tools read it.
                format!(
                    "{title} conformer {rank} · ΔE {delta_kcal:.2} kcal/mol · {percent:.1} %{cluster} · charge={} mult={} · E = {energy:.10} Eh",
                    path.charge, path.multiplicity
                )
            })
            .collect(),
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    };
    write_xyz_trajectory(&labeled, title).map_err(|error| error.to_string())
}

/// Export every saved conformer as a V3000 SDF record with drawing bonds and charges.
pub(crate) fn sdf(
    doc: &Document,
    atoms: &[Option<u64>],
    result: &CalcResult,
    title: &str,
) -> Result<String, String> {
    let (path, ranked) = ranked_trajectory(result)?;
    let first = ranked.first().ok_or(NO_SAVED_POSITIONS)?;
    let bonds = frame_bonds(doc, atoms, &first.frame.positions_angstrom);
    let formal_charges: Vec<_> = atoms
        .iter()
        .map(|id| id.and_then(|id| doc.atom(id)).map_or(0, |atom| atom.charge))
        .collect();
    let records: Vec<_> = ranked
        .iter()
        .enumerate()
        .map(|(index, conformer)| {
            let RankedConformer {
                step,
                energy,
                delta_kcal,
                percent,
                frame,
            } = conformer;
            let rank = index + 1;
            let mut items = vec![
                ("RANK".into(), rank.to_string()),
                ("ENERGY_HARTREE".into(), format!("{energy:.10}")),
                ("DELTA_E_KCAL_MOL".into(), format!("{delta_kcal:.3}")),
                ("BOLTZMANN_PERCENT".into(), format!("{percent:.2}")),
            ];
            if let Some(size) = path.cluster_sizes.get(*step) {
                items.push(("CLUSTER_SIZE".into(), size.to_string()));
            }
            if let Some(start) = path.minimizer_energies_hartree.get(*step) {
                items.push(("START_ENERGY_HARTREE".into(), format!("{start:.10}")));
            }
            // The calculated state, which a typed charge can set apart from
            // the drawn formal charges.
            items.push(("TOTAL_CHARGE".into(), path.charge.to_string()));
            items.push(("MULTIPLICITY".into(), path.multiplicity.to_string()));
            (frame, format!("{title} conformer {rank}"), items)
        })
        .collect();

    let entries: Vec<_> = records
        .iter()
        .map(|(frame, title, properties)| SdfEntry {
            frame,
            title,
            bonds: &bonds,
            formal_charges: &formal_charges,
            properties,
        })
        .collect();
    write_sdf(&entries).map_err(|error| error.to_string())
}

struct RankedConformer {
    step: usize,
    frame: Frame,
    energy: f64,
    delta_kcal: f64,
    percent: f64,
}

fn ranked_trajectory(result: &CalcResult) -> Result<(&Trajectory, Vec<RankedConformer>), String> {
    let path = result
        .trajectory
        .as_ref()
        .filter(|path| !path.positions_angstrom.is_empty())
        .ok_or(NO_SAVED_POSITIONS)?;
    path.validate()
        .map_err(|error| format!("The saved conformers could not be exported ({error})."))?;
    let energies = &path.energies_hartree;
    let minimum = energies
        .iter()
        .copied()
        .min_by(f64::total_cmp)
        .ok_or("This result has no saved conformer energies.")?;
    let mut ranked = energies
        .iter()
        .copied()
        .zip(populations(energies))
        .enumerate()
        .map(|(step, (energy, fraction))| {
            Ok(RankedConformer {
                step,
                frame: path.frame(step).ok_or(NO_SAVED_POSITIONS)?,
                energy,
                delta_kcal: (energy - minimum) * KCAL_PER_HARTREE,
                percent: fraction * 100.0,
            })
        })
        .collect::<Result<Vec<_>, &str>>()?;
    ranked.sort_by(|a, b| a.energy.total_cmp(&b.energy));
    Ok((path, ranked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Point;
    use reshiki::qcdata::{Provenance, Settings, Source, Status, Task};

    const LOWEST: [[f64; 3]; 3] = [[-0.24, 0.93, 0.0], [0.0, 0.0, 0.0], [0.96, 0.0, 0.0]];
    const MIDDLE: [[f64; 3]; 3] = [[-0.24, 0.93, 0.0], [0.0, 0.0, 0.01], [0.95, 0.0, 0.0]];
    // Here the model hydrogen is closer to the drawn hydrogen than it is to oxygen.
    const HIGHEST: [[f64; 3]; 3] = [[-0.24, 0.93, 0.0], [0.0, 0.0, 0.02], [-0.48, 1.86, 0.0]];

    fn water() -> (Document, [Option<u64>; 3], CalcResult) {
        let mut doc = Document::default();
        let oxygen = doc.add_atom("O", Point::default());
        let hydrogen = doc.add_atom("H", Point::default());
        doc.atom_mut(oxygen).unwrap().charge = 1;
        doc.add_bond(oxygen, hydrogen, 1, "plain");
        let atoms = [Some(hydrogen), Some(oxygen), None];
        let path = Trajectory {
            numbers: vec![1, 8, 1],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 1,
            multiplicity: 2,
            positions_angstrom: [LOWEST, HIGHEST, MIDDLE]
                .into_iter()
                .flatten()
                .flatten()
                .collect(),
            energies_hartree: vec![-5.07, -5.068, -5.069],
            labels: vec![],
            minimizer_energies_hartree: vec![-5.12, -5.10, -5.11],
            cluster_sizes: vec![6, 2, 4],
        };
        let frame = path.frame(0).unwrap();
        let result = CalcResult {
            frame: frame.hash(),
            input_frame: None,
            task: Task::Conformers,
            endpoints: None,
            settings: Settings::Imported { keywords: vec![] },
            provenance: Provenance {
                program: "test".into(),
                program_version: None,
                method: None,
                basis: None,
                source: Source::Imported {
                    format: "test".into(),
                    file_name: "water".into(),
                    file_sha256: None,
                    job: 0,
                },
            },
            status: Status::Converged,
            energy_hartree: Some(path.energies_hartree[0]),
            energy_terms: Default::default(),
            gradient_hartree_per_bohr: vec![],
            hessian_hartree_per_bohr2: vec![],
            charges: vec![],
            dipole_e_bohr: None,
            bond_orders: vec![],
            orbitals: None,
            vibrations: None,
            thermo: None,
            trajectory: Some(path),
            keep_full: false,
            omitted_bytes: None,
            run: None,
        };
        result.validate(&frame).unwrap();
        (doc, atoms, result)
    }

    #[test]
    fn frame_bonds_keep_drawing_orders_and_use_nearest_linked_atoms() {
        let mut doc = Document::default();
        let carbon = doc.add_atom("C", Point::default());
        let nitrogen = doc.add_atom("N", Point::default());
        let outside = doc.add_atom("O", Point::default());
        doc.add_bond(carbon, nitrogen, 4, "plain");
        doc.add_bond(carbon, outside, 1, "plain");
        doc.bonds.push(doc.bonds[0].clone());

        let atoms = [None, Some(nitrogen), Some(carbon), None];
        let positions = [[0., 0., -1.], [0., 0., 0.], [1.4, 0., 0.], [2.4, 0., 0.]];
        assert_eq!(
            frame_bonds(&doc, &atoms, &positions),
            vec![(0, 1, 1), (1, 2, 4), (2, 3, 1)]
        );
    }

    #[test]
    fn populations_sum_to_one_and_follow_energies() {
        let weights = populations(&[-5.069, -5.068, -5.07]);
        assert_eq!(weights.len(), 3);
        assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(weights[2] > weights[0] && weights[0] > weights[1]);
        assert!(populations(&[]).is_empty());
    }

    #[test]
    fn xyz_exports_all_frames_in_energy_order() {
        let (_, _, result) = water();
        let text = xyz(&result, "Water").unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 15);
        let (frames, _) = lines.as_chunks::<5>();
        for (frame, (comment, oxygen_z)) in frames.iter().zip([
            (
                "Water conformer 1 · ΔE 0.00 kcal/mol · 68.2 % · cluster 6 · charge=1 mult=2 · E = -5.0700000000 Eh",
                "0.00000000",
            ),
            (
                "Water conformer 2 · ΔE 0.63 kcal/mol · 23.6 % · cluster 4 · charge=1 mult=2 · E = -5.0690000000 Eh",
                "0.01000000",
            ),
            (
                "Water conformer 3 · ΔE 1.26 kcal/mol · 8.2 % · cluster 2 · charge=1 mult=2 · E = -5.0680000000 Eh",
                "0.02000000",
            ),
        ]) {
            assert_eq!(frame[0], "3");
            assert_eq!(frame[1], comment);
            let oxygen: Vec<_> = frame[3].split_whitespace().collect();
            assert_eq!(oxygen, ["O", "0.00000000", "0.00000000", oxygen_z]);
        }
    }

    #[test]
    fn sdf_exports_ranked_records_with_charges_and_shared_bonds() {
        let (doc, atoms, result) = water();
        let path = result.trajectory.as_ref().unwrap();
        let text = sdf(&doc, &atoms, &result, "Water").unwrap();
        let records: Vec<_> = text.split_terminator("$$$$\n").collect();
        assert_eq!(records.len(), 3);
        let expected = [
            (
                "-5.0700000000",
                "0.000",
                "68.17",
                6,
                "-5.1200000000",
                "0.000000",
            ),
            (
                "-5.0690000000",
                "0.628",
                "23.64",
                4,
                "-5.1100000000",
                "0.010000",
            ),
            (
                "-5.0680000000",
                "1.255",
                "8.20",
                2,
                "-5.1000000000",
                "0.020000",
            ),
        ];
        for (index, (record, (energy, delta, percent, size, start, oxygen_z))) in
            records.into_iter().zip(expected).enumerate()
        {
            let rank = index + 1;
            assert!(record.starts_with(&format!("Water conformer {rank}\n")));
            assert!(record.contains("999 V3000\n"));
            assert!(record.contains("M  V30 COUNTS 3 2 0 0 0\n"));
            assert!(record.contains(&format!(
                "M  V30 2 O 0.000000 0.000000 {oxygen_z} 0 CHG=1\n"
            )));
            assert_eq!(record.matches("CHG=").count(), 1);
            assert!(
                record.contains(
                    "M  V30 BEGIN BOND\nM  V30 1 1 1 2\nM  V30 2 1 2 3\nM  V30 END BOND\n"
                )
            );
            let (_, properties) = record.split_once("M  END\n").unwrap();
            assert_eq!(
                properties,
                format!(
                    "> <RANK>\n{rank}\n\n> <ENERGY_HARTREE>\n{energy}\n\n\
                     > <DELTA_E_KCAL_MOL>\n{delta}\n\n> <BOLTZMANN_PERCENT>\n{percent}\n\n\
                     > <CLUSTER_SIZE>\n{size}\n\n> <START_ENERGY_HARTREE>\n{start}\n\n\
                     > <TOTAL_CHARGE>\n{}\n\n> <MULTIPLICITY>\n{}\n\n",
                    path.charge, path.multiplicity
                )
            );
        }
    }

    #[test]
    fn sdf_skips_unsaved_optional_items() {
        let (doc, atoms, mut result) = water();
        let path = result.trajectory.as_mut().unwrap();
        path.cluster_sizes.clear();
        path.minimizer_energies_hartree.clear();
        let text = sdf(&doc, &atoms, &result, "Water").unwrap();
        assert_eq!(text.matches("$$$$\n").count(), 3);
        assert!(!text.contains("> <CLUSTER_SIZE>"));
        assert!(!text.contains("> <START_ENERGY_HARTREE>"));
    }

    #[test]
    fn exports_require_saved_positions() {
        let (doc, atoms, mut result) = water();
        result
            .trajectory
            .as_mut()
            .unwrap()
            .positions_angstrom
            .clear();
        let message = "This result has no saved conformer positions.";
        assert_eq!(xyz(&result, "Water").unwrap_err(), message);
        assert_eq!(sdf(&doc, &atoms, &result, "Water").unwrap_err(), message);
    }
}
