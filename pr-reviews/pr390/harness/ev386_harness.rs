//! Evidence harness for #386 phase 1a (not part of the repo).
//! Usage: ev386_harness WORKER_EXE OUT_DIR
//! Imports caffeine exactly like the app's Insert (Request::import_smiles
//! through LocalEngine), then generates a model exactly like the Structure
//! section (Prepared::new(..).without_retained_depth(), Client::generate with
//! MMFF94s, Prepared::model, require_version(MODELS_3D), set_display), and
//! writes caffeine-2d.rsk, caffeine-3d.rsk, caffeine-both.rsk and
//! caffeine-main.rsk (the 2D drawing marked version 27 so origin/main reads it).
use reshiki::engine::{ChemistryEngine, LocalEngine, Request};
use reshiki::geometry::{Client, ForceField, Limits, Prepared};
use reshiki::models3d::{self, Display};
use std::path::PathBuf;

const SMILES: &str = "CN1C=NC2=C1C(=O)N(C(=O)N2C)C";

fn main() -> anyhow::Result<()> {
    // The engine relaunches this executable as its InChI helper, as the app does.
    match std::env::args_os().nth(1).as_deref().and_then(reshiki::workers::run) {
        Some(0) => return Ok(()),
        Some(code) => std::process::exit(code),
        None => {}
    }
    let mut args = std::env::args_os().skip(1);
    let worker = PathBuf::from(args.next().expect("worker exe"));
    let out = PathBuf::from(args.next().expect("out dir"));
    let rt = tokio::runtime::Runtime::new()?;
    let response = rt
        .block_on(LocalEngine::default().execute(Request::import_smiles(SMILES)))
        .map_err(anyhow::Error::msg)?;
    let doc = response.document.expect("document");
    doc.validate().map_err(anyhow::Error::msg)?;
    let atoms: Vec<u64> = doc.atoms.iter().map(|a| a.id).collect();
    eprintln!("imported {} atoms, {} bonds, version {}", atoms.len(), doc.bonds.len(), doc.version);

    let write = |name: &str, d: &reshiki::document::Document| -> anyhow::Result<()> {
        std::fs::write(out.join(name), serde_json::to_vec_pretty(d)?)?;
        // Round-trip through the loader, as opening the file would.
        reshiki::document::Document::from_json(&std::fs::read(out.join(name))?)
            .map_err(anyhow::Error::msg)?;
        Ok(())
    };

    let mut main_doc = doc.clone();
    main_doc.version = 27;
    write("caffeine-main.rsk", &main_doc)?;

    let selected = models3d::selected_molecule(&doc, &atoms).expect("one molecule");
    let prepared = Prepared::new(&doc, &selected)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .without_retained_depth();
    let client = Client::new(worker, Limits::default()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let optimized = rt
        .block_on(client.generate(&prepared, ForceField::Mmff94s))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!(
        "MMFF94s energy {:.3} -> {:.3}, converged {}",
        optimized.initial_energy, optimized.energy, optimized.converged
    );
    let model = prepared.model(&optimized).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("model: {} atoms, {} hydrogens", model.atoms.len(), model.hydrogens.len());

    for (name, display) in [
        ("caffeine-2d.rsk", Display::TwoD),
        ("caffeine-3d.rsk", Display::ThreeD),
        ("caffeine-both.rsk", Display::Both),
    ] {
        let mut d = doc.clone();
        d.models.push(model.clone());
        d.require_version(reshiki::document::MODELS_3D);
        models3d::set_display(&mut d, 0, display);
        d.validate().map_err(anyhow::Error::msg)?;
        write(name, &d)?;
    }
    Ok(())
}
