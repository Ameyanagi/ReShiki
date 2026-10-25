use super::*;
use crate::chemistry::{smiles, stereo::wedging::Conformer};

fn input(text: &str) -> anyhow::Result<(State, Vec<depict::ranks::AtomProperties>, Conformer)> {
    let input = smiles::read(text)?;
    let ranks = input
        .reaction_properties
        .iter()
        .map(|p| depict::ranks::AtomProperties::from_pairs(p))
        .collect::<Vec<_>>();
    let original = depict::compute_with_rank_properties(
        &input.prepared.state,
        &ranks,
        None,
        Default::default(),
    )?;
    Ok((input.prepared.state, ranks, original))
}

#[test]
fn long_polystyrene_and_alanine_layouts_are_clear_deterministic_and_keep_visible_stereo()
-> anyhow::Result<()> {
    for (name, text) in [
        ("polystyrene30", "CC(c1ccccc1)".repeat(30)),
        ("polystyrene75", "CC(c1ccccc1)".repeat(75)),
        ("polystyrene100", "CC(c1ccccc1)".repeat(100)),
        ("alanine200", format!("{}O", "NC(C)C(=O)".repeat(200))),
        (
            "stereo-alanine200",
            format!("{}O", "N[C@@H](C)C(=O)".repeat(200)),
        ),
    ] {
        let started = std::time::Instant::now();
        let (state, ranks, original) = input(&text)?;
        let unchanged = serde_json::to_vec(&state)?;
        assert_ne!(
            clearance::geometry(&state, &original.positions),
            Quality::Clear,
            "{name}"
        );
        let result = improve(&state, &ranks, original.positions.clone());
        assert!(result.warnings.is_empty(), "{name}: {:?}", result.warnings);
        assert_eq!(
            clearance::geometry(&state, &result.positions),
            Quality::Clear,
            "{name}"
        );
        let doc = label_document(&state, &result.positions)
            .ok_or_else(|| anyhow::anyhow!("Label drawing"))?;
        assert_eq!(
            clearance::labels(&state, &result.positions, &doc),
            Quality::Clear,
            "{name}"
        );
        assert_eq!(
            result.positions,
            improve(&state, &ranks, original.positions.clone()).positions,
            "{name}"
        );
        assert_eq!(serde_json::to_vec(&state)?, unchanged);
        let expected = smiles::write::write(&state, Default::default())?.text;
        let mut visible = doc.clone();
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        for bond in &mut visible.bonds {
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
        let perceived = molecular::prepare(&visible)?;
        assert_eq!(
            smiles::write::write(&perceived.state, Default::default())?.text,
            expected,
            "{name} visible stereo"
        );
        if name == "stereo-alanine200" {
            assert_eq!(
                state
                    .metadata
                    .atoms
                    .iter()
                    .filter(|a| matches!(a.chiral_tag, 1 | 2))
                    .count(),
                200
            );
            assert_eq!(
                perceived
                    .state
                    .metadata
                    .atoms
                    .iter()
                    .filter(|a| matches!(a.chiral_tag, 1 | 2))
                    .count(),
                200
            );
        }
        if let Ok(dir) = std::env::var("RESHIKI_CHAIN_QA_DIR") {
            capture(&dir, name, &state, &original.positions, &result.positions)?;
        }
        eprintln!(
            "{name}: {} atoms clear; total regression time {:.3}s",
            state.graph.atoms.len(),
            started.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

fn capture(
    dir: &str,
    name: &str,
    state: &State,
    before: &[Point3],
    after: &[Point3],
) -> anyhow::Result<()> {
    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir)?;
    let before = label_document(state, before).ok_or_else(|| anyhow::anyhow!("Before drawing"))?;
    let after = label_document(state, after).ok_or_else(|| anyhow::anyhow!("After drawing"))?;
    for (suffix, doc) in [("before", before), ("after", after)] {
        std::fs::write(
            dir.join(format!("{name}-{suffix}.rsk")),
            doc.file_json().map_err(anyhow::Error::msg)?,
        )?;
        std::fs::write(
            dir.join(format!("{name}-{suffix}.svg")),
            crate::scene::svg_with_background(&doc),
        )?;
    }
    Ok(())
}

#[test]
fn exhausted_budget_preserves_original_and_reports_overlaps() -> anyhow::Result<()> {
    let (state, ranks, original) = input(&"CC(c1ccccc1)".repeat(30))?;
    let result = improve_with_limit(&state, &ranks, original.positions.clone(), 0);
    assert_eq!(result.positions, original.positions);
    assert_eq!(result.warnings, [NOTICE]);
    Ok(())
}

#[test]
fn exclusions_and_small_or_clear_drawings_preserve_native_coordinates() -> anyhow::Result<()> {
    let (state, ranks, original) = input("CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC")?;
    assert_eq!(
        improve(&state, &ranks, original.positions.clone()).positions,
        original.positions
    );
    assert!(
        improve(&state, &ranks, original.positions)
            .warnings
            .is_empty()
    );
    let (mut state, _, _) = input(&"CC(c1ccccc1)".repeat(30))?;
    assert!(backbone(&state).is_some());
    state
        .metadata
        .bonds
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("bond"))?
        .stereo = 3;
    assert!(!supported_stereo(&state));
    state
        .metadata
        .bonds
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("bond"))?
        .stereo = 6;
    assert!(!supported_stereo(&state));
    state
        .metadata
        .bonds
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("bond"))?
        .stereo = 0;
    state
        .metadata
        .atoms
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("atom"))?
        .chiral_tag = 4;
    assert!(!supported_stereo(&state));
    let (state, _, _) = input("CC(c1ccccc1)CC(c1ccccc1)")?;
    assert!(backbone(&state).is_none());
    let (state, ranks, original) =
        input("C[C@@H]1CC[C@H]2[C@H](C(=O)O[C@H]3[C@@]24[C@H]1CC[C@](O3)(OO4)C)C")?;
    let result = improve(&state, &ranks, original.positions.clone());
    assert_eq!(result.positions, original.positions);
    assert!(result.warnings.is_empty());
    Ok(())
}

#[test]
fn unsupported_long_chain_keeps_its_layout_and_the_import_warning() -> anyhow::Result<()> {
    let text = format!("C/C=C/{}", "CC(c1ccccc1)".repeat(30));
    let (state, _, original) = input(&text)?;
    assert!(backbone(&state).is_some());
    assert!(!supported_stereo(&state));
    assert_ne!(
        clearance::geometry(&state, &original.positions),
        Quality::Clear
    );
    let mut request = crate::engine::Request::molecule("import", Document::default());
    request.format = Some("smiles".into());
    request.text = Some(text);
    let super::super::Preparation::Complete(prepared) = super::super::prepare(request)? else {
        anyhow::bail!("Import did not complete");
    };
    assert_eq!(prepared.molecule.positions, original.positions);
    assert_eq!(prepared.warnings, [NOTICE]);
    Ok(())
}
