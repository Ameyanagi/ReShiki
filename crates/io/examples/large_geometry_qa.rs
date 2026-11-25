//! 3D generation for large fragments through the desktop app's worker path.
//! This example is its own `--geometry-worker`, with the app's bounded heap and
//! default machine budget. It prepares like the 3D model (strict), falling back
//! to relaxed mode like the Optimization panel when strict preparation fails.
//! Usage: large_geometry_qa CASES.tsv OUTPUT_DIRECTORY [NAME]
//! where each CASES.tsv line is `name<TAB>SMILES`.
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
        let client = Client::new(std::env::current_exe()?, limits)?;
        eprintln!(
            "worker timeout {} s, heap {} MiB",
            limits.timeout.as_secs(),
            limits.heap_bytes >> 20
        );
        for line in cases.lines().filter(|line| !line.trim().is_empty()) {
            let (name, smiles) = line.split_once('\t').unwrap_or((line, ""));
            if rest.first().is_some_and(|only| only != name) {
                continue;
            }
            let row = run(&client, &output, name, smiles).await;
            println!(
                "{name}\t{}",
                row.unwrap_or_else(|error| format!("error\t{error}"))
            );
        }
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
    let field = if prepared.mmff_available() {
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
    let method = optimized
        .diagnostics
        .iter()
        .find(|line| line.contains("selection="))
        .and_then(|line| line.split(';').next())
        .unwrap_or("");
    doc.models.push(prepared.model(&optimized)?);
    reshiki_model::models3d::require_version(&mut doc);
    reshiki_model::models3d::set_display(&mut doc, 0, Display::ThreeD);
    std::fs::write(
        output.join(format!("{name}.rsk")),
        doc.file_json().map_err(anyhow::Error::msg)?,
    )?;
    Ok(format!(
        "{mode}\tok\t{seconds:.1}\t{} atoms; {}; converged={}; E={:.1}; drawn bonds {:.3}-{:.3} A; closest non-bonded drawn pair {closest:.2} A; {method}",
        optimized.conformer.positions.len(),
        optimized.force_field.name(),
        optimized.converged,
        optimized.energy,
        lengths.iter().copied().fold(f64::INFINITY, f64::min),
        lengths.iter().copied().fold(0., f64::max),
    ))
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}
