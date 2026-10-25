use super::*;
use crate::chemistry::{cleanup, document as molecular, smiles};

const ARTEMISININ: &str = "C[C@@H]1CC[C@H]2[C@H](C(=O)O[C@H]3[C@@]24[C@H]1CC[C@](O3)(OO4)C)C";

fn imported(text: &str, improved: bool) -> anyhow::Result<crate::document::Document> {
    let input = smiles::read(text)?;
    let mut molecule = super::super::molecule(input.prepared.state)?;
    molecule.positions = if improved {
        super::super::coordinates(&molecule.state, &input.reaction_properties)?.positions
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
    let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
    assert_eq!(collisions(state, &positions, 4096), 0);
    assert_eq!(
        positions,
        super::super::coordinates(state, &input.reaction_properties)?.positions
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
            super::super::coordinates(state, &input.reaction_properties)?.positions,
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
        super::super::coordinates(state, &input.reaction_properties)?.positions,
        original
    );
    Ok(())
}

fn visible_identity(document: &crate::document::Document) -> anyhow::Result<String> {
    let mut drawing = document.clone();
    for atom in &mut drawing.atoms {
        atom.stereo = None;
        atom.cip_label = None;
    }
    for bond in &mut drawing.bonds {
        bond.stereo = None;
        bond.stereo_atoms.clear();
    }
    let visible = molecular::prepare(&drawing)?;
    assert_eq!(
        visible
            .state
            .metadata
            .atoms
            .iter()
            .filter(|atom| atom.chiral_tag != 0)
            .count(),
        7
    );
    Ok(smiles::write::write(&visible.state, Default::default())?.text)
}

fn outward_methyl(document: &crate::document::Document) {
    let center = document.atom(15).unwrap().position;
    let methyl = document.atom(19).unwrap().position;
    for neighbor in [14, 16, 17] {
        let ring = document.atom(neighbor).unwrap().position;
        assert!(
            (methyl.x - center.x) * (ring.x - center.x)
                + (methyl.y - center.y) * (ring.y - center.y)
                < 0.,
            "The methyl must be outside all three ring rays"
        );
    }
}

fn peroxide_ink(document: &crate::document::Document) {
    use crate::{document::Point, scene::Primitive};
    let a = document.atom(17).unwrap().position;
    let b = document.atom(18).unwrap().position;
    let point = a.offset((b.x - a.x) * 0.5, (b.y - a.y) * 0.5);
    let inside = |polygon: &[Point]| {
        polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
            .filter(|(a, b)| {
                (a.y > point.y) != (b.y > point.y)
                    && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            })
            .count()
            % 2
            == 1
    };
    assert!(
        crate::scene::primitives(document)
            .iter()
            .any(|part| match part {
                Primitive::Polygon(points) => inside(points),
                Primitive::Path {
                    commands,
                    filled: true,
                    ..
                } => crate::graphics::flattened(commands)
                    .iter()
                    .any(|points| inside(points)),
                _ => false,
            }),
        "The actual scene must contain the peroxide O–O connector, not just its graph edge"
    );
}

#[test]
fn exterior_methyl_and_visible_peroxide_keep_all_seven_visible_stereocentres() -> anyhow::Result<()>
{
    let input = smiles::read(ARTEMISININ)?;
    let expected = smiles::write::write(&input.prepared.state, Default::default())?.text;
    let document = imported(ARTEMISININ, true)?;
    outward_methyl(&document);
    peroxide_ink(&document);
    assert_eq!(visible_identity(&document)?, expected);
    let cleaned = clean(&document)?.document;
    outward_methyl(&cleaned);
    peroxide_ink(&cleaned);
    assert_eq!(visible_identity(&cleaned)?, expected);
    let repeated = clean(&cleaned)?.document;
    outward_methyl(&repeated);
    peroxide_ink(&repeated);
    assert_eq!(visible_identity(&repeated)?, expected);
    for (a, b) in cleaned.atoms.iter().zip(&repeated.atoms) {
        assert!(
            a.position.distance(b.position) < 0.001,
            "Repeated cleanup must not drift"
        );
    }
    Ok(())
}

#[test]
fn cleanup_repairs_the_previous_inward_template_without_changing_visible_identity()
-> anyhow::Result<()> {
    let input = smiles::read(ARTEMISININ)?;
    let mut molecule = super::super::molecule(input.prepared.state)?;
    molecule.positions = depict::compute(
        &molecule.state,
        &vec![None; molecule.ids.len()],
        None,
        depict::Options {
            use_ring_templates: true,
            ..Default::default()
        },
    )?
    .positions;
    let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = drawing.labels()?;
    let old = drawing.finish(labels)?;
    let expected = visible_identity(&old)?;
    let cleaned = clean(&old)?.document;
    outward_methyl(&cleaned);
    peroxide_ink(&cleaned);
    assert_eq!(visible_identity(&cleaned)?, expected);
    Ok(())
}
