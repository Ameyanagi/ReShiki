//! RDKit 2026.09 takes its SSSR basis from RingDecomposerLib, even with legacy
//! symmetrization; ReShiki keeps its Figueras basis. See "Deliberate
//! differences" in reference/README.md.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Ring lists hold the basis first, then the other symmetric rings. The basis
/// size is the cycle rank of the bonds the rings use.
pub fn basis(rings: &[Vec<usize>]) -> BTreeSet<Vec<usize>> {
    let mut parent = BTreeMap::new();
    fn root(parent: &mut BTreeMap<usize, usize>, atom: usize) -> usize {
        let next = *parent.entry(atom).or_insert(atom);
        if next == atom {
            return atom;
        }
        let top = root(parent, next);
        parent.insert(atom, top);
        top
    }
    let mut bonds = BTreeSet::new();
    for ring in rings {
        for (i, &a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            bonds.insert((a.min(b), a.max(b)));
        }
    }
    let mut rank = bonds.len();
    for &(a, b) in &bonds {
        let (a, b) = (root(&mut parent, a), root(&mut parent, b));
        if a != b {
            parent.insert(a, b);
            rank -= 1;
        }
    }
    rings[..rank.min(rings.len())]
        .iter()
        .map(|ring| {
            let mut ring = ring.clone();
            ring.sort_unstable();
            ring
        })
        .collect()
}

/// Where a ring list (`rings` or `rings.atoms`) differs only because the two
/// SSSR bases differ, copy RDKit's rings into `actual`, so that every other
/// field is still compared. Returns how many ring lists were taken.
pub fn adopt_rdkit_basis(actual: &mut Value, expected: &Value) -> usize {
    fn list(rings: &mut Value) -> Option<&mut Value> {
        match rings {
            Value::Object(rings) => rings.get_mut("atoms"),
            rings => Some(rings),
        }
    }
    let atoms = |value: &Value| serde_json::from_value::<Vec<Vec<usize>>>(value.clone()).ok();
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            let mut taken = 0;
            // Fast ring finding is not an SSSR, so the rule does not apply.
            let theirs = e
                .get("rings")
                .filter(|rings| rings["kind"] != "fast")
                .map(|rings| rings.get("atoms").unwrap_or(rings));
            if let (Some(ours), Some(theirs)) = (a.get_mut("rings").and_then(list), theirs)
                && ours != theirs
                && let (Some(x), Some(y)) = (atoms(ours), atoms(theirs))
                && basis(&x) != basis(&y)
            {
                *ours = theirs.clone();
                taken += 1;
            }
            taken
                + a.iter_mut()
                    .filter(|(key, _)| *key != "rings")
                    .filter_map(|(key, value)| Some((value, e.get(key)?)))
                    .map(|(a, e)| adopt_rdkit_basis(a, e))
                    .sum::<usize>()
        }
        (Value::Array(a), Value::Array(e)) => a
            .iter_mut()
            .zip(e)
            .map(|(a, e)| adopt_rdkit_basis(a, e))
            .sum(),
        _ => 0,
    }
}
