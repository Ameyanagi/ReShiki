//! Opt-in profiling of the real import stages, without screenshot-poll overhead.
use super::*;
use reshiki::{
    atom_labels::refresh::Refresh,
    chemistry::{self, depict, document as molecular, smiles},
    engine::native_response,
    scene,
};
use std::{sync::Arc, time::Instant};

#[tokio::test]
async fn imports_snapshot_the_originating_documents_wedge_preference() -> Result<(), String> {
    use iced::futures::StreamExt;
    for two_wedges in [false, true] {
        let (mut app, _) = App::new();
        app.engine = reshiki::engine::LocalEngine::in_process();
        app.tab.busy = false;
        app.tab.doc.drawing_style.two_wedge_stereocentres = two_wedges;
        let task = app.run(Request::import_smiles("CC[C@](C)(Br)Cl"), Job::Insert);
        // A later style change must not alter a request already launched.
        app.tab.doc.drawing_style.two_wedge_stereocentres = !two_wedges;
        let mut stream = iced_runtime::task::into_stream(task).ok_or("Missing import task")?;
        let Some(iced_runtime::Action::Output(Message::EngineFinished { result, .. })) =
            stream.next().await
        else {
            return Err("Missing import result".into());
        };
        let drawing = (*result)?.document.ok_or("Missing imported drawing")?;
        let wedges = drawing
            .bonds
            .iter()
            .filter(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
            .count();
        assert_eq!(wedges, if two_wedges { 2 } else { 1 });
    }
    Ok(())
}

fn elapsed(stages: &mut Vec<(&'static str, f64)>, name: &'static str, started: Instant) {
    stages.push((name, started.elapsed().as_secs_f64()));
}

#[tokio::test]
#[ignore = "manual before/after import profiling; use --release and --nocapture"]
async fn profile_polyalanine_insert_stages() -> anyhow::Result<()> {
    for (name, unit) in [
        ("alanine200", "NC(C)C(=O)"),
        ("stereo-alanine200", "N[C@@H](C)C(=O)"),
    ] {
        let mut stages = Vec::new();
        let text = format!("{}O", unit.repeat(200));
        let started = Instant::now();
        let input = smiles::read(&text)?;
        elapsed(&mut stages, "parse", started);
        let mut molecule = molecular::Molecule {
            rdkit_version: chemistry::RDKIT_VERSION,
            ids: (1..=input.prepared.state.graph.atoms.len() as u64).collect(),
            positions: vec![],
            state: input.prepared.state,
        };
        let ranks = input
            .reaction_properties
            .iter()
            .map(|p| depict::ranks::AtomProperties::from_pairs(p))
            .collect::<Vec<_>>();
        let started = Instant::now();
        molecule.positions = depict::compute_with_rank_properties(
            &molecule.state,
            &ranks,
            None,
            Default::default(),
        )?
        .positions;
        elapsed(&mut stages, "layout", started);
        let started = Instant::now();
        let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
        let labels = drawing.labels()?;
        let document = drawing.finish(labels)?;
        document.validate().map_err(anyhow::Error::msg)?;
        elapsed(&mut stages, "drawing_and_labels", started);
        let started = Instant::now();
        let _analysis = native_response::analyze_prepared(Arc::new(molecule), None).await?;
        elapsed(&mut stages, "analysis", started);
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        app.viewport = iced::Size::new(1040., 680.);
        let before = app.tab.doc.clone();
        let started = Instant::now();
        app.tab.selected = app
            .insert_beside_drawing(&document)
            .ok_or_else(|| anyhow::anyhow!("Insert failed"))?;
        elapsed(&mut stages, "append", started);
        let started = Instant::now();
        app.changed(before.clone());
        assert!(!app.error, "{}", app.status);
        elapsed(&mut stages, "reconcile_and_commit", started);
        let started = Instant::now();
        app.fit();
        elapsed(&mut stages, "fit", started);
        let started = Instant::now();
        assert!(!scene::primitives(&app.tab.doc).is_empty());
        elapsed(&mut stages, "first_scene", started);
        let started = Instant::now();
        let refresh =
            Refresh::calculate(&app.tab.doc, &Refresh::default()).map_err(anyhow::Error::msg)?;
        elapsed(&mut stages, "background_labels", started);
        let started = Instant::now();
        refresh.apply(&mut app.tab.doc);
        app.settle_label_layout();
        elapsed(&mut stages, "settle_labels", started);
        let started = Instant::now();
        assert!(!scene::primitives(&app.tab.doc).is_empty());
        elapsed(&mut stages, "labelled_scene", started);
        assert_eq!(app.tab.doc.atoms.len(), 1001);
        assert_eq!(app.tab.doc.bonds.len(), 1000);
        assert_eq!(app.tab.history.frames(), 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        eprintln!("{}", serde_json::json!({"fixture":name,"stages":stages}));
    }
    Ok(())
}
