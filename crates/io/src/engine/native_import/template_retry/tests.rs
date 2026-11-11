use super::*;
use crate::chemistry::{
    cleanup, document as molecular,
    inchi::{kernel, key},
    smiles,
};

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
    // #9208's normalized ring traversal changes the native layout: independently
    // checked with RDKit 2026.09.1 legacy ring finding (11 in 2026.03.6).
    assert_eq!(collisions(state, &native.positions, 4096, 1.5), 9);
    let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
    assert_eq!(collisions(state, &positions, 4096, 1.5), 0);
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
    let input = smiles::read(ARTEMISININ)?;
    let mut molecule = super::super::molecule(input.prepared.state)?;
    // Freeze the origin/main 19f6dfd4 layout: #9208's normalized ring traversal
    // now generates another layout that no longer exercises this stereo guard.
    // The old coordinates also match RDKit 2026.03.6 within 1e-12.
    molecule.positions = [
        (-1.9492295060020077, -3.607201941380124),
        (-0.770654068654141, -2.6793226720350853),
        (0.6222006689175417, -3.2360593063821734),
        (1.800776106265408, -2.308180037037134),
        (1.586496806041593, -0.823564133345006),
        (2.765072243389461, 0.10431513600003438),
        (2.550792943165645, 1.5889310396921619),
        (3.7293683805135114, 2.516810309037202),
        (1.1579382055939638, 2.1456676740392493),
        (-0.020637231753902578, 1.2177884046942087),
        (0.19364206846991214, -0.2668274989979183),
        (-0.9849333688779551, -1.1947067683429577),
        (-2.4452083594889316, -0.8517826487536329),
        (-3.087566051251668, 0.5037160033420643),
        (-2.4282980005598542, 1.851071055782592),
        (-1.2868546146018736, 0.8778768322830984),
        (-1.2141784744460777, 0.8703537539582965),
        (-1.0204774577768714, 0.7138898026617155),
        (-3.3561772699048933, 3.029646493130459),
        (4.157926980961142, -0.452421498347052),
    ]
    .map(|(x, y)| Point3 { x, y, z: 0. })
    .to_vec();
    assert_eq!(
        collisions(&molecule.state, &molecule.positions, 4096, 1.5),
        11
    );
    let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = drawing.labels()?;
    let document = drawing.finish(labels)?;
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
fn clear_bridged_and_disjoint_ring_imports_keep_native_shape() -> anyhow::Result<()> {
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
        let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
        let mut expected = original.clone();
        crate::chemistry::layout_refinement::straighten(state, &mut expected, None);
        super::super::tests::assert_rigid_rotation(&original, &expected);
        crate::chemistry::layout_refinement::conventional_orientation(state, &mut expected);
        assert_eq!(positions, expected);
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
    // #9208 changes the untemplated layout from four overlaps to two; RDKit
    // 2026.09.1 legacy agrees, while the unchanged template still has nine.
    assert_eq!(collisions(state, &original, 4096, 1.5), 2);
    assert_eq!(collisions(state, &candidate, 4096, 1.5), 9);
    let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
    super::super::tests::assert_rigid_rotation(&original, &positions);
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

const CODEINE: &str = "COc1ccc2c3c1O[C@H]1[C@@H](O)C=C[C@H]4[C@@H](C2)N(C)CC[C@@]341";

/// The morphinan bridge cannot avoid crossing C7=C8, so it is drawn textbook
/// style: the bridge stays whole and the double bond under it gets a gap.
#[test]
fn morphinan_bridge_crosses_over_a_gapped_double_bond() -> anyhow::Result<()> {
    let morphine = "Oc1ccc2c3c1O[C@H]1[C@@H](O)C=C[C@H]4[C@@H](C2)N(C)CC[C@@]341";
    for text in [CODEINE, morphine] {
        let document = imported(text, true)?;
        let position = |id| document.atom(id).unwrap().position;
        let cross =
            |a: crate::document::Point, b: crate::document::Point, c: crate::document::Point| {
                (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
            };
        let mut crossings = vec![];
        for (i, first) in document.bonds.iter().enumerate() {
            for (j, second) in document.bonds.iter().enumerate().skip(i + 1) {
                let [a, b, c, d] = [first.a, first.b, second.a, second.b].map(position);
                if cross(a, b, c) * cross(a, b, d) < 0. && cross(c, d, a) * cross(c, d, b) < 0. {
                    crossings.push((i, j));
                }
            }
        }
        let [(i, j)] = crossings[..] else {
            panic!("{text}: expected one crossing, found {crossings:?}");
        };
        let (over, under) = if document.bonds[i].order == 1 {
            (i, j)
        } else {
            (j, i)
        };
        let (top, bottom) = (&document.bonds[over], &document.bonds[under]);
        assert_eq!((top.order, bottom.order), (1, 2), "{text}");
        let methylene = |id| {
            let degree = (document.bonds.iter())
                .filter(|b| b.a == id || b.b == id)
                .count();
            document.atom(id).unwrap().element == "C" && degree == 2
        };
        assert!(
            methylene(top.a) && methylene(top.b),
            "{text}: the CH2–CH2 bridge is drawn on top"
        );
        let gaps = crate::crossings::gaps(&document);
        assert!(gaps[over].is_empty() && !gaps[under].is_empty(), "{text}");
        assert_eq!(gaps.iter().filter(|g| !g.is_empty()).count(), 1, "{text}");
    }
    let aspirin = imported("CC(=O)Oc1ccccc1C(=O)O", true)?;
    assert!(crate::crossings::gaps(&aspirin).iter().all(Vec::is_empty));
    Ok(())
}

/// Smallest distance, in bond lengths, from an atom to a bond it is not part of.
fn atom_bond_clearance(state: &State, positions: &[Point3]) -> f64 {
    let mut best = f64::INFINITY;
    for bond in &state.graph.bonds {
        let (a, b) = (positions[bond.a], positions[bond.b]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy);
        for (atom, p) in positions.iter().enumerate() {
            if atom == bond.a || atom == bond.b {
                continue;
            }
            let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (length * length)).clamp(0., 1.);
            best = best.min((p.x - a.x - t * dx).hypot(p.y - a.y - t * dy) / 1.5);
        }
    }
    best
}

#[test]
fn codeine_import_keeps_every_atom_clear_of_bonds_it_does_not_belong_to() -> anyhow::Result<()> {
    let input = smiles::read(CODEINE)?;
    let state = &input.prepared.state;
    let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
    assert_eq!(collisions(state, &positions, 4096, 1.5), 0);
    let clearance = atom_bond_clearance(state, &positions);
    assert!(
        clearance >= 0.3,
        "atom lies {clearance:.3} bond lengths from a foreign bond"
    );
    assert_eq!(
        positions,
        super::super::coordinates(state, &input.reaction_properties)?.positions
    );
    let expected = smiles::write::write(state, Default::default())?.text;
    let inchi = kernel::generate(&kernel::Molecule::prepare(state, None)?)
        .map_err(anyhow::Error::msg)?
        .inchi;
    let expected_key = key::from_inchi(&inchi)?;
    let document = imported(CODEINE, true)?;
    let mut request = crate::engine::Request::molecule("clean", document.clone());
    request.cleanup = Some(crate::cleanup::Options {
        keep_orientation: true,
        ..Default::default()
    });
    let cleaned = crate::engine::native_cleanup::prepare(&request)?;
    for document in [&document, &cleaned.document] {
        let molecule = molecular::prepare(document)?;
        assert!(atom_bond_clearance(&molecule.state, &molecule.positions) >= 0.3);
        assert_eq!(
            smiles::write::write(&molecule.state, Default::default())?.text,
            expected
        );
        let inchi = kernel::generate(&kernel::Molecule::prepare(
            &molecule.state,
            Some(&molecule.positions),
        )?)
        .map_err(anyhow::Error::msg)?
        .inchi;
        assert_eq!(key::from_inchi(&inchi)?, expected_key);
        // Recover stereo from the actual wedges, independently of saved tags.
        let mut visible = document.clone();
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        for bond in &mut visible.bonds {
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
        assert_eq!(
            smiles::write::write(&molecular::prepare(&visible)?.state, Default::default())?.text,
            expected
        );
    }
    Ok(())
}

const PORPHINE: &str = "C1=Cc2cc3ccc(cc4ccc(cc5nc(cc1n2)C=C5)[nH]4)[nH]3";
const TETRAPHENYLPORPHINE: &str =
    "c1ccc(cc1)c2c3ccc(n3)c(c4ccccc4)c5ccc([nH]5)c(c6ccccc6)c7ccc(n7)c(c8ccccc8)c9ccc2[nH]9";
/// RDKit has no template for the phthalocyanine core, so its default layout stays.
const PHTHALOCYANINE: &str =
    "c1ccc2c(c1)c3nc2nc4[nH]c(nc5nc(nc6[nH]c(n3)c7ccccc67)c8ccccc58)c9ccccc49";

fn bond_lengths(document: &crate::document::Document) -> Vec<f32> {
    let position = |id| document.atom(id).unwrap().position;
    (document.bonds.iter())
        .map(|bond| position(bond.a).distance(position(bond.b)))
        .collect()
}

fn assert_uniform_bonds(document: &crate::document::Document) {
    let lengths = bond_lengths(document);
    let longest = lengths.iter().cloned().fold(0_f32, f32::max);
    let shortest = lengths.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(
        longest - shortest < 0.01 * longest,
        "bond lengths range from {shortest} to {longest}"
    );
}

/// The four nitrogens of a regular porphyrin core form a square.
fn assert_square_core(document: &crate::document::Document) {
    let nitrogens: Vec<_> = (document.atoms.iter())
        .filter(|atom| atom.element == "N")
        .map(|atom| atom.position)
        .collect();
    assert_eq!(nitrogens.len(), 4);
    let mut distances: Vec<f32> = (0..4)
        .flat_map(|i| (i + 1..4).map(move |j| (i, j)))
        .map(|(i, j)| nitrogens[i].distance(nitrogens[j]))
        .collect();
    distances.sort_by(f32::total_cmp);
    let side = distances[0];
    for &edge in &distances[..4] {
        assert!((edge - side).abs() < 0.01 * side, "sides {distances:?}");
    }
    for &diagonal in &distances[4..] {
        assert!(
            (diagonal - side * 2_f32.sqrt()).abs() < 0.01 * side,
            "diagonals {distances:?}"
        );
    }
}

fn pairwise_distances(document: &crate::document::Document) -> Vec<f32> {
    let n = document.atoms.len();
    (0..n)
        .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
        .map(|(i, j)| {
            document.atoms[i]
                .position
                .distance(document.atoms[j].position)
        })
        .collect()
}

#[test]
fn stretched_porphine_import_takes_its_regular_template() -> anyhow::Result<()> {
    let input = smiles::read(PORPHINE)?;
    let state = &input.prepared.state;
    let native = depict::compute(
        state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    assert!(bridged(state));
    assert_eq!(collisions(state, &native, 4096, 1.5), 0);
    assert!(
        spread(state, &native, 1.5) > 0.25,
        "RDKit's default layout stretches 1.50 to 1.95"
    );
    let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
    assert!(spread(state, &positions, 1.5) <= 0.01);
    assert_eq!(collisions(state, &positions, 4096, 1.5), 0);
    assert_eq!(
        positions,
        super::super::coordinates(state, &input.reaction_properties)?.positions
    );
    Ok(())
}

#[test]
fn porphyrin_imports_and_cleanups_draw_the_same_regular_macrocycle() -> anyhow::Result<()> {
    for text in [PORPHINE, TETRAPHENYLPORPHINE] {
        let expected =
            smiles::write::write(&smiles::read(text)?.prepared.state, Default::default())?.text;
        let document = imported(text, true)?;
        assert_uniform_bonds(&document);
        assert_square_core(&document);
        let mut request = crate::engine::Request::molecule("clean", document.clone());
        request.cleanup = Some(crate::cleanup::Options {
            keep_orientation: true,
            ..Default::default()
        });
        let cleaned = crate::engine::native_cleanup::prepare(&request)?.document;
        assert_uniform_bonds(&cleaned);
        assert_square_core(&cleaned);
        for (a, b) in pairwise_distances(&document)
            .iter()
            .zip(pairwise_distances(&cleaned))
        {
            assert!(
                (a - b).abs() < 1e-3 * a.max(1.),
                "{text}: cleanup changed the shape"
            );
        }
        for document in [&document, &cleaned] {
            assert_eq!(
                smiles::write::write(&molecular::prepare(document)?.state, Default::default())?
                    .text,
                expected
            );
        }
    }
    Ok(())
}

#[test]
fn a_stretched_macrocycle_without_a_template_keeps_its_native_layout() -> anyhow::Result<()> {
    let input = smiles::read(PHTHALOCYANINE)?;
    let state = &input.prepared.state;
    let ranks = vec![None; state.graph.atoms.len()];
    let original = depict::compute(state, &ranks, None, Default::default())?.positions;
    assert!(bridged(state));
    assert_eq!(collisions(state, &original, 4096, 1.5), 0);
    assert!(spread(state, &original, 1.5) > 0.25);
    let positions = super::super::coordinates(state, &input.reaction_properties)?.positions;
    let mut expected = original.clone();
    crate::chemistry::layout_refinement::straighten(state, &mut expected, None);
    crate::chemistry::layout_refinement::conventional_orientation(state, &mut expected);
    assert_eq!(positions, expected);
    Ok(())
}
