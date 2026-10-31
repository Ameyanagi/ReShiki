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
    let original = vec![
        Point3 {
            x: 1.1,
            y: 2.7,
            z: 0.
        };
        state.graph.atoms.len()
    ];
    let result = for_cleanup(&state, original.clone(), 1.7, true);
    assert_eq!(result.positions, original);
    assert!(result.warnings.is_empty());
    let (state, ranks, original) =
        input("C[C@@H]1CC[C@H]2[C@H](C(=O)O[C@H]3[C@@]24[C@H]1CC[C@](O3)(OO4)C)C")?;
    let result = improve(&state, &ranks, original.positions.clone());
    assert_eq!(result.positions, original.positions);
    assert!(result.warnings.is_empty());
    Ok(())
}

#[test]
fn unsupported_long_chain_keeps_its_shape_and_the_import_warning() -> anyhow::Result<()> {
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
    super::super::tests::assert_rigid_rotation(&original.positions, &prepared.molecule.positions);
    assert_eq!(prepared.warnings, [NOTICE]);
    Ok(())
}

#[test]
fn whole_cleanup_of_an_imported_long_chain_stays_clear() -> anyhow::Result<()> {
    for (name, text, formula) in [
        ("polystyrene100", "CC(c1ccccc1)".repeat(100), "C800H802"),
        (
            "alanine200",
            format!("{}O", "NC(C)C(=O)".repeat(200)),
            "C600H1002N200O201",
        ),
        (
            "stereo-alanine200",
            format!("{}O", "N[C@@H](C)C(=O)".repeat(200)),
            "C600H1002N200O201",
        ),
    ] {
        let super::super::Preparation::Complete(imported) =
            super::super::prepare(crate::engine::Request::import_smiles(&text))?
        else {
            anyhow::bail!("{name}: import did not complete");
        };
        assert!(imported.warnings.is_empty(), "{name}");
        let expected = smiles::write::write(&imported.molecule.state, Default::default())?.text;
        let request = crate::engine::Request::molecule("clean", imported.document);
        let cleaned = crate::engine::native_cleanup::prepare(&request)?;
        assert!(
            cleaned.warnings.is_empty(),
            "{name}: {:?}",
            cleaned.warnings
        );
        let after = cleaned.molecule.expect("whole cleanup analysis");
        assert_eq!(
            clearance::geometry(&after.state, &after.positions),
            Quality::Clear,
            "{name} geometry"
        );
        assert_eq!(
            clearance::labels(&after.state, &after.positions, &cleaned.document),
            Quality::Clear,
            "{name} label ink"
        );
        assert_eq!(
            smiles::write::write(&after.state, Default::default())?.text,
            expected,
            "{name} canonical isomeric SMILES"
        );
        let facts = after.state.graph.atom_facts().map_err(anyhow::Error::msg)?;
        assert_eq!(
            crate::chemistry::properties(&facts)
                .map_err(anyhow::Error::msg)?
                .formula,
            formula,
            "{name} formula"
        );
        let mut visible = cleaned.document;
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        for bond in &mut visible.bonds {
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
        assert_eq!(
            smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?.text,
            expected,
            "{name} visible stereochemistry"
        );
    }
    Ok(())
}

#[test]
fn cleanup_fallback_restores_the_requested_bond_length() -> anyhow::Result<()> {
    let (state, _, original) = input(&"CC(c1ccccc1)".repeat(30))?;
    for scale in [0.75, 1.6] {
        let mut scaled = original.positions.clone();
        for p in &mut scaled {
            p.x *= scale;
            p.y *= scale;
        }
        let result = for_cleanup(&state, scaled, 1.5 * scale, false);
        assert!(result.warnings.is_empty());
        for bond in &state.graph.bonds {
            let a = result.positions[bond.a];
            let b = result.positions[bond.b];
            assert!(((a.x - b.x).hypot(a.y - b.y) - 1.5 * scale).abs() < 1e-6);
        }
        let normalized = result
            .positions
            .iter()
            .map(|p| Point3 {
                x: p.x / scale,
                y: p.y / scale,
                z: p.z,
            })
            .collect::<Vec<_>>();
        assert_eq!(clearance::geometry(&state, &normalized), Quality::Clear);
    }
    let document = label_document(&state, &original.positions)
        .ok_or_else(|| anyhow::anyhow!("Scaled cleanup drawing"))?;
    let mut request = crate::engine::Request::molecule("clean", document);
    request
        .document
        .as_mut()
        .expect("cleanup document")
        .drawing_style
        .set_bond_length(14.4 * 1.6);
    let cleaned = crate::engine::native_cleanup::prepare(&request)?;
    assert!(cleaned.warnings.is_empty());
    let after = cleaned.molecule.expect("scaled whole cleanup analysis");
    let bond_length = f64::from(cleaned.document.drawing_style.bond_length_world) / 28.;
    for bond in &after.state.graph.bonds {
        let a = after.positions[bond.a];
        let b = after.positions[bond.b];
        assert!(((a.x - b.x).hypot(a.y - b.y) - bond_length).abs() < 1e-6);
    }
    Ok(())
}

#[test]
fn partial_cleanup_preserves_fixed_atoms_and_warns_about_crowded_chains() -> anyhow::Result<()> {
    let (state, _, original) = input(&"CC(c1ccccc1)".repeat(30))?;
    let result = for_cleanup(&state, original.positions.clone(), 1.5, true);
    assert_eq!(result.positions, original.positions);
    assert_eq!(result.warnings, [NOTICE]);
    let molecule = molecular::Molecule {
        positions: original.positions,
        ..super::super::molecule(state)?
    };
    let drawing = molecular::for_import(&molecule, false, &vec![None; molecule.ids.len()])?;
    let labels = drawing.labels()?;
    let document = drawing.finish(labels)?;
    let mut request = crate::engine::Request::molecule("clean", document.clone());
    request.cleanup = Some(crate::cleanup::Options {
        scope: crate::cleanup::Scope::SelectedAtoms,
        ..Default::default()
    });
    request.selected_ids = Some(vec![molecule.ids[0]]);
    let cleaned = crate::engine::native_cleanup::prepare(&request)?;
    assert_eq!(cleaned.warnings, [NOTICE]);
    for (before, after) in document.atoms.iter().zip(&cleaned.document.atoms).skip(1) {
        assert_eq!(after.position, before.position);
    }
    Ok(())
}
