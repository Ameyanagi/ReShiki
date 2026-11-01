use super::*;
use crate::chemistry::smiles;

const SPHOS: &str = "COc1cccc(OC)c1-c1ccccc1P(C1CCCCC1)C1CCCCC1";
const XPHOS: &str = "CC(C)c1cc(C(C)C)c(-c2ccccc2P(C2CCCCC2)C2CCCCC2)c(C(C)C)c1";

#[test]
fn generated_unknown_alkene_depictions_roundtrip_mol_and_cdxml() -> anyhow::Result<()> {
    for (input, known) in [("CC=CCC", 0), ("C/C=C/C=CC", 1)] {
        for wavy in [false, true] {
            let imported = smiles::read(input)?;
            let mut molecule = molecule(imported.prepared.state)?;
            layout(&mut molecule, &imported.reaction_properties)?;
            // SMILES cannot say "unknown"; mark the unspecified double as such.
            let (graph, metadata) = (&molecule.state.graph, &mut molecule.state.metadata);
            for (bond, meta) in graph.bonds.iter().zip(&mut metadata.bonds) {
                if bond.order == 2 && meta.stereo == 0 {
                    meta.stereo = 1;
                }
            }
            let options = molecular::DrawingOptions {
                wavy_unknown_alkenes: wavy,
                ..Default::default()
            };
            let drawing =
                molecular::for_generated(&molecule, &imported.prepared.dummy_labels, options)?;
            let labels = drawing.labels()?;
            let document = drawing.finish(labels)?;
            assert_eq!(
                document
                    .bonds
                    .iter()
                    .filter(|b| b.order == 1 && b.display == "wavy")
                    .count(),
                usize::from(wavy)
            );
            assert_eq!(
                document
                    .bonds
                    .iter()
                    .filter(|b| b.order == 2 && b.display == "wavy")
                    .count(),
                usize::from(!wavy)
            );
            let molecule = molecular::prepare(&document)?;
            assert_eq!(
                molecule
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo == 1)
                    .count(),
                1
            );
            assert_eq!(
                molecule
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo > 1)
                    .count(),
                known
            );
            for force_v3000 in [false, true] {
                let (text, fallback) = molfile::write_drawing_absolute(
                    &molecule,
                    &document,
                    molfile::Options { force_v3000 },
                )?;
                assert!(!fallback);
                let imported = molfile::read(&text)?;
                assert_eq!(
                    imported
                        .molecule
                        .state
                        .metadata
                        .bonds
                        .iter()
                        .filter(|b| b.stereo == 1)
                        .count(),
                    1,
                    "input={input}, wavy={wavy}, v3000={force_v3000}: {text}"
                );
                assert_eq!(
                    imported
                        .molecule
                        .state
                        .metadata
                        .bonds
                        .iter()
                        .filter(|b| b.stereo > 1)
                        .count(),
                    known
                );
            }
            let xml = crate::exchange::drawing::write(&document, Default::default())?;
            let imported = cdxml::import_cdxml(&xml)?;
            assert_eq!(
                molecular::prepare(&imported.document)?
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo == 1)
                    .count(),
                1
            );
        }
    }
    Ok(())
}

const MOLECULES: &str = "\
ibuprofen CC(C)Cc1ccc(cc1)C(C)C(=O)O
aspirin CC(=O)Oc1ccccc1C(=O)O
paracetamol CC(=O)Nc1ccc(O)cc1
caffeine Cn1cnc2c1c(=O)n(C)c(=O)n2C
toluene Cc1ccccc1
benzoic_acid OC(=O)c1ccccc1
ethyl_cinnamate CCOC(=O)/C=C/c1ccccc1
stearic_acid CCCCCCCCCCCCCCCCCC(=O)O
naproxen COc1ccc2cc(ccc2c1)[C@H](C)C(=O)O
biphenyl c1ccc(cc1)-c1ccccc1
tyrosine N[C@@H](Cc1ccc(O)cc1)C(=O)O
menthol CC(C)[C@@H]1CC[C@@H](C)C[C@H]1O
cholesterol C[C@H](CCCC(C)C)[C@H]1CC[C@@H]2[C@@]1(CC[C@H]3[C@H]2CC=C4[C@@]3(CC[C@@H](C4)O)C)C
diphenhydramine CN(C)CCOC(c1ccccc1)c1ccccc1
lidocaine CCN(CC)CC(=O)Nc1c(C)cccc1C
glucose OC[C@H]1OC(O)[C@H](O)[C@@H](O)[C@@H]1O";

fn grid_deviations(state: &State, points: &[Point3]) -> Vec<f64> {
    state
        .graph
        .bonds
        .iter()
        .filter(|bond| {
            !state.rings.atoms.iter().any(|ring| {
                ring.len() != 6
                    && ring
                        .iter()
                        .zip(ring.iter().cycle().skip(1))
                        .take(ring.len())
                        .any(|(&a, &b)| {
                            (a == bond.a && b == bond.b) || (a == bond.b && b == bond.a)
                        })
            })
        })
        .map(|bond| {
            let (a, b) = (points[bond.a], points[bond.b]);
            let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees().rem_euclid(30.);
            angle.min(30. - angle)
        })
        .collect()
}

pub(super) fn assert_rigid_rotation(before: &[Point3], after: &[Point3]) {
    assert_eq!(before.len(), after.len());
    let center = Point3 {
        x: before.iter().map(|p| p.x).sum::<f64>() / before.len() as f64,
        y: before.iter().map(|p| p.y).sum::<f64>() / before.len() as f64,
        z: 0.,
    };
    let (&a, &b) = before
        .iter()
        .zip(after)
        .find(|(a, _)| (a.x - center.x).hypot(a.y - center.y) > 1e-6)
        .unwrap();
    let (ax, ay, bx, by) = (
        a.x - center.x,
        a.y - center.y,
        b.x - center.x,
        b.y - center.y,
    );
    let angle = (ax * by - ay * bx).atan2(ax * bx + ay * by);
    assert!(angle.abs() <= 15_f64.to_radians() + 1e-9);
    let (sin, cos) = angle.sin_cos();
    for (a, b) in before.iter().zip(after) {
        let (x, y) = (a.x - center.x, a.y - center.y);
        let (x, y) = (center.x + cos * x - sin * y, center.y + sin * x + cos * y);
        assert!((x - b.x).hypot(y - b.y) < 1e-10);
        assert_eq!(a.z, b.z);
    }
}

#[test]
fn common_native_imports_straighten_six_rings_and_chains() -> anyhow::Result<()> {
    for entry in MOLECULES.lines() {
        let (name, text) = entry.split_once(' ').unwrap();
        let Preparation::Complete(prepared) = prepare(Request::import("smiles", text))? else {
            panic!("{name}: import deferred");
        };
        let molecule = prepared.molecule;
        let deviations = grid_deviations(&molecule.state, &molecule.positions);
        let fitted = deviations
            .iter()
            .filter(|&&angle| angle <= 1.5 + 1e-7)
            .count();
        // Cholesterol's fused rings leave some bonds off the grid; the
        // rotation still brings most of them onto it.
        let expected = match name {
            "cholesterol" => deviations.len() / 2 + 1,
            // One caffeine bond is distorted by its fused five-ring geometry.
            "caffeine" => deviations.len() - 1,
            _ => deviations.len(),
        };
        assert!(
            fitted >= expected,
            "{name}: {fitted}/{} on grid: {deviations:?}",
            deviations.len()
        );
        if name == "paracetamol" {
            let ring = molecule
                .state
                .rings
                .atoms
                .iter()
                .find(|ring| ring.len() == 6)
                .unwrap();
            let vertical = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .take(6)
                .filter(|&(&a, &b)| {
                    (molecule.positions[a].x - molecule.positions[b].x).abs() < 1e-7
                })
                .count();
            assert_eq!(vertical, 2, "paracetamol's ring sides must be vertical");
        }
    }
    Ok(())
}

#[test]
fn disconnected_imports_turn_as_one_body() -> anyhow::Result<()> {
    // Rotating each component alone would make these two overlap.
    let input = smiles::read("CCOC(=O)/C=C/c1ccccc1.CCCC(CCCC)CCCC")?;
    let state = input.prepared.state;
    let original = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let positions = coordinates(&state, &input.reaction_properties)?.positions;
    assert_ne!(positions, original);
    assert_rigid_rotation(&original, &positions);
    Ok(())
}

fn cross(o: &Point3, a: &Point3, b: &Point3) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// No non-bonded atom pair is inside the depictor's 0.7-unit collision
/// threshold (bonds are 1.5 units), and no bond outside a ring crosses that
/// ring's edges or ends inside its polygon.
fn assert_separated(text: &str, state: &State, p: &[Point3]) {
    let bonded = |a: usize, b: usize| {
        state
            .graph
            .bonds
            .iter()
            .any(|bond| (bond.a, bond.b) == (a, b) || (bond.a, bond.b) == (b, a))
    };
    for a in 0..p.len() {
        for b in a + 1..p.len() {
            let d = (p[a].x - p[b].x).hypot(p[a].y - p[b].y);
            assert!(
                bonded(a, b) || d >= 0.7,
                "{text}: atoms {a} and {b} are {d:.3} apart"
            );
        }
    }
    for ring in &state.rings.atoms {
        let edges = (0..ring.len()).map(|i| (ring[i], ring[(i + 1) % ring.len()]));
        for bond in &state.graph.bonds {
            let (a, b) = (bond.a, bond.b);
            if ring.contains(&a) && ring.contains(&b) {
                continue;
            }
            for (c, d) in edges.clone() {
                if [c, d].contains(&a) || [c, d].contains(&b) {
                    continue;
                }
                let crosses = cross(&p[a], &p[b], &p[c]) * cross(&p[a], &p[b], &p[d]) < 0.0
                    && cross(&p[c], &p[d], &p[a]) * cross(&p[c], &p[d], &p[b]) < 0.0;
                assert!(!crosses, "{text}: bond {a}-{b} crosses ring edge {c}-{d}");
            }
            for end in [a, b].into_iter().filter(|end| !ring.contains(end)) {
                let mut sides = edges
                    .clone()
                    .map(|(c, d)| cross(&p[c], &p[d], &p[end]) > 0.0);
                let first = sides.next();
                let inside = sides.all(|side| Some(side) == first);
                assert!(!inside, "{text}: atom {end} lies inside ring {ring:?}");
            }
        }
    }
}

#[test]
fn buchwald_ligands_lay_out_without_ring_overlap() -> anyhow::Result<()> {
    for text in [SPHOS, XPHOS] {
        let input = smiles::read(text)?;
        let state = &input.prepared.state;
        assert_separated(
            text,
            state,
            &coordinates(state, &input.reaction_properties)?.positions,
        );
        // Clean up computes with ring templates.
        let cleaned = depict::compute(
            state,
            &vec![None; state.graph.atoms.len()],
            None,
            depict::Options {
                use_ring_templates: true,
                ..Default::default()
            },
        )?;
        assert_separated(text, state, &cleaned.positions);
    }
    Ok(())
}
