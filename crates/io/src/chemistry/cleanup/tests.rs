use super::*;
use crate::chemistry::{depict, smiles};
use std::f64::consts::PI;

const PROPYLBENZENE: &str = "CCCc1ccccc1";
const ASPIRIN: &str = "CC(=O)Oc1ccccc1C(=O)O";

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
