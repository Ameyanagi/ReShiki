//! 3D generation through the desktop app's worker path.
//! This example is its own `--geometry-worker`, with the app's bounded heap and
//! default machine budget. It prepares like the 3D model view and the
//! Optimization panel: strict, falling back to relaxed mode when strict
//! preparation fails.
//! Usage: large_geometry_qa CASES.tsv OUTPUT_DIRECTORY [NAME]
//! where each CASES.tsv line is `name<TAB>SMILES`. Each result line ends with
//! every element pair's bond lengths and the angles at the most connected atom.
use reshiki_io::{
    engine::{LocalEngine, Request},
    geometry::{Client, ForceField, Limits, Prepared},
};
use reshiki_model::{document::Document, models3d::Display};
use std::{path::PathBuf, time::Instant};

#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // SMILES import also runs the InChI kernel in a worker of this executable.
    match args.first().map(String::as_str) {
        Some("--geometry-worker") => reshiki_io::geometry::worker::run(),
        Some("--inchi-worker") => reshiki_io::chemistry::inchi::worker::run(),
        _ => return qa(&args),
    }
    Ok(())
}

fn qa(args: &[String]) -> anyhow::Result<()> {
    let [cases, output, rest @ ..] = args else {
        anyhow::bail!("Usage: large_geometry_qa CASES.tsv OUTPUT_DIRECTORY [NAME]");
    };
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let cases = std::fs::read_to_string(cases)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let limits = Limits::default();
        // GEOMETRY_QA_WORKER names a wrapper, such as one that tees the
        // worker's input to capture a request fixture, and execs this example.
        let worker = std::env::var_os("GEOMETRY_QA_WORKER").map(PathBuf::from);
        let client = Client::new(worker.map_or_else(std::env::current_exe, Ok)?, limits)?;
        eprintln!(
            "worker timeout {} s, heap {} MiB",
            limits.timeout.as_secs(),
            limits.heap_bytes >> 20
        );
        let mut failed = 0;
        let mut ran = 0;
        for line in cases.lines().filter(|line| !line.trim().is_empty()) {
            let (name, smiles) = line.split_once('\t').unwrap_or((line, ""));
            if rest.first().is_some_and(|only| only != name) {
                continue;
            }
            ran += 1;
            // Names become output file names: keep them inside the directory.
            anyhow::ensure!(
                std::path::Path::new(name).file_name() == Some(name.as_ref()),
                "case name {name:?} must be a plain file name"
            );
            let _ = std::fs::remove_file(output.join(format!("{name}.rsk")));
            let row = run(&client, &output, name, smiles)
                .await
                .unwrap_or_else(|error| format!("error\t{error}"));
            failed += usize::from(!row.contains("\tok\t"));
            println!("{name}\t{row}");
        }
        anyhow::ensure!(ran > 0 && failed == 0, "{failed} of {ran} cases failed");
        Ok(())
    })
}

async fn run(
    client: &Client,
    output: &std::path::Path,
    name: &str,
    smiles: &str,
) -> anyhow::Result<String> {
    let mut doc: Document = LocalEngine::default()
        .request(Request::import_smiles(smiles))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .ok_or_else(|| anyhow::anyhow!("no drawing"))?;
    let atoms: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    let (prepared, mode) = match Prepared::new(&doc, &atoms) {
        Ok(prepared) => (prepared, "strict".to_owned()),
        Err(error) => (
            Prepared::relaxed(&doc, &atoms)?,
            format!("relaxed (strict: {error})"),
        ),
    };
    let prepared = prepared.without_retained_depth();
    // GEOMETRY_QA_FIELD=MMFF94s requests MMFF94s as the 3D model view does;
    // otherwise metals get UFF as in the Optimization panel.
    let field = if std::env::var("GEOMETRY_QA_FIELD").is_ok_and(|f| f == "MMFF94s")
        || prepared.mmff_available()
    {
        ForceField::Mmff94s
    } else {
        ForceField::Uff
    };
    let started = Instant::now();
    let result = client.generate(&prepared, field).await;
    let seconds = started.elapsed().as_secs_f64();
    let optimized = match result {
        Ok(optimized) => optimized,
        Err(error) => return Ok(format!("{mode}\tfailed\t{seconds:.1}\t{error}")),
    };
    // Drawn bond lengths and the closest non-bonded drawn pair: a sanity view.
    let position = |id| {
        let p = optimized.conformer.positions[prepared.index(id).expect("prepared atom")];
        [p.x, p.y, p.z]
    };
    let lengths: Vec<f64> = doc
        .bonds
        .iter()
        .map(|bond| distance(position(bond.a), position(bond.b)))
        .collect();
    let bonded: std::collections::HashSet<_> = doc
        .bonds
        .iter()
        .flat_map(|bond| [(bond.a, bond.b), (bond.b, bond.a)])
        .collect();
    let mut closest = f64::INFINITY;
    for (i, a) in atoms.iter().enumerate() {
        for b in &atoms[i + 1..] {
            if !bonded.contains(&(*a, *b)) {
                closest = closest.min(distance(position(*a), position(*b)));
            }
        }
    }
    // The method line of a generated ensemble, else how the start was made.
    let method = optimized
        .diagnostics
        .iter()
        .find(|line| line.contains("selection="))
        .or_else(|| {
            optimized
                .diagnostics
                .iter()
                .find(|line| line.starts_with("initialization="))
        })
        .and_then(|line| line.split(';').next())
        .unwrap_or("");
    // Small molecules: each element pair's bond lengths and the angle range
    // at the most connected drawn atom, for comparison with reference data.
    let mut pairs = std::collections::BTreeMap::<String, Vec<f64>>::new();
    for (bond, length) in doc.bonds.iter().zip(&lengths) {
        let mut names = [bond.a, bond.b].map(|id| doc.atom(id).map_or("?", |a| &a.element));
        names.sort();
        pairs.entry(names.join("-")).or_default().push(*length);
    }
    let pairs: Vec<_> = pairs
        .iter()
        .map(|(pair, d)| {
            let (lo, hi) = d
                .iter()
                .fold((f64::INFINITY, 0f64), |(l, h), x| (l.min(*x), h.max(*x)));
            format!("{pair} {lo:.3}-{hi:.3}")
        })
        .collect();
    let centre = atoms.iter().copied().max_by_key(|id| {
        doc.bonds
            .iter()
            .filter(|b| b.a == *id || b.b == *id)
            .count()
    });
    let mut angles = vec![];
    if let Some(centre) = centre {
        let ligands: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                (b.a == centre)
                    .then_some(b.b)
                    .or((b.b == centre).then_some(b.a))
            })
            .collect();
        for (i, a) in ligands.iter().enumerate() {
            for b in &ligands[i + 1..] {
                angles.push(angle(position(*a), position(centre), position(*b)));
            }
        }
    }
    let angles = if angles.is_empty() {
        String::new()
    } else {
        format!(
            "; angles at {} {:.1}-{:.1} deg",
            centre
                .and_then(|id| doc.atom(id))
                .map_or("?", |a| &a.element),
            angles.iter().copied().fold(f64::INFINITY, f64::min),
            angles.iter().copied().fold(0., f64::max)
        )
    };
    let geometry = format!("{}{angles}", pairs.join(", "));
    doc.models.push(prepared.model(&optimized)?);
    reshiki_model::models3d::require_version(&mut doc);
    reshiki_model::models3d::set_display(&mut doc, 0, Display::ThreeD);
    std::fs::write(
        output.join(format!("{name}.rsk")),
        doc.file_json().map_err(anyhow::Error::msg)?,
    )?;
    Ok(format!(
        "{mode}\tok\t{seconds:.1}\t{} atoms; {}; converged={}; E={:.1}; drawn bonds {:.3}-{:.3} A; closest non-bonded drawn pair {closest:.2} A; {method}\t{geometry}",
        optimized.conformer.positions.len(),
        optimized.force_field.name(),
        optimized.converged,
        optimized.energy,
        lengths.iter().copied().fold(f64::INFINITY, f64::min),
        lengths.iter().copied().fold(0., f64::max),
    ))
}

fn angle(a: [f64; 3], centre: [f64; 3], b: [f64; 3]) -> f64 {
    let u: [f64; 3] = std::array::from_fn(|i| a[i] - centre[i]);
    let v: [f64; 3] = std::array::from_fn(|i| b[i] - centre[i]);
    let dot: f64 = (0..3).map(|i| u[i] * v[i]).sum();
    (dot / (distance(a, centre) * distance(b, centre)))
        .clamp(-1., 1.)
        .acos()
        .to_degrees()
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}
