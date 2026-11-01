use super::*;
use crate::chemistry::{depict, smiles};
use std::f64::consts::PI;

const PROPYLBENZENE: &str = "CCCc1ccccc1";
const ASPIRIN: &str = "CC(=O)Oc1ccccc1C(=O)O";

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

#[test]
fn keeping_orientation_leaves_substituted_six_rings_on_the_grid() -> anyhow::Result<()> {
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
        ring_on_grid(after);
        let ring = &before.state.rings.atoms[0];
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let direction =
                |points: &[Point3]| (points[b].y - points[a].y).atan2(points[b].x - points[a].x);
            let angle = direction(&after.positions) - direction(&before.positions);
            let error = angle.sin().atan2(angle.cos()).abs();
            assert!(
                error <= 1.5_f64.to_radians(),
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
fn cleanup_with_keep_orientation_straightens_a_tilted_drawing() -> anyhow::Result<()> {
    for text in [PROPYLBENZENE, ASPIRIN] {
        let source = drawing(text)?;
        // A ring that turns at most 15 degrees onto the grid from a 20-degree
        // tilt lands on the 30-degree line, not back on 0.
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
            ring_on_grid(after);
            let angle = direction(after) - direction(&before);
            let turn = angle.sin().atan2(angle.cos()).abs();
            assert!(
                turn <= 15_f64.to_radians(),
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
