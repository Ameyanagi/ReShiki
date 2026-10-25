//! Relabelling a ligand does not rotate the unchanged tetrahedral atom beside it.
use crate::{
    chemistry::{document, stereo::perception},
    document::Document,
};
use std::collections::HashSet;

fn star(doc: &Document, center: u64) -> Vec<(u64, u8)> {
    let mut neighbors: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|bond| {
            if bond.a == center {
                Some((bond.b, bond.order))
            } else if bond.b == center {
                Some((bond.a, bond.order))
            } else {
                None
            }
        })
        .collect();
    neighbors.sort_unstable();
    neighbors
}

pub(super) fn invalidate(
    before: &Document,
    after: &mut Document,
    affected: &[u64],
) -> Result<(), String> {
    let affected_ids: HashSet<_> = affected.iter().copied().collect();
    let retained: Vec<_> = before
        .atoms
        .iter()
        .filter(|atom| !affected_ids.contains(&atom.id))
        .filter_map(|atom| {
            let stereo = atom.stereo.as_ref()?;
            (stereo.neighbors.iter().any(|id| affected_ids.contains(id))
                && after.atom(atom.id) == Some(atom)
                && star(before, atom.id) == star(after, atom.id))
            .then_some((atom.id, stereo.clone()))
        })
        .collect();
    after.invalidate_chemistry(affected);
    // These tags encode an orientation relative to persistent neighbor IDs,
    // not an R/S assignment. Restore them together: other configured centers
    // may distinguish otherwise equivalent substituents during perception.
    for (id, stereo) in &retained {
        after
            .atom_mut(*id)
            .ok_or("Missing retained stereocenter")?
            .stereo = Some(stereo.clone());
    }
    let configured: HashSet<_> = after
        .atoms
        .iter()
        .filter(|atom| atom.stereo.is_some())
        .map(|atom| atom.id)
        .collect();
    if configured.is_empty() {
        return Ok(());
    }
    let changed = affected
        .iter()
        .copied()
        .chain(retained.iter().map(|(id, _)| *id))
        .collect::<Vec<_>>();
    for ids in crate::reactions::molecules(after, &changed) {
        if !ids.iter().any(|id| configured.contains(id)) {
            continue;
        }
        // A complete component retains ligand identity without selection H
        // caps. Unrelated invalid or generic drawings cannot veto this edit.
        let part = crate::editing::selection(after, &ids);
        let checked = document::prepare(&part).map_err(|error| {
            format!("Could not verify stereochemistry after this label edit: {error}")
        })?;
        let cleaned = perception::perceive(
            &checked.state,
            perception::Options {
                clean: true,
                force: true,
                flag_possible: false,
            },
        )
        .map_err(|error| {
            format!("Could not verify stereochemistry after this label edit: {error}")
        })?;
        // Ligand identity can make an unchanged star nonstereogenic. Clear
        // only tags that cleanup proves invalid; do not install its bond
        // stereo, hydrogen caches, computed R/S labels or other state.
        let invalid: HashSet<_> = checked
            .ids
            .iter()
            .zip(&cleaned.metadata.atoms)
            .filter(|(id, meta)| configured.contains(id) && meta.chiral_tag == 0)
            .map(|(id, _)| *id)
            .collect();
        for atom in &mut after.atoms {
            if invalid.contains(&atom.id) {
                atom.stereo = None;
            }
        }
    }
    Ok(())
}
