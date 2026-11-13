//! RDKit 2026.09 takes its SSSR basis from RingDecomposerLib (rdkit#9105),
//! which changes the rings a layout is built from. See "Deliberate
//! differences" in reference/README.md.
use reshiki::{chemistry, document::Document};
use std::collections::BTreeSet;

/// Whether RDKit's SSSR basis for the drawing (sorted atom lists in document
/// order, from the oracle) differs from ReShiki's.
pub fn differs(rdkit: Option<&Vec<Vec<usize>>>, document: Option<&Document>) -> bool {
    let (Some(rdkit), Some(document)) = (rdkit, document) else {
        return false;
    };
    // A reaction drawing is prepared as its molecules.
    let mut molecules = document.clone();
    molecules.reactions.clear();
    molecules.arrows.clear();
    let Ok(molecule) = chemistry::document::prepare(&molecules) else {
        return false;
    };
    // Layout rings include dative bonds, as RDKit's RingInfo does.
    let options = chemistry::rings::Options {
        include_dative: true,
        include_hydrogen: false,
    };
    let Ok(rings) = chemistry::rings::perceive(&molecule.state.graph, options) else {
        return false;
    };
    let ours: BTreeSet<Vec<usize>> = rings.atoms[..rings.basis_count]
        .iter()
        .map(|ring| {
            let mut ring = ring.clone();
            ring.sort_unstable();
            ring
        })
        .collect();
    ours != rdkit.iter().cloned().collect()
}
