use super::*;
use crate::chemistry::smiles;

const SPHOS: &str = "COc1cccc(OC)c1-c1ccccc1P(C1CCCCC1)C1CCCCC1";
const XPHOS: &str = "CC(C)c1cc(C(C)C)c(-c2ccccc2P(C2CCCCC2)C2CCCCC2)c(C(C)C)c1";

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
