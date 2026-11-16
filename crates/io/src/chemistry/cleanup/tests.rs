use super::*;
use crate::chemistry::{depict, smiles};
use std::f64::consts::PI;

const PROPYLBENZENE: &str = "CCCc1ccccc1";
const ASPIRIN: &str = "CC(=O)Oc1ccccc1C(=O)O";

#[test]
fn stereo_groups_cleanup_preserves_all_source_members() -> anyhow::Result<()> {
    use crate::stereo_groups::{Kind, StereoGroup};
    let mut source = drawing("C[C@H](O)c1ccccc1")?;
    source.atoms[1].stereo_group = Some(StereoGroup {
        kind: Kind::And,
        number: 7,
    });
    // Even a member removed by stereocentre perception remains an editable assignment.
    source.atoms[0].stereo_group = Some(StereoGroup {
        kind: Kind::Or,
        number: 3,
    });
    let expected: Vec<_> = source
        .atoms
        .iter()
        .map(|atom| (atom.id, atom.stereo_group))
        .collect();
    let cleaned = clean(&source, Options::default(), &[], 0.)?;
    assert_eq!(
        cleaned
            .document
            .atoms
            .iter()
            .map(|atom| (atom.id, atom.stereo_group))
            .collect::<Vec<_>>(),
        expected
    );
    Ok(())
}

#[test]
fn unspecified_alkene_cleanup_uses_document_depiction_and_notice() -> anyhow::Result<()> {
    for wavy in [false, true] {
        let mut source = Document::default();
        source.drawing_style.wavy_unknown_alkenes = wavy;
        let atoms = (0..5)
            .map(|i| source.add_atom("C", crate::document::Point::new(i as f32 * 42., 0.)))
            .collect::<Vec<_>>();
        for (i, pair) in atoms.windows(2).enumerate() {
            source.add_bond(pair[0], pair[1], if i == 1 { 2 } else { 1 }, "plain");
        }
        let cleaned = clean(&source, Options::default(), &[], 0.)?;
        assert_eq!(cleaned.document.atoms.len(), source.atoms.len());
        let expected = if wavy {
            "wavy substituent bond"
        } else {
            "crossed depiction"
        };
        assert!(
            cleaned
                .warnings
                .iter()
                .any(|warning| warning.contains(expected)),
            "{:?}",
            cleaned.warnings
        );
        assert_eq!(
            cleaned
                .document
                .bonds
                .iter()
                .filter(|bond| bond.order == 1 && bond.display == "wavy")
                .count(),
            usize::from(wavy)
        );
        assert_eq!(
            cleaned
                .document
                .bonds
                .iter()
                .filter(|bond| bond.order == 2 && bond.display == "wavy")
                .count(),
            usize::from(!wavy)
        );
        assert!(
            cleaned
                .molecule
                .unwrap()
                .state
                .metadata
                .bonds
                .iter()
                .all(|bond| bond.stereo <= 1)
        );
    }
    Ok(())
}

#[test]
fn crossed_alkene_cleanup_updates_and_reuses_the_wavy_depiction() -> anyhow::Result<()> {
    let mut source = Document::default();
    source.drawing_style.wavy_unknown_alkenes = true;
    let atoms = (0..5)
        .map(|i| source.add_atom("C", crate::document::Point::new(i as f32 * 42., 0.)))
        .collect::<Vec<_>>();
    for (i, pair) in atoms.windows(2).enumerate() {
        source.add_bond(
            pair[0],
            pair[1],
            if i == 1 { 2 } else { 1 },
            if i == 1 { "wavy" } else { "plain" },
        );
    }
    assert_eq!(
        molecular::prepare(&source)?.state.metadata.bonds[1].stereo,
        1
    );
    for _ in 0..3 {
        let cleaned = clean(&source, Options::default(), &[], 0.)?;
        assert_eq!(cleaned.document.bonds[1].display, "plain");
        assert_eq!(
            cleaned
                .document
                .bonds
                .iter()
                .filter(|bond| bond.order == 1 && bond.display == "wavy")
                .count(),
            1
        );
        assert!(
            cleaned
                .warnings
                .iter()
                .any(|warning| warning.contains("wavy substituent bond"))
        );
        if source.bonds[1].display == "plain" {
            assert_eq!(cleaned.document.bonds, source.bonds);
        }
        source = cleaned.document;
    }
    source.drawing_style.wavy_unknown_alkenes = false;
    let cleaned = clean(&source, Options::default(), &[], 0.)?;
    assert_eq!(cleaned.document.bonds[1].display, "wavy");
    assert!(
        cleaned
            .document
            .bonds
            .iter()
            .all(|bond| bond.order != 1 || bond.display != "wavy")
    );
    Ok(())
}

#[test]
fn existing_unknown_alkene_cleanup_preserves_fixed_atoms_and_bonds() -> anyhow::Result<()> {
    for marker in [0, 1] {
        for wavy in [false, true] {
            let mut source = Document::default();
            source.drawing_style.wavy_unknown_alkenes = wavy;
            let atoms = (0..5)
                .map(|i| source.add_atom("C", crate::document::Point::new(i as f32 * 42., 0.)))
                .collect::<Vec<_>>();
            for (i, pair) in atoms.windows(2).enumerate() {
                source.add_bond(
                    pair[0],
                    pair[1],
                    if i == 1 { 2 } else { 1 },
                    if i == marker { "wavy" } else { "plain" },
                );
            }
            let cleaned = clean(
                &source,
                Options {
                    scope: Scope::SelectedAtoms,
                    ..Default::default()
                },
                &atoms[2..],
                0.,
            )?;
            assert_eq!(cleaned.document.atoms[..2], source.atoms[..2]);
            assert_eq!(cleaned.document.bonds[0], source.bonds[0]);
            assert_eq!(
                cleaned.document.bonds[1].display,
                if wavy { "plain" } else { "wavy" }
            );
            assert_eq!(
                cleaned
                    .document
                    .bonds
                    .iter()
                    .filter(|bond| bond.order == 1 && bond.display == "wavy")
                    .count(),
                usize::from(wavy || marker == 0)
            );
        }
    }
    Ok(())
}

#[test]
fn wavy_alkene_cleanup_falls_back_to_crossed_between_stereocentres() -> anyhow::Result<()> {
    // FC(Cl)C=CC(Br)I drawn collinear: both substituents are unassigned
    // stereocentres, so no single bond may carry the wavy marker.
    let mut source = Document::default();
    source.drawing_style.wavy_unknown_alkenes = true;
    let point = crate::document::Point::new;
    let chain = (0..4)
        .map(|i| source.add_atom("C", point(i as f32 * 42., 0.)))
        .collect::<Vec<_>>();
    for (i, pair) in chain.windows(2).enumerate() {
        source.add_bond(pair[0], pair[1], if i == 1 { 2 } else { 1 }, "plain");
    }
    for (element, at, y) in [
        ("F", 0, -42.),
        ("Cl", 0, 42.),
        ("Br", 3, -42.),
        ("I", 3, 42.),
    ] {
        let x = at as f32 * 42.;
        let atom = source.add_atom(element, point(x, y));
        source.add_bond(chain[at], atom, 1, "plain");
    }
    let cleaned = clean(&source, Options::default(), &[], 0.)?;
    assert!(
        cleaned
            .document
            .bonds
            .iter()
            .all(|bond| bond.order != 1 || bond.display != "wavy")
    );
    assert!(
        cleaned
            .document
            .bonds
            .iter()
            .any(|bond| bond.order == 2 && bond.display == "wavy"),
        "{:?}",
        cleaned.warnings
    );
    Ok(())
}

#[test]
fn partial_alkene_cleanup_keeps_the_wavy_marker_on_a_moving_bond() -> anyhow::Result<()> {
    let mut source = Document::default();
    source.drawing_style.wavy_unknown_alkenes = true;
    let atoms = (0..5)
        .map(|i| source.add_atom("C", crate::document::Point::new(i as f32 * 42., 0.)))
        .collect::<Vec<_>>();
    for (i, pair) in atoms.windows(2).enumerate() {
        source.add_bond(pair[0], pair[1], if i == 1 { 2 } else { 1 }, "plain");
    }
    let cleaned = clean(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            ..Default::default()
        },
        &atoms[2..],
        0.,
    )?;
    assert_eq!(cleaned.document.bonds[0], source.bonds[0]);
    assert!(cleaned.document.bonds.iter().any(|bond| bond.order == 1
        && bond.display == "wavy"
        && (atoms[2..].contains(&bond.a) || atoms[2..].contains(&bond.b))));
    assert!(
        cleaned
            .molecule
            .unwrap()
            .state
            .metadata
            .bonds
            .iter()
            .all(|bond| bond.stereo <= 1)
    );
    Ok(())
}

fn rotate(points: &mut [Point3], angle: f64, center: Point3) {
    let (s, c) = angle.sin_cos();
    for point in points {
        let (x, y) = (point.x - center.x, point.y - center.y);
        point.x = center.x + c * x - s * y;
        point.y = center.y + s * x + c * y;
    }
}

fn centroid(points: &[Point3]) -> Point3 {
    Point3 {
        x: points.iter().map(|point| point.x).sum::<f64>() / points.len() as f64,
        y: points.iter().map(|point| point.y).sum::<f64>() / points.len() as f64,
        z: 0.,
    }
}

fn grid_error(a: Point3, b: Point3) -> f64 {
    let angle = (b.y - a.y).atan2(b.x - a.x);
    let step = PI / 6.;
    (angle - (angle / step).round() * step).abs()
}

fn ring_on_grid(molecule: &Molecule) {
    let ring = molecule
        .state
        .rings
        .atoms
        .iter()
        .find(|ring| ring.len() == 6)
        .unwrap();
    for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        let error = grid_error(molecule.positions[a], molecule.positions[b]);
        assert!(
            error <= 1.5_f64.to_radians() + 1e-7,
            "Ring bond {a}–{b} is {:.6} degrees off the grid",
            error.to_degrees()
        );
    }
}

fn ring_upright(molecule: &Molecule) {
    ring_on_grid(molecule);
    let ring = molecule
        .state
        .rings
        .atoms
        .iter()
        .find(|ring| ring.len() == 6)
        .unwrap();
    for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        let (a, b) = (molecule.positions[a], molecule.positions[b]);
        assert!(
            (6. * (b.y - a.y).atan2(b.x - a.x)).cos() < -0.999,
            "{a:?} {b:?}"
        );
    }
}

fn drawing(text: &str) -> anyhow::Result<Document> {
    let input = smiles::read(text)?;
    let state = input.prepared.state;
    let mut points = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let ring = state
        .rings
        .atoms
        .iter()
        .find(|ring| ring.len() == 6)
        .unwrap();
    let (a, b) = (points[ring[0]], points[ring[1]]);
    let angle = (b.y - a.y).atan2(b.x - a.x);
    let step = PI / 6.;
    let center = centroid(&points);
    rotate(&mut points, (angle / step).round() * step - angle, center);
    // A terminal bond three times the usual length leaves its direction on the
    // grid, but makes cleanup's least-squares orientation introduce a clear tilt.
    let neighbor = state
        .graph
        .bonds
        .iter()
        .find_map(|bond| {
            if bond.a == 0 {
                Some(bond.b)
            } else if bond.b == 0 {
                Some(bond.a)
            } else {
                None
            }
        })
        .unwrap();
    points[0].x = points[neighbor].x + (points[0].x - points[neighbor].x) * 3.;
    points[0].y = points[neighbor].y + (points[0].y - points[neighbor].y) * 3.;
    let molecule = Molecule {
        rdkit_version: crate::chemistry::RDKIT_VERSION,
        ids: (1..=points.len() as u64).collect(),
        positions: points,
        state,
    };
    let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = drawing.labels()?;
    Ok(drawing.finish(labels)?)
}

fn layout(request: &LayoutRequest<'_>) -> Result<Vec<Point3>> {
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
    .map_err(|error| Error::Document(error.to_string()))
}

fn clean(
    document: &Document,
    options: Options,
    selected: &[u64],
    solver_rotation: f64,
) -> Result<Cleaned> {
    prepare(document, options, selected)?.finish_with(|request| {
        let mut points = layout(&request)?;
        if solver_rotation != 0. {
            let center = request
                .fixed
                .first_key_value()
                .map(|(&index, _)| points[index])
                .unwrap_or_else(|| centroid(&points));
            rotate(&mut points, solver_rotation, center);
        }
        Ok(points)
    })
}

fn drawing_at(text: &str, xy: &[(f64, f64)]) -> anyhow::Result<Document> {
    let input = smiles::read(text)?;
    let molecule = Molecule {
        rdkit_version: crate::chemistry::RDKIT_VERSION,
        ids: (1..=xy.len() as u64).collect(),
        positions: xy.iter().map(|&(x, y)| Point3 { x, y, z: 0. }).collect(),
        state: input.prepared.state,
    };
    let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = drawing.labels()?;
    Ok(drawing.finish(labels)?)
}

#[test]
fn cleanup_conventional_orientation_requires_keep_orientation_off() -> anyhow::Result<()> {
    // Molfile coordinates: the acid group points left and its C=O points down.
    let source = drawing_at("CC(=O)O", &[(2., 0.), (1., 0.), (1., -1.), (0., 0.)])?;
    let before = positions(&source);
    let kept = prepare(&source, Options::default(), &[])?.finish(vec![before.clone()])?;
    assert!(Options::default().keep_orientation);
    assert_eq!(kept.document, source);
    let cleaned = prepare(
        &source,
        Options {
            keep_orientation: false,
            ..Default::default()
        },
        &[],
    )?
    .finish(vec![before.clone()])?;
    let after = &cleaned.molecule.as_ref().unwrap().positions;
    assert!(after[3].x > after[0].x, "The acid group must point right");
    assert!(
        after[2].y > after[1].y,
        "C=O must point up in molfile coordinates"
    );
    assert_eq!(centroid(after), centroid(&before));
    assert_eq!(
        identity(cleaned.molecule.as_ref().unwrap())?,
        identity(&molecular::prepare(&source)?)?
    );
    Ok(())
}

#[test]
fn cleanup_conventional_orientation_never_moves_a_fixed_part() -> anyhow::Result<()> {
    let source = drawing_at("CC(=O)O", &[(2., 0.), (1., 0.), (1., -1.), (0., 0.)])?;
    let selected = source.atoms[1..]
        .iter()
        .map(|atom| atom.id)
        .collect::<Vec<_>>();
    let cleaned = prepare(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            keep_orientation: false,
            ..Default::default()
        },
        &selected,
    )?
    .finish(vec![positions(&source)])?;
    // One fixed atom still lets straighten rotate; it must disable the new pass.
    assert_eq!(cleaned.document, source);
    Ok(())
}

#[test]
fn cleanup_reorients_components_without_rearranging_them() -> anyhow::Result<()> {
    let source = drawing_at(
        "[Na+].[O-]C(=O)C",
        &[(-20., 11.), (0., 0.), (1., 0.), (1., -1.), (2., 0.)],
    )?;
    let before = positions(&source);
    let cleaned = prepare(
        &source,
        Options {
            keep_orientation: false,
            ..Default::default()
        },
        &[],
    )?
    .finish(vec![before[..1].to_vec(), before[1..].to_vec()])?;
    assert_eq!(cleaned.document.atoms[0], source.atoms[0]);
    let after = &cleaned.molecule.as_ref().unwrap().positions;
    assert_eq!(centroid(&after[1..]), centroid(&before[1..]));
    assert!(after[1].x > after[4].x);
    assert!(after[3].y > after[2].y);
    Ok(())
}

#[test]
fn cleanup_conventional_reflection_rewedges_visible_stereochemistry() -> anyhow::Result<()> {
    // C=O is already up; putting the acid group right requires reflection in X.
    let source = drawing_at(
        "C[C@H](O)C(=O)O",
        &[(3., 0.), (2., 0.), (2., -1.), (1., 0.), (1., 1.), (0., 0.)],
    )?;
    let cleaned = prepare(
        &source,
        Options {
            keep_orientation: false,
            ..Default::default()
        },
        &[],
    )?
    .finish(vec![positions(&source)])?;
    let after = cleaned.molecule.as_ref().unwrap();
    assert!(after.positions[5].x > after.positions[0].x);
    assert!(after.positions[4].y > after.positions[3].y);
    assert_eq!(identity(after)?, identity(&molecular::prepare(&source)?)?);
    let mut visible = cleaned.document.clone();
    for atom in &mut visible.atoms {
        atom.stereo = None;
    }
    assert_eq!(identity(&molecular::prepare(&visible)?)?, identity(after)?);
    let wedges = source
        .bonds
        .iter()
        .zip(&cleaned.document.bonds)
        .filter(|(before, _)| matches!(before.display.as_str(), "wedge" | "hash"))
        .collect::<Vec<_>>();
    assert!(!wedges.is_empty());
    for (before, after) in wedges {
        assert!(matches!(
            (before.display.as_str(), after.display.as_str()),
            ("wedge", "hash") | ("hash", "wedge")
        ));
    }
    Ok(())
}

#[test]
fn cleanup_iupac_unfused_and_fixed_layouts_match_free_orientation() -> anyhow::Result<()> {
    let source = drawing_at("CC(=O)O", &[(2., 0.), (1., 0.), (1., -1.), (0., 0.)])?;
    for fixed in 0..=2 {
        let selected = source.atoms[fixed..]
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        let options = Options {
            scope: Scope::SelectedAtoms,
            keep_orientation: false,
            ..Default::default()
        };
        let free = prepare(&source, options, &selected)?.finish(vec![positions(&source)])?;
        let iupac = prepare(
            &source,
            Options {
                keep_orientation: true,
                iupac_orientation: true,
                ..options
            },
            &selected,
        )?
        .finish(vec![positions(&source)])?;
        assert_eq!(iupac.document, free.document);
        for atom in &source.atoms[..fixed] {
            assert_eq!(iupac.document.atom(atom.id).unwrap(), atom);
        }
    }
    Ok(())
}

#[test]
fn cleanup_iupac_reflection_rewedges_visible_stereochemistry() -> anyhow::Result<()> {
    // This four-ring system has a unique three-ring row. Its fourth ring must
    // lie above the right end, so its mirror requires an actual reflection.
    let text = "C[C@H](O)c1ccc2c(c1)ccc1cc3ccccc3cc12";
    let input = smiles::read(text)?;
    let state = input.prepared.state;
    let mut points = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        depict::Options {
            use_ring_templates: true,
            ..Default::default()
        },
    )?
    .positions;
    super::super::layout_refinement::iupac_orientation(&state, &mut points);
    let xy = points
        .iter()
        .map(|point| (-point.x, point.y))
        .collect::<Vec<_>>();
    let source = drawing_at(text, &xy)?;
    let before = positions(&source);
    let cleaned = prepare(
        &source,
        Options {
            iupac_orientation: true,
            ..Default::default()
        },
        &[],
    )?
    .finish(vec![before.clone()])?;
    let after = cleaned.molecule.as_ref().unwrap();
    let area = |points: &[Point3]| {
        (points[1].x - points[0].x) * (points[2].y - points[0].y)
            - (points[1].y - points[0].y) * (points[2].x - points[0].x)
    };
    assert!(area(&before) * area(&after.positions) < 0.);
    assert_eq!(identity(after)?, identity(&molecular::prepare(&source)?)?);
    let mut visible = cleaned.document.clone();
    for atom in &mut visible.atoms {
        atom.stereo = None;
    }
    assert_eq!(identity(&molecular::prepare(&visible)?)?, identity(after)?);
    assert!(
        source
            .bonds
            .iter()
            .zip(&cleaned.document.bonds)
            .any(|(before, after)| {
                matches!(
                    (before.display.as_str(), after.display.as_str()),
                    ("wedge", "hash") | ("hash", "wedge")
                )
            })
    );
    Ok(())
}

#[test]
fn keeping_orientation_stands_substituted_six_rings_upright() -> anyhow::Result<()> {
    for text in [PROPYLBENZENE, ASPIRIN] {
        let source = drawing(text)?;
        let before = molecular::prepare(&source)?;
        ring_on_grid(&before);
        if text == PROPYLBENZENE {
            let prepared = prepare(&source, Options::default(), &[])?;
            let request = prepared.requests().next().unwrap();
            let mut points = layout(&request)?;
            numeric::orient(
                &request.molecule.positions,
                &mut points,
                request.fixed,
                true,
            )?;
            let ring = &before.state.rings.atoms[0];
            assert!(
                ring.iter()
                    .zip(ring.iter().cycle().skip(1))
                    .any(|(&a, &b)| grid_error(points[a], points[b]) > 1.5_f64.to_radians()),
                "The solver's best-fit orientation must reproduce the cleanup tilt"
            );
        }
        let cleaned = clean(&source, Options::default(), &[], 0.)?;
        let after = cleaned.molecule.as_ref().unwrap();
        ring_upright(after);
        let ring = &before.state.rings.atoms[0];
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let direction =
                |points: &[Point3]| (points[b].y - points[a].y).atan2(points[b].x - points[a].x);
            let angle = direction(&after.positions) - direction(&before.positions);
            let error = angle.sin().atan2(angle.cos()).abs();
            assert!(
                error <= 30_f64.to_radians() + 1e-6,
                "Ring bond {a}–{b} turned by {:.6} degrees",
                error.to_degrees()
            );
        }
        let (before, after) = (centroid(&before.positions), centroid(&after.positions));
        assert!((before.x - after.x).hypot(before.y - after.y) < 1e-10);
        assert!(cleaned.warnings.is_empty());
    }
    Ok(())
}

#[test]
fn cleanup_without_keep_orientation_straightens_a_rotated_solver_layout() -> anyhow::Result<()> {
    let source = drawing(ASPIRIN)?;
    let cleaned = clean(
        &source,
        Options {
            keep_orientation: false,
            ..Default::default()
        },
        &[],
        7_f64.to_radians(),
    )?;
    ring_on_grid(cleaned.molecule.as_ref().unwrap());
    Ok(())
}

#[test]
fn cleanup_with_keep_orientation_stands_a_tilted_ring_upright() -> anyhow::Result<()> {
    for text in [PROPYLBENZENE, ASPIRIN] {
        let source = drawing(text)?;
        // The ring turns at most 30 degrees until it stands upright.
        let ring = molecular::prepare(&source)?.state.rings.atoms.remove(0);
        let (a, b) = (ring[0], ring[1]);
        let direction = |molecule: &Molecule| {
            let (a, b) = (molecule.positions[a], molecule.positions[b]);
            (b.y - a.y).atan2(b.x - a.x)
        };
        for tilt in [7_f32, -9., 20.] {
            let mut source = source.clone();
            let (s, c) = tilt.to_radians().sin_cos();
            for atom in &mut source.atoms {
                let point = atom.position;
                atom.position.x = c * point.x - s * point.y;
                atom.position.y = s * point.x + c * point.y;
            }
            let before = molecular::prepare(&source)?;
            let cleaned = clean(&source, Options::default(), &[], 0.)?;
            let after = cleaned.molecule.as_ref().unwrap();
            ring_upright(after);
            let angle = direction(after) - direction(&before);
            let turn = angle.sin().atan2(angle.cos()).abs();
            assert!(
                turn <= 30_f64.to_radians() + 1e-6,
                "{text} tilted by {tilt} degrees turned by {:.6} degrees",
                turn.to_degrees()
            );
        }
    }
    Ok(())
}

#[test]
fn selected_cleanup_straightens_about_its_single_fixed_atom() -> anyhow::Result<()> {
    let source = drawing(PROPYLBENZENE)?;
    let fixed = &source.atoms[0];
    let selected = source
        .atoms
        .iter()
        .skip(1)
        .map(|atom| atom.id)
        .collect::<Vec<_>>();
    let cleaned = clean(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            ..Default::default()
        },
        &selected,
        0.,
    )?;
    assert_eq!(
        cleaned.document.atom(fixed.id).unwrap().position,
        fixed.position
    );
    ring_on_grid(cleaned.molecule.as_ref().unwrap());
    Ok(())
}

#[test]
fn selected_cleanup_keeps_two_fixed_atoms_at_their_original_angle() -> anyhow::Result<()> {
    let mut source = drawing(PROPYLBENZENE)?;
    let molecule = molecular::prepare(&source)?;
    let ring = &molecule.state.rings.atoms[0];
    let fixed = [molecule.ids[ring[0]], molecule.ids[ring[1]]];
    let angle = 7_f32.to_radians();
    let (s, c) = angle.sin_cos();
    for atom in &mut source.atoms {
        let point = atom.position;
        atom.position.x = c * point.x - s * point.y;
        atom.position.y = s * point.x + c * point.y;
    }
    let selected = source
        .atoms
        .iter()
        .filter(|atom| !fixed.contains(&atom.id))
        .map(|atom| atom.id)
        .collect::<Vec<_>>();
    let cleaned = clean(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            ..Default::default()
        },
        &selected,
        0.,
    )?;
    for id in fixed {
        assert_eq!(
            cleaned.document.atom(id).unwrap().position,
            source.atom(id).unwrap().position
        );
    }
    let after = cleaned.molecule.as_ref().unwrap();
    assert!(grid_error(after.positions[ring[0]], after.positions[ring[1]]) > 6_f64.to_radians());
    Ok(())
}

/// Issue #1181: the solver can close a ring on the wrong side of the two
/// fixed atoms that hold it, on top of other fixed atoms.
#[test]
fn selected_cleanup_reflects_atoms_folded_onto_fixed_atoms() -> anyhow::Result<()> {
    let hexagon: Vec<_> = (0..6)
        .map(|i| (f64::from(i) * PI / 3.).sin_cos())
        .map(|(sin, cos)| (1.5 * cos, 1.5 * sin))
        .collect();
    let source = drawing_at("c1ccccc1", &hexagon)?;
    let selected = [source.atoms[0].id, source.atoms[1].id];
    let cleaned = prepare(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            ..Default::default()
        },
        &selected,
    )?
    .finish_with(|request| {
        // Atoms 0 and 1 hang on fixed atoms 5 and 2; fold them onto 4 and 3.
        let mut points = request.molecule.positions.clone();
        points[0] = points[4];
        points[1] = points[3];
        Ok::<_, Error>(points)
    })?;
    assert!(cleaned.warnings.is_empty(), "{:?}", cleaned.warnings);
    for (before, after) in source.atoms.iter().zip(&cleaned.document.atoms) {
        assert!(
            before.position.distance(after.position) < 1e-3,
            "atom {} moved from {:?} to {:?}",
            before.id,
            before.position,
            after.position
        );
    }
    Ok(())
}

#[test]
fn selected_cleanup_leaves_atoms_it_cannot_unfold_with_a_warning() -> anyhow::Result<()> {
    let source = drawing(PROPYLBENZENE)?;
    // The methyl carbon hangs on one fixed atom, so no reflection is defined.
    let cleaned = prepare(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            ..Default::default()
        },
        &[source.atoms[0].id],
    )?
    .finish_with(|request| {
        let mut points = request.molecule.positions.clone();
        points[0] = points[2];
        Ok::<_, Error>(points)
    })?;
    assert_eq!(cleaned.document.atoms, source.atoms);
    assert_eq!(cleaned.document.bonds, source.bonds);
    assert_eq!(
        cleaned.warnings,
        [
            "Selected atoms in 1 molecule(s) kept unchanged: cleaned up, they would lie on other atoms. Choose Selected molecules."
        ]
    );
    Ok(())
}

/// Issue #1181: a group turned back to its free side can land on selected
/// atoms outside the group, which are not fixed.
#[test]
fn selected_cleanup_never_reflects_atoms_onto_other_selected_atoms() -> anyhow::Result<()> {
    let mut points = vec![(3., 0.)];
    points.extend(
        (0..6)
            .map(|i| (f64::from(i) * PI / 3.).sin_cos())
            .map(|(sin, cos)| (1.5 * cos, 1.5 * sin)),
    );
    let source = drawing_at("Cc1ccccc1", &points)?;
    let selected = [0, 2, 3].map(|index| source.atoms[index].id);
    let cleaned = prepare(
        &source,
        Options {
            scope: Scope::SelectedAtoms,
            ..Default::default()
        },
        &selected,
    )?
    .finish_with(|request| {
        // Atoms 2 and 3 hang on fixed atoms 1 and 4; fold them onto 6 and 5.
        // The methyl carbon sits where the reflection puts atom 2 back.
        let mut points = request.molecule.positions.clone();
        points[0] = points[2];
        points[2] = points[6];
        points[3] = points[5];
        Ok::<_, Error>(points)
    })?;
    assert_eq!(cleaned.document.atoms, source.atoms);
    assert_eq!(cleaned.document.bonds, source.bonds);
    assert_eq!(
        cleaned.warnings,
        [
            "Selected atoms in 1 molecule(s) kept unchanged: cleaned up, they would lie on other atoms. Choose Selected molecules."
        ]
    );
    Ok(())
}

/// A finite but huge coordinate fills the integer grid cell; its neighbour
/// lookup must not overflow.
#[test]
fn unfold_accepts_huge_coordinates() -> anyhow::Result<()> {
    let source = drawing(PROPYLBENZENE)?;
    for selection in [vec![], vec![source.atoms[0].id]] {
        let scope = if selection.is_empty() {
            Scope::Drawing
        } else {
            Scope::SelectedAtoms
        };
        let prepared = prepare(
            &source,
            Options {
                scope,
                ..Default::default()
            },
            &selection,
        )?;
        let part = &prepared.parts[0];
        let huge = Point3 {
            x: 1e20,
            y: 1e20,
            z: 0.,
        };
        let mut layout = vec![huge; source.atoms.len()];
        unfold(&part.molecule, &mut layout, &part.fixed, 1.)?;
    }
    Ok(())
}

/// Stacked ineligible atoms must not make proximity searches quadratic.
#[test]
fn near_skips_ineligible_sources() -> anyhow::Result<()> {
    let layout = vec![
        Point3 {
            x: 0.,
            y: 0.,
            z: 0.,
        };
        2_000
    ];
    let calls = std::cell::Cell::new(0);
    let counts = |_, other| {
        calls.set(calls.get() + 1);
        other == layout.len() - 1
    };
    let flags = near(&layout, 0.5, 0.1, &|_| false, &counts);
    assert_eq!(calls.get(), 0);
    assert_eq!(flags, vec![false; layout.len()]);

    let flags = near(&layout, 0.5, 0.1, &|index| index == 0, &counts);
    assert!(calls.get() <= layout.len());
    let mut expected = vec![false; layout.len()];
    expected[0] = true;
    assert_eq!(flags, expected);
    Ok(())
}

/// Issue #1121: the solver's layout can be the mirror image of the drawing.
#[test]
fn cleanup_with_keep_orientation_keeps_the_handedness_of_the_drawing() -> anyhow::Result<()> {
    let text = "O=C1Cc2ccccc2C12CCC(=O)c1ccccc12";
    let state = smiles::read(text)?.prepared.state;
    let points = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let ring = &state.rings.atoms[0];
    let area = |points: &[Point3]| -> f64 {
        ring.iter()
            .zip(ring.iter().cycle().skip(1))
            .map(|(&a, &b)| points[a].x * points[b].y - points[b].x * points[a].y)
            .sum()
    };
    for mirror in [1., -1.] {
        let xy: Vec<_> = points.iter().map(|p| (p.x * mirror, p.y)).collect();
        let source = drawing_at(text, &xy)?;
        let before = area(&molecular::prepare(&source)?.positions);
        let cleaned = clean(&source, Options::default(), &[], 0.7)?;
        let after = area(&cleaned.molecule.as_ref().unwrap().positions);
        assert!(
            before * after > 0.,
            "mirror {mirror}: ring area {before} became {after}"
        );
    }
    Ok(())
}

/// A fresh depiction of `text`, scaled from the 1.5 bond length.
fn scaled(text: &str, scale: f64) -> anyhow::Result<Document> {
    let state = smiles::read(text)?.prepared.state;
    let points = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let xy: Vec<_> = points.iter().map(|p| (p.x * scale, p.y * scale)).collect();
    drawing_at(text, &xy)
}

/// Regular hexagons with vertices at `phase + 60k` degrees, one per center.
fn hexagons(text: &str, rings: &[((f64, f64), f64, f64)]) -> anyhow::Result<Document> {
    let mut xy = vec![];
    for &((x, y), phase, length) in rings {
        for k in 0..6 {
            let angle = (phase + 60. * f64::from(k)).to_radians();
            xy.push((x + length * angle.cos(), y + length * angle.sin()));
        }
    }
    drawing_at(text, &xy)
}

fn bond_angles(document: &Document) -> Vec<(f32, f64)> {
    document
        .bonds
        .iter()
        .map(|bond| {
            let a = document.atom(bond.a).unwrap().position;
            let b = document.atom(bond.b).unwrap().position;
            (
                a.distance(b),
                f64::from(b.y - a.y).atan2(f64::from(b.x - a.x)),
            )
        })
        .collect()
}

fn median_length(document: &Document) -> f32 {
    let mut lengths: Vec<_> = bond_angles(document).into_iter().map(|(l, _)| l).collect();
    lengths.sort_by(f32::total_cmp);
    let half = lengths.len() / 2;
    if lengths.len() % 2 == 0 {
        (lengths[half - 1] + lengths[half]) / 2.
    } else {
        lengths[half]
    }
}

#[test]
fn cleanup_keeps_the_drawn_size_including_templated_rings() -> anyhow::Result<()> {
    // Cyclododecane and norbornane are drawn from ring templates, which ignore
    // the requested bond length.
    for text in [
        "c1ccccc1CCO",
        "C1CCCCCCCCCCC1",
        "C1CC2CCC1C2",
        "C1C2CC3CC1CC(C2)C3",
    ] {
        for scale in [0.7, 1., 1.5] {
            let source = scaled(text, scale)?;
            for orientation in [Orientation::Keep, Orientation::Free, Orientation::Iupac] {
                let mut options = Options::default();
                options.set_orientation(orientation);
                let cleaned = clean(&source, options, &[], 0.)?.document;
                let (before, after) = (median_length(&source), median_length(&cleaned));
                assert!(
                    (after / before - 1.).abs() < 0.005,
                    "{text} at {scale}x, {orientation}: {before} -> {after}"
                );
            }
            // One fixed atom: the layout scales about it.
            let selected: Vec<_> = source.atoms.iter().skip(1).map(|a| a.id).collect();
            let options = Options {
                scope: Scope::SelectedAtoms,
                ..Options::default()
            };
            let cleaned = clean(&source, options, &selected, 0.)?.document;
            let (before, after) = (median_length(&source), median_length(&cleaned));
            assert!(
                (after / before - 1.).abs() < 0.005,
                "{text} at {scale}x, one fixed atom: {before} -> {after}"
            );
            assert_eq!(cleaned.atoms[0].position, source.atoms[0].position);
        }
    }
    // An even count takes the middle pair: bonds of 42 and 84 give 63.
    let propane = drawing_at("CCC", &[(0., 0.), (1.5, 0.), (4.5, 0.)])?;
    let cleaned = clean(&propane, Options::default(), &[], 0.)?.document;
    for (length, _) in bond_angles(&cleaned) {
        assert!((length - 63.).abs() < 0.01, "{length}");
    }
    Ok(())
}

#[test]
fn keeping_orientation_turns_six_rings_upright_by_at_most_30_degrees() -> anyhow::Result<()> {
    // Flat (0), tilted towards flat (10, -10) and upright (30) benzene.
    for phase in [0., 10., -10., 25., 30.] {
        let source = hexagons("c1ccccc1", &[((0., 0.), phase, 1.5)])?;
        let cleaned = clean(&source, Options::default(), &[], 0.)?.document;
        for ((_, before), (_, after)) in bond_angles(&source).into_iter().zip(bond_angles(&cleaned))
        {
            assert!(
                (6. * after).cos() < -0.9999,
                "phase {phase}: a side at {} degrees is not upright",
                after.to_degrees()
            );
            let turn = (after - before + PI).rem_euclid(2. * PI) - PI;
            assert!(turn.abs() <= PI / 6. + 1e-6, "phase {phase}: turned {turn}");
        }
    }
    // A flat naphthalene ties; it turns so the fusion bond stands vertical.
    for turn in [30_f64, -30.] {
        let mut source = scaled("c1ccc2ccccc2c1", 1.)?;
        let degree = |id: u64| {
            source
                .bonds
                .iter()
                .filter(|b| b.a == id || b.b == id)
                .count()
        };
        let fusion = source
            .bonds
            .iter()
            .find(|bond| degree(bond.a) == 3 && degree(bond.b) == 3)
            .map(|bond| (bond.a, bond.b))
            .unwrap();
        let (a, b) = (
            source.atom(fusion.0).unwrap().position,
            source.atom(fusion.1).unwrap().position,
        );
        let angle = f64::from(b.y - a.y).atan2(f64::from(b.x - a.x));
        let (sin, cos) = (PI / 2. - angle + turn.to_radians()).sin_cos();
        for atom in &mut source.atoms {
            let (x, y) = (f64::from(atom.position.x), f64::from(atom.position.y));
            atom.position.x = (cos * x - sin * y) as f32;
            atom.position.y = (sin * x + cos * y) as f32;
        }
        let cleaned = clean(&source, Options::default(), &[], 0.)?.document;
        let (a, b) = (
            cleaned.atom(fusion.0).unwrap().position,
            cleaned.atom(fusion.1).unwrap().position,
        );
        assert!((a.x - b.x).abs() < 1e-3, "{turn}: {a:?} {b:?}");
    }
    Ok(())
}

#[test]
fn cleanup_without_a_selection_cleans_each_component_on_its_own() -> anyhow::Result<()> {
    // A salt-like pair of rings, tilted opposite ways and drawn at two sizes.
    let source = hexagons(
        "c1ccccc1.C1CCCCC1",
        &[((0., 0.), 10., 1.5), ((8., 0.), -12., 2.25)],
    )?;
    let cleaned = clean(&source, Options::default(), &[], 0.)?.document;
    let angles = bond_angles(&cleaned);
    for (length, angle) in &angles {
        assert!((6. * angle).cos() < -0.9999, "{}", angle.to_degrees());
        assert!(
            [1.5_f32, 2.25]
                .iter()
                .any(|l| (length - l * 28.).abs() < 0.05),
            "{length}"
        );
    }
    // Each molecule keeps its center.
    for (range, x) in [(0..6, 0.), (6..12, 8. * 28.)] {
        let center = cleaned.atoms[range]
            .iter()
            .map(|a| a.position.x)
            .sum::<f32>()
            / 6.;
        assert!((center - x).abs() < 0.01, "{center}");
    }
    Ok(())
}

#[test]
fn straightening_a_side_chain_never_tilts_upright_rings() -> anyhow::Result<()> {
    // Its cyclopentane and chain favour another grid than the benzene ring.
    let source = scaled("c1ccccc1C2CCCC2CCCCCCCC", 1.)?;
    for orientation in [Orientation::Keep, Orientation::Free] {
        let mut options = Options::default();
        options.set_orientation(orientation);
        let cleaned = clean(&source, options, &[], 0.)?;
        ring_upright(cleaned.molecule.as_ref().unwrap());
    }
    Ok(())
}
