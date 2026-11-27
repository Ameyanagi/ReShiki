//! Conformer ensembles as files: ranked tables, multi-frame XYZ and SDF with energies.

use super::Format;
use crate::app::App;
use crate::app::figure_export::{Kind, Prepared};
use crate::app::xtb::{EnergyUnit, palette::Shown, state};
use reshiki::document::Document;
use reshiki::qc_input::{SdfEntry, write_sdf};
use reshiki::qcdata::CalcResult;
pub(crate) use reshiki::science::frame_bonds;
#[cfg(test)]
use reshiki_xtb_jobs::conformers::populations;
use reshiki_xtb_jobs::conformers::{self, RankedConformer, ranked_trajectory};

impl App {
    pub(super) fn xtb_prepare_conformer_export(
        &self,
        shown: &Shown<'_>,
        format: Format,
    ) -> Result<Prepared, String> {
        let result = &shown.stored.result;
        let atoms: Vec<_> = shown.atoms.iter().flatten().copied().collect();
        let (title, stem) = reshiki::molecule_names::label(&self.tab.doc, &atoms)
            .map(|caption| caption.text.trim().to_owned())
            .and_then(export_name)
            .or_else(|| self.export_stem().and_then(export_name))
            .or_else(|| export_name(state::formula(&shown.frame.numbers)))
            .unwrap_or_else(|| ("Molecule".into(), "Molecule".into()));
        let unit = self.appearance.energy_unit;
        let (text, extension) = match format {
            Format::Sdf => (
                sdf(&self.tab.doc, shown.atoms, result, &title, unit)?,
                "sdf",
            ),
            Format::Xyz => (xyz(result, &title, unit)?, "xyz"),
            Format::Tsv => (table(result, unit, '\t')?, "tsv"),
            Format::Csv => (table(result, unit, ',')?, "csv"),
        };
        let count = result.trajectory.as_ref().map_or(0, |path| path.steps());
        let detail = format!("{count} conformer{}", if count == 1 { "" } else { "s" });
        Ok(Prepared::new(
            text.into_bytes(),
            extension,
            Some(format!("{stem}-conformers")),
            vec![detail],
            Kind::Xtb,
        ))
    }
}

fn export_name(title: String) -> Option<(String, String)> {
    let stem = title
        .chars()
        .filter(|c| (!c.is_control() || c.is_whitespace()) && !r#"<>:"/\|?*"#.contains(*c))
        .collect::<String>();
    let stem = stem.split_whitespace().collect::<Vec<_>>().join("-");
    (!stem.is_empty()).then_some((title, stem))
}

/// Export every saved conformer as XYZ, lowest energy first.
pub(crate) fn xyz(result: &CalcResult, title: &str, unit: EnergyUnit) -> Result<String, String> {
    conformers::xyz_with_relative_energy(result.trajectory.as_ref(), title, |delta| {
        format!("{:.2} {}", unit.convert_kcal(delta), unit.label())
    })
}

/// Ranked saved conformers as TSV or CSV; every column has a numeric value or is empty.
fn table(result: &CalcResult, unit: EnergyUnit, separator: char) -> Result<String, String> {
    let (path, ranked) = ranked_trajectory(result.trajectory.as_ref())?;
    let separator = separator.to_string();
    let mut text = [
        "Rank".into(),
        "Energy (Eh)".into(),
        format!("ΔE ({})", unit.label()),
        format!("ΔE start ({})", unit.label()),
        "Boltzmann (%)".into(),
        "RMSD to first saved (Å)".into(),
        "Cluster size".into(),
        "Start energy (Eh)".into(),
    ]
    .join(&separator);
    text.push('\n');
    for (index, conformer) in ranked.iter().enumerate() {
        let statistics = &conformer.statistics;
        let values = [
            (index + 1).to_string(),
            format!("{:.10}", conformer.energy),
            format!("{:.3}", unit.convert_kcal(statistics.delta_kcal)),
            statistics.start_kcal.map_or_else(String::new, |delta| {
                format!("{:.3}", unit.convert_kcal(delta))
            }),
            format!("{:.2}", statistics.percent),
            statistics
                .rmsd
                .map_or_else(String::new, |rmsd| format!("{rmsd:.3}")),
            statistics
                .cluster
                .map_or_else(String::new, |size| size.to_string()),
            path.minimizer_energies_hartree
                .get(conformer.step)
                .map_or_else(String::new, |energy| format!("{energy:.10}")),
        ];
        text.push_str(&values.join(&separator));
        text.push('\n');
    }
    Ok(text)
}

/// Export every saved conformer as a V3000 SDF record with drawing bonds and charges.
pub(crate) fn sdf(
    doc: &Document,
    atoms: &[Option<u64>],
    result: &CalcResult,
    title: &str,
    unit: EnergyUnit,
) -> Result<String, String> {
    let (path, ranked) = ranked_trajectory(result.trajectory.as_ref())?;
    let first = ranked
        .first()
        .ok_or("This result has no saved conformer positions.")?;
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
                statistics,
                frame,
            } = conformer;
            let (delta_kcal, percent) = (statistics.delta_kcal, statistics.percent);
            let rank = index + 1;
            let mut items = vec![
                ("RANK".into(), rank.to_string()),
                ("ENERGY_HARTREE".into(), format!("{energy:.10}")),
                (
                    match unit {
                        EnergyUnit::KcalPerMol => "DELTA_E_KCAL_MOL",
                        EnergyUnit::KjPerMol => "DELTA_E_KJ_MOL",
                        EnergyUnit::Ev => "DELTA_E_EV",
                    }
                    .into(),
                    format!("{:.3}", unit.convert_kcal(delta_kcal)),
                ),
                ("BOLTZMANN_PERCENT".into(), format!("{percent:.2}")),
            ];
            if let Some(size) = statistics.cluster {
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

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Point;
    use reshiki::qcdata::{Provenance, Settings, Source, Status, Task, Trajectory};

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
            rmsd_to_best_angstrom: vec![],
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
        let text = xyz(&result, "Water", EnergyUnit::default()).unwrap();
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
        let text = sdf(&doc, &atoms, &result, "Water", EnergyUnit::default()).unwrap();
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
        let text = sdf(&doc, &atoms, &result, "Water", EnergyUnit::default()).unwrap();
        assert_eq!(text.matches("$$$$\n").count(), 3);
        assert!(!text.contains("> <CLUSTER_SIZE>"));
        assert!(!text.contains("> <START_ENERGY_HARTREE>"));
    }

    #[test]
    fn tables_rank_saved_conformers_and_leave_missing_statistics_empty() {
        let (_, _, mut result) = water();
        let text = table(&result, EnergyUnit::KjPerMol, '\t').unwrap();
        let second: Vec<_> = text.lines().nth(2).unwrap().split('\t').collect();
        assert_eq!(
            second,
            [
                "2",
                "-5.0690000000",
                "2.625",
                "26.255",
                "23.64",
                "",
                "4",
                "-5.1100000000"
            ]
        );
        let path = result.trajectory.as_mut().unwrap();
        path.minimizer_energies_hartree.clear();
        path.cluster_sizes.clear();
        let text = table(&result, EnergyUnit::KjPerMol, ',').unwrap();
        let second: Vec<_> = text.lines().nth(2).unwrap().split(',').collect();
        assert_eq!(
            second,
            ["2", "-5.0690000000", "2.625", "", "23.64", "", "", ""]
        );
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
        assert_eq!(
            xyz(&result, "Water", EnergyUnit::default()).unwrap_err(),
            message
        );
        assert_eq!(
            sdf(&doc, &atoms, &result, "Water", EnergyUnit::default()).unwrap_err(),
            message
        );
        assert_eq!(
            table(&result, EnergyUnit::KjPerMol, '\t').unwrap_err(),
            message
        );
    }
}
