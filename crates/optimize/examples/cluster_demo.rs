use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use cosmolkit::{EmbedParams, ForceFieldMinimizeParams, MmffForceFieldParams, Molecule};
use reshiki_optimize::{
    KCAL_PER_HARTREE,
    conformers::{self, Candidate, Diversity, PerCluster, Settings, Start, Topology},
};

const MOLECULES: [(&str, &str); 3] = [
    ("butane", "CCCC"),
    ("ibuprofen", "CC(C)Cc1ccc(C(C)C(=O)O)cc1"),
    ("diphenhydramine", "CN(C)CCOC(c1ccccc1)c1ccccc1"),
];
const BATCHES: [u32; 4] = [32, 32, 32, 4];
const BASE_SEED: i32 = 0x52534b;

fn minimized_starts(molecule: &Molecule) -> Result<(Vec<Start>, usize), Box<dyn Error>> {
    let mut embedding = EmbedParams::etkdg_v3();
    embedding.num_threads = 1;
    embedding.max_iterations = 100;
    embedding.enable_sequential_random_seeds = true;
    // Keep all 100 embeddings so the clustering code handles duplicate removal.
    embedding.prune_rms_thresh = -1.0;

    let minimization = ForceFieldMinimizeParams::new(2_000, 1e-4, 1e-6);
    let mut starts = Vec::with_capacity(BATCHES.iter().sum::<u32>() as usize);
    let mut converged = 0;
    for (batch, count) in BATCHES.into_iter().enumerate() {
        embedding.random_seed = BASE_SEED + batch as i32 * 7919;
        let embedded = molecule.with_3d_conformers_result_with_params(count, &embedding)?;
        let conformers = embedded.molecule.conformers_3d();
        if conformers.len() != count as usize {
            return Err(format!(
                "ETKDG batch {} generated {} of {count} conformers (seed {})",
                batch + 1,
                conformers.len(),
                embedding.random_seed
            )
            .into());
        }
        for conformer in conformers {
            // Select this batch member and keep all intramolecular nonbonded terms.
            let params = MmffForceFieldParams::new(
                Some(conformer.id()),
                "MMFF94s".into(),
                f64::INFINITY,
                true,
            );
            let mut field = embedded.molecule.mmff_force_field_with_params(&params)?;
            let outcome = field.minimize_with_params_(&minimization)?;
            converged += usize::from(outcome.converged());
            starts.push(Start {
                positions: field.positions(),
                energy_hartree: outcome.energy() / KCAL_PER_HARTREE,
            });
        }
    }
    Ok((starts, converged))
}

fn write_molecule(report: &mut String, name: &str, smiles: &str) -> Result<(), Box<dyn Error>> {
    let molecule = Molecule::from_smiles(smiles)?
        .with_hydrogens()?
        .sanitize()?;
    let numbers = molecule.atoms().iter().map(|a| a.atomic_number()).collect();
    let bonds = molecule
        .bonds()
        .iter()
        .map(|b| [b.begin().index(), b.end().index()])
        .collect();
    let topology = Topology::new(numbers, bonds)?;
    let defaults = Settings::default();
    let autos = topology.automorphisms(defaults.automorphism_cap);
    let (starts, converged) = minimized_starts(&molecule)?;
    let minimum = starts
        .iter()
        .map(|start| start.energy_hartree)
        .fold(f64::INFINITY, f64::min);
    writeln!(report, "\n{name} ({smiles})")?;
    writeln!(
        report,
        "  {} atoms, {} heavy atoms, {} automorphisms (truncated: {})",
        molecule.num_atoms(),
        topology.heavy_atoms().len(),
        autos.perms().len(),
        autos.truncated(),
    )?;
    writeln!(report, "  MMFF94s converged: {converged}/{}", starts.len())?;

    let mut candidates = Vec::new();
    for (diversity, per_cluster) in [
        (Diversity::Fine, PerCluster::One),
        (Diversity::Normal, PerCluster::One),
        (Diversity::Coarse, PerCluster::One),
        (Diversity::Normal, PerCluster::Three),
    ] {
        let settings = Settings {
            diversity,
            per_cluster,
            ..defaults
        };
        let before = Instant::now();
        let clustered = conformers::cluster(&topology, &starts, &settings)?;
        let elapsed_ms = before.elapsed().as_secs_f64() * 1_000.0;
        let mut sizes: Vec<_> = clustered.clusters.iter().map(Vec::len).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        let cluster_label = if sizes.len() == 1 {
            "cluster"
        } else {
            "clusters"
        };
        writeln!(
            report,
            "  {diversity:?}/{per_cluster:?} ({:.2} Å)  {} {cluster_label}, {} starts kept, {:.3} ms",
            diversity.threshold_angstrom(),
            sizes.len(),
            clustered.kept.len(),
            elapsed_ms,
        )?;
        writeln!(report, "    Cluster sizes: {sizes:?}")?;
        writeln!(
            report,
            "    Representatives (start index, ΔE vs global min in kcal/mol)"
        )?;
        for row in clustered.representatives.chunks(6) {
            write!(report, "   ")?;
            for &index in row {
                let delta_kcal = (starts[index].energy_hartree - minimum) * KCAL_PER_HARTREE;
                write!(report, " ({index:>2}, {delta_kcal:>6.3})")?;
            }
            writeln!(report)?;
        }
        if (diversity, per_cluster) == (Diversity::Normal, PerCluster::One) {
            for &index in &clustered.representatives {
                let members = clustered
                    .clusters
                    .iter()
                    .find(|members| members[0] == index)
                    .expect("each Normal/One representative is the first member of its cluster");
                let start = &starts[index];
                candidates.push(Candidate {
                    positions: start.positions.clone(),
                    energy_hartree: start.energy_hartree,
                    start_energy_hartree: start.energy_hartree,
                    cluster_size: members.len() as u32,
                });
            }
        }
    }

    let ranked = conformers::rank(&topology, candidates, &defaults)?;
    writeln!(
        report,
        "  Normal/One re-ranked with MMFF94s at {:.2} K",
        defaults.temperature_k
    )?;
    writeln!(
        report,
        "    Rank  ΔE kcal/mol  Boltzmann %  RMSD to best Å  Cluster size"
    )?;
    for (index, entry) in ranked.iter().enumerate() {
        writeln!(
            report,
            "    {:>4}  {:>11.3}  {:>11.2}  {:>14.3}  {:>12}",
            index + 1,
            entry.relative_kcal,
            entry.boltzmann * 100.0,
            entry.rmsd_to_best,
            entry.cluster_size,
        )?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let output = match (args.next(), args.next(), args.next()) {
        (None, None, None) => None,
        (Some(flag), Some(path), None) if flag == "--out" => Some(PathBuf::from(path)),
        _ => return Err("usage: cluster_demo [--out <dir>]".into()),
    };

    let mut report = format!(
        "ETKDGv3 batches {BATCHES:?}, seeds 0x{BASE_SEED:x} + k*7919, no embedding RMSD pruning\n\
         MMFF94s minimization (2000 iterations max), zero-based start indices\n"
    );
    for (name, smiles) in MOLECULES {
        write_molecule(&mut report, name, smiles).map_err(|error| format!("{name}: {error}"))?;
    }
    print!("{report}");
    if let Some(dir) = output {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("cluster-demo.txt"), report)?;
    }
    Ok(())
}
