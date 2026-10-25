use super::*;
use crate::chemistry::{cleanup, document as molecular, smiles};

const ARTEMISININ: &str = "C[C@@H]1CC[C@H]2[C@H](C(=O)O[C@H]3[C@@]24[C@H]1CC[C@](O3)(OO4)C)C";

fn imported(text: &str, improved: bool) -> anyhow::Result<crate::document::Document> {
    let input = smiles::read(text)?;
    let mut molecule = super::super::molecule(input.prepared.state)?;
    molecule.positions = if improved {
        super::super::coordinates(&molecule.state, &input.reaction_properties)?
    } else {
        depict::compute(
            &molecule.state,
            &vec![None; molecule.ids.len()],
            None,
            Default::default(),
        )?
        .positions
    };
    let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = drawing.labels()?;
    Ok(drawing.finish(labels)?)
}

fn clean(document: &crate::document::Document) -> Result<cleanup::Cleaned, cleanup::Error> {
    clean_selected(
        document,
        crate::cleanup::Options::default(),
        &document.all_ids(),
    )
}

fn clean_selected(
    document: &crate::document::Document,
    options: crate::cleanup::Options,
    selected: &[u64],
) -> Result<cleanup::Cleaned, cleanup::Error> {
    let prepared = cleanup::prepare(document, options, selected)?;
    let layouts = prepared
        .requests()
        .map(|request| {
            depict::compute(
                &request.molecule.state,
                &vec![None; request.molecule.ids.len()],
                Some(request.fixed),
                depict::Options {
                    bond_length: request.bond_length,
                    canonical_orientation: request.canonical_orientation,
                    use_ring_templates: request.use_ring_templates,
                    ..Default::default()
                },
            )
            .map(|conformer| conformer.positions)
            .map_err(|error| cleanup::Error::Document(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    prepared.finish(layouts)
}

#[test]
fn bridged_template_retry_separates_artemisinin_and_preserves_cleanup_identity()
-> anyhow::Result<()> {
    let input = smiles::read(ARTEMISININ)?;
    let state = &input.prepared.state;
    let native = depict::compute(
        state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?;
    assert!(bridged(state));
    assert_eq!(collisions(state, &native.positions, 4096), 2);
    let positions = super::super::coordinates(state, &input.reaction_properties)?;
    assert_eq!(collisions(state, &positions, 4096), 0);
    assert_eq!(
        positions,
        super::super::coordinates(state, &input.reaction_properties)?
    );
    let document = imported(ARTEMISININ, true)?;
    let ink = document
        .atoms
        .iter()
        .filter_map(|atom| crate::scene::atom_label_ink_bounds(atom, &document))
        .collect::<Vec<_>>();
    for (index, &(lo, hi)) in ink.iter().enumerate() {
        for &(other_lo, other_hi) in ink.iter().skip(index + 1) {
            assert!(
                hi.x <= other_lo.x
                    || other_hi.x <= lo.x
                    || hi.y <= other_lo.y
                    || other_hi.y <= lo.y,
                "Imported label ink overlaps: {lo:?}..{hi:?} and {other_lo:?}..{other_hi:?}"
            );
        }
    }
    let before = molecular::prepare(&document)?;
    assert_eq!(
        smiles::write::write(state, Default::default())?.text,
        smiles::write::write(&before.state, Default::default())?.text
    );
    let cleaned = clean(&document)?;
    let after = molecular::prepare(&cleaned.document)?;
    assert_eq!(
        smiles::write::write(&before.state, Default::default())?.text,
        smiles::write::write(&after.state, Default::default())?.text
    );
    assert_eq!(before.state.graph.atoms.len(), 20);
    assert_eq!(before.state.graph.bonds.len(), 23);
    Ok(())
}

#[test]
fn old_overlapping_whole_molecule_keeps_the_visible_stereo_guard() -> anyhow::Result<()> {
    let document = imported(ARTEMISININ, false)?;
    let original = document.clone();
    let error = clean(&document).unwrap_err();
    assert!(matches!(error, cleanup::Error::VisibleChangedWhole));
    assert!(!error.to_string().contains("whole molecule"));
    assert!(error.to_string().contains("drawing unchanged"));
    assert_eq!(document, original);
    let partial = clean_selected(
        &document,
        crate::cleanup::Options {
            scope: crate::cleanup::Scope::SelectedAtoms,
            ..Default::default()
        },
        &document
            .atoms
            .iter()
            .skip(1)
            .map(|a| a.id)
            .collect::<Vec<_>>(),
    )
    .unwrap_err();
    assert!(matches!(partial, cleanup::Error::VisibleChanged));
    assert!(partial.to_string().contains("include the whole molecule"));
    Ok(())
}

#[test]
fn clear_bridged_and_disjoint_ring_imports_keep_native_coordinates() -> anyhow::Result<()> {
    for text in ["C1C2CC3CC1CC(C2)C3", "CC(c1ccccc1)CC(c1ccccc1)"] {
        let input = smiles::read(text)?;
        let state = &input.prepared.state;
        let original = depict::compute(
            state,
            &vec![None; state.graph.atoms.len()],
            None,
            Default::default(),
        )?
        .positions;
        assert_eq!(
            super::super::coordinates(state, &input.reaction_properties)?,
            original
        );
    }
    Ok(())
}

#[test]
fn a_template_that_increases_collisions_is_not_published() -> anyhow::Result<()> {
    let text = "C1CC2CC3CCCC45C(CC(C1)C24)C1CCC35CC1";
    let input = smiles::read(text)?;
    let state = &input.prepared.state;
    let ranks = vec![None; state.graph.atoms.len()];
    let original = depict::compute(state, &ranks, None, Default::default())?.positions;
    let candidate = depict::compute(
        state,
        &ranks,
        None,
        depict::Options {
            use_ring_templates: true,
            ..Default::default()
        },
    )?
    .positions;
    assert_eq!(collisions(state, &original, 4096), 2);
    assert_eq!(collisions(state, &candidate, 4096), 4);
    assert_eq!(
        super::super::coordinates(state, &input.reaction_properties)?,
        original
    );
    Ok(())
}
