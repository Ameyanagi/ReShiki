use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cosmolkit::{
    EmbedParams, ForceFieldMinimizeParams, MmffForceFieldParams, MolecularForceField, Molecule,
};
use reshiki_optimize::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, KCAL_PER_HARTREE, Level, Method, Options, Potential,
    Status, optimize,
};

const MOLECULES: [(&str, &str); 20] = [
    ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
    ("paracetamol", "CC(=O)Nc1ccc(O)cc1"),
    ("ibuprofen", "CC(C)Cc1ccc(C(C)C(=O)O)cc1"),
    ("naproxen", "COc1ccc2cc([C@@H](C)C(=O)O)ccc2c1"),
    ("caffeine", "Cn1c(=O)c2c(ncn2C)n(C)c1=O"),
    ("nicotine", "CN1CCC[C@H]1c1cccnc1"),
    ("lidocaine", "CCN(CC)CC(=O)Nc1c(C)cccc1C"),
    ("propranolol", "CC(C)NCC(O)COc1cccc2ccccc12"),
    ("diazepam", "CN1C(=O)CN=C(c2ccccc2)c2cc(Cl)ccc21"),
    ("fluoxetine", "CNCCC(Oc1ccc(C(F)(F)F)cc1)c1ccccc1"),
    ("sertraline", "CN[C@H]1CC[C@H](c2ccccc21)c2cc(Cl)c(Cl)cc2"),
    ("warfarin", "CC(=O)CC(c1ccccc1)c1c(O)c2ccccc2oc1=O"),
    ("omeprazole", "COc1ccc2[nH]c(S(=O)Cc3ncc(C)c(OC)c3C)nc2c1"),
    (
        "celecoxib",
        "Cc1ccc(-c2cc(C(F)(F)F)nn2-c2ccc(S(N)(=O)=O)cc2)cc1",
    ),
    (
        "sildenafil",
        "CCCc1nn(C)c2c(=O)[nH]c(-c3cc(S(=O)(=O)N4CCN(C)CC4)ccc3OCC)nc12",
    ),
    (
        "losartan",
        "CCCCc1nc(Cl)c(CO)n1Cc1ccc(-c2ccccc2-c2nn[nH]n2)cc1",
    ),
    (
        "ciprofloxacin",
        "O=C(O)c1cn(C2CC2)c2cc(N3CCNCC3)c(F)cc2c1=O",
    ),
    (
        "tamoxifen",
        r"CC/C(=C(\c1ccccc1)/c1ccc(OCCN(C)C)cc1)/c1ccccc1",
    ),
    (
        "testosterone",
        "C[C@]12CC[C@H]3[C@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@]34C",
    ),
    (
        "imatinib",
        "Cc1ccc(NC(=O)c2ccc(CN3CCN(C)CC3)cc2)cc1Nc1nccc(-c2cccnc2)n1",
    ),
];

const RUNS: [(&str, Method, Level); 4] = [
    ("ANC-RFO", Method::AncRfo, Level::Normal),
    ("L-BFGS", Method::Lbfgs, Level::Normal),
    ("ANC-RFO Tight", Method::AncRfo, Level::Tight),
    ("L-BFGS Tight", Method::Lbfgs, Level::Tight),
];

struct MmffPotential(MolecularForceField);

impl Potential for MmffPotential {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let coordinates: Vec<_> = positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|row| row.map(|x| x * ANGSTROM_PER_BOHR))
            .collect();
        let failed = |error: cosmolkit::ForceFieldError| EvalError::Failed(error.to_string());
        self.0.set_positions_(&coordinates).map_err(failed)?;
        let energy = self.0.energy().map_err(failed)?;
        let mmff_gradient = self.0.gradient_unconstrained().map_err(failed)?;
        for (output, &value) in gradient.iter_mut().zip(mmff_gradient.as_flattened()) {
            *output = value * ANGSTROM_PER_BOHR / KCAL_PER_HARTREE;
        }
        Ok(energy / KCAL_PER_HARTREE)
    }
}

struct Run {
    label: &'static str,
    status: Status,
    steps: usize,
    evaluations: usize,
    energy: f64,
    max_gradient: f64,
    wall_ms: f64,
    energies: Vec<f64>,
}

struct Benchmark {
    name: &'static str,
    atoms: usize,
    heavy_atoms: usize,
    runs: Vec<Run>,
    reference_iterations: u32,
    reference_energy: f64,
}

fn benchmark(name: &'static str, smiles: &str) -> Result<Benchmark, Box<dyn Error>> {
    let molecule = Molecule::from_smiles(smiles)?
        .with_hydrogens()?
        .sanitize()?;
    let numbers: Vec<_> = molecule.atoms().iter().map(|a| a.atomic_number()).collect();
    let heavy_atoms = numbers.iter().filter(|&&z| z > 1).count();
    let charge: i32 = molecule
        .atoms()
        .iter()
        .map(|a| i32::from(a.formal_charge()))
        .sum();
    if !(10..=70).contains(&heavy_atoms) || charge != 0 {
        return Err(format!(
            "expected 10–70 heavy atoms and zero charge, got {heavy_atoms} and {charge}"
        )
        .into());
    }

    let mut embedding = EmbedParams::etkdg_v3();
    embedding.random_seed = 42;
    embedding.num_threads = 1;
    let embedded = molecule.with_3d_conformer_result_with_params(&embedding)?;
    let [conformer] = embedded.molecule.conformers_3d() else {
        return Err("ETKDG did not generate one conformer".into());
    };
    let positions: Vec<_> = conformer
        .coordinates()
        .as_flattened()
        .iter()
        .map(|x| x / ANGSTROM_PER_BOHR)
        .collect();
    let field_params = MmffForceFieldParams::new(None, "MMFF94".into(), f64::INFINITY, true);
    let new_field = || {
        embedded
            .molecule
            .mmff_force_field_with_params(&field_params)
    };

    let mut runs = Vec::with_capacity(RUNS.len());
    for (label, method, level) in RUNS {
        let mut potential = MmffPotential(new_field()?);
        let options = Options {
            method,
            level,
            ..Options::default()
        };
        let start = Instant::now();
        let outcome = optimize(&mut potential, &numbers, &positions, &options, |_| {
            ControlFlow::Continue(())
        });
        let wall_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let max_gradient = outcome
            .gradient
            .iter()
            .map(|x| x.abs())
            .reduce(f64::max)
            .unwrap_or(f64::NAN)
            * KCAL_PER_HARTREE
            / ANGSTROM_PER_BOHR;
        if outcome.status != Status::Converged {
            eprintln!("{name} {label}: {:?}", outcome.status);
        }
        runs.push(Run {
            label,
            status: outcome.status,
            steps: outcome.steps,
            evaluations: outcome.evaluations,
            energy: outcome.energy * KCAL_PER_HARTREE,
            max_gradient,
            wall_ms,
            energies: outcome
                .energies
                .into_iter()
                .map(|e| e * KCAL_PER_HARTREE)
                .collect(),
        });
    }

    let defaults = ForceFieldMinimizeParams::default();
    let reference = new_field()?.minimize_with_params_(&ForceFieldMinimizeParams::new(
        10_000,
        defaults.force_tolerance(),
        defaults.energy_tolerance(),
    ))?;
    Ok(Benchmark {
        name,
        atoms: numbers.len(),
        heavy_atoms,
        runs,
        reference_iterations: reference.iterations(),
        reference_energy: reference.energy(),
    })
}

fn csv_number(value: f64) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        String::new()
    }
}

fn write_csv(path: &Path, benchmarks: &[Benchmark]) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(
        out,
        "molecule,atoms,heavy_atoms,method,status,steps,evaluations,iterations,energy_kcal_mol,max_gradient_kcal_mol_angstrom,wall_ms"
    )?;
    for bench in benchmarks {
        for run in &bench.runs {
            let status = format!("{:?}", run.status).replace('"', "\"\"");
            writeln!(
                out,
                "{},{},{},{},\"{}\",{},{},,{},{},{:.3}",
                bench.name,
                bench.atoms,
                bench.heavy_atoms,
                run.label,
                status,
                run.steps,
                run.evaluations,
                csv_number(run.energy),
                csv_number(run.max_gradient),
                run.wall_ms,
            )?;
        }
        writeln!(
            out,
            "{},{},{},COSMolKit,,,,{},{},,",
            bench.name,
            bench.atoms,
            bench.heavy_atoms,
            bench.reference_iterations,
            csv_number(bench.reference_energy),
        )?;
    }
    out.flush()
}

fn write_traces(path: &Path, benchmarks: &[Benchmark]) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    write!(out, "{{")?;
    for (i, bench) in benchmarks.iter().enumerate() {
        // Names and run labels are fixed strings containing no JSON escapes.
        let separator = if i == 0 { "" } else { "," };
        write!(out, "{separator}\n  \"{}\": {{", bench.name)?;
        for (j, run) in bench.runs.iter().enumerate() {
            let separator = if j == 0 { "" } else { "," };
            write!(
                out,
                "{separator}\n    \"{}\": {:?}",
                run.label, run.energies
            )?;
        }
        write!(out, "\n  }}")?;
    }
    writeln!(out, "\n}}")?;
    out.flush()
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values: Vec<_> = values.filter(|x| x.is_finite()).collect();
    values.sort_unstable_by(f64::total_cmp);
    if values.is_empty() {
        return f64::NAN;
    }
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        values[middle - 1].midpoint(values[middle])
    } else {
        values[middle]
    }
}

fn markdown(benchmarks: &[Benchmark]) -> String {
    let mut table = String::from(concat!(
        "| Molecule | Atoms | ANC steps/evals | L-BFGS steps/evals | ANC Tight evals | L-BFGS Tight evals | COSMolKit iterations | ΔE ANC−LBFGS (kcal/mol) | ΔE ANC−COSMolKit (kcal/mol) | L-BFGS evals / ANC evals |\n",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    ));
    let mut total_steps = [0; 4];
    let mut total_evals = [0; 4];
    let mut total_atoms = 0;
    let mut total_iterations = 0;
    let mut deltas_lbfgs = Vec::with_capacity(benchmarks.len());
    let mut deltas_reference = Vec::with_capacity(benchmarks.len());
    for bench in benchmarks {
        let anc = &bench.runs[0];
        let lbfgs = &bench.runs[1];
        let delta_lbfgs = anc.energy - lbfgs.energy;
        let delta_reference = anc.energy - bench.reference_energy;
        deltas_lbfgs.push(delta_lbfgs);
        deltas_reference.push(delta_reference);
        writeln!(
            table,
            "| {} | {} | {}/{} | {}/{} | {} | {} | {} | {:+.6} | {:+.6} | {:.3} |",
            bench.name,
            bench.atoms,
            anc.steps,
            anc.evaluations,
            lbfgs.steps,
            lbfgs.evaluations,
            bench.runs[2].evaluations,
            bench.runs[3].evaluations,
            bench.reference_iterations,
            delta_lbfgs,
            delta_reference,
            lbfgs.evaluations as f64 / anc.evaluations as f64,
        )
        .unwrap();
        total_atoms += bench.atoms;
        total_iterations += bench.reference_iterations;
        for (i, run) in bench.runs.iter().enumerate() {
            total_steps[i] += run.steps;
            total_evals[i] += run.evaluations;
        }
    }
    writeln!(
        table,
        "| **Totals / median ΔE** | {} | {}/{} | {}/{} | {} | {} | {} | {:+.6} | {:+.6} | {:.3} |",
        total_atoms,
        total_steps[0],
        total_evals[0],
        total_steps[1],
        total_evals[1],
        total_evals[2],
        total_evals[3],
        total_iterations,
        median(deltas_lbfgs.into_iter()),
        median(deltas_reference.into_iter()),
        total_evals[1] as f64 / total_evals[0] as f64,
    )
    .unwrap();
    table
}

fn output_dir() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next(), args.next()) {
        (None, None, None) => Ok("target/optimize-bench".into()),
        (Some(flag), Some(path), None) if flag == "--out" => Ok(path.into()),
        _ => Err("usage: mmff_benchmark [--out <dir>]".into()),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = output_dir()?;
    let mut benchmarks = Vec::with_capacity(MOLECULES.len());
    for (name, smiles) in MOLECULES {
        let bench = benchmark(name, smiles).map_err(|error| format!("{name}: {error}"))?;
        benchmarks.push(bench);
    }
    let table = markdown(&benchmarks);
    fs::create_dir_all(&out)?;
    write_csv(&out.join("results.csv"), &benchmarks)?;
    write_traces(&out.join("traces.json"), &benchmarks)?;
    fs::write(out.join("results.md"), &table)?;
    print!("{table}");
    Ok(())
}
