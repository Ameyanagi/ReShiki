//! ReShiki's import template retry (#538, #1114) stays on in RDKit-parity
//! comparisons, like the #632 expansion trial. RDKit 2026.09.1's new placement
//! (#9650, #9378) can leave two atoms of a bridged molecule closer than half a
//! bond, which trips the retry where 2026.03.6 kept them apart: the frozen
//! worker keeps RDKit's default layout and ReShiki draws its ring-template
//! layout. See "Deliberate differences" in reference/README.md.
use reshiki::{chemistry, document::Document};
use std::collections::{BTreeSet, HashMap};

/// The retry's clearance: half of RDKit's 1.5 bond at the worker's scale 28.
const CLEARANCE: f32 = 0.75 * 28.0;

/// Whether a failed comparison is only the retry's redrawing: RDKit's drawing
/// has a participant that is bridged (two rings share at least three atoms)
/// with a non-bonded pair closer than half a bond, ReShiki drew that
/// participant differently, and every difference is one of its positions, an
/// atom x position the row assembly derives from its width, or an arrow end x.
pub fn redrawn(actual: &Document, expected: &Document, failure: &str) -> bool {
    let ids = triggered(expected);
    let redrawn: BTreeSet<usize> = expected
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, atom)| ids.contains(&atom.id))
        .map(|(i, _)| i)
        .collect();
    let distance = |document: &Document, a: usize, b: usize| {
        let (p, q) = (document.atoms[a].position, document.atoms[b].position);
        (p.x - q.x).hypot(p.y - q.y)
    };
    let changed = actual.atoms.len() == expected.atoms.len()
        && redrawn.iter().any(|&a| {
            redrawn
                .iter()
                .any(|&b| (distance(actual, a, b) - distance(expected, a, b)).abs() > 1.)
        });
    let geometry = |line: &str| {
        let Some(path) = line
            .strip_prefix("[response-field] /document/")
            .and_then(|rest| rest.split(':').next())
        else {
            return false;
        };
        match path.split('/').collect::<Vec<_>>().as_slice() {
            ["atoms", i, "position", axis] => {
                *axis == "x" || i.parse().is_ok_and(|i: usize| redrawn.contains(&i))
            }
            ["arrows", _, "start" | "end", "x"] => true,
            _ => false,
        }
    };
    let mut lines = failure.lines();
    changed && lines.next() == Some("Engine response mismatch") && lines.all(geometry)
}

/// Atom ids of the participants whose drawing trips the retry.
fn triggered(document: &Document) -> BTreeSet<u64> {
    let mut molecules = document.clone();
    molecules.reactions.clear();
    molecules.arrows.clear();
    let Ok(molecule) = chemistry::document::prepare(&molecules) else {
        return BTreeSet::new();
    };
    let participants: Vec<Vec<u64>> = match document.reactions.as_slice() {
        [] => vec![document.atoms.iter().map(|a| a.id).collect()],
        reactions => reactions
            .iter()
            .flat_map(|r| r.reactants.iter().chain(&r.products).chain(&r.agents))
            .map(|p| p.atoms.clone())
            .collect(),
    };
    let rings: Vec<BTreeSet<u64>> = molecule
        .state
        .rings
        .atoms
        .iter()
        .map(|ring| {
            ring.iter()
                .filter_map(|&i| molecule.ids.get(i).copied())
                .collect()
        })
        .collect();
    let position: HashMap<u64, _> = document.atoms.iter().map(|a| (a.id, a.position)).collect();
    let bonded: BTreeSet<(u64, u64)> = document
        .bonds
        .iter()
        .map(|b| (b.a.min(b.b), b.a.max(b.b)))
        .collect();
    let mut result = BTreeSet::new();
    for atoms in participants {
        let own: Vec<_> = rings
            .iter()
            .filter(|r| r.iter().all(|id| atoms.contains(id)))
            .collect();
        let bridged = own
            .iter()
            .enumerate()
            .any(|(i, a)| own[i + 1..].iter().any(|b| a.intersection(b).count() >= 3));
        let crowded = atoms.iter().enumerate().any(|(i, &a)| {
            atoms[i + 1..].iter().any(|&b| {
                !bonded.contains(&(a.min(b), a.max(b)))
                    && position
                        .get(&a)
                        .zip(position.get(&b))
                        .is_some_and(|(p, q)| (p.x - q.x).hypot(p.y - q.y) < CLEARANCE)
            })
        });
        if bridged && crowded {
            result.extend(atoms);
        }
    }
    result
}
