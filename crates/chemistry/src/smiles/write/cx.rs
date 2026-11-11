//! Enhanced stereo annotations in the atom order of a completed SMILES write.
use super::{Error, invalid};
use crate::ranking::Metadata;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Return a CX suffix such as ` |a:3,&1:1|`, or an empty string when there is
/// no AND or OR group. `atom_order` holds the source atom indices in written
/// order. `a:` lists the ungrouped stereocentres. Bond members have no CX form.
pub fn groups(metadata: &Metadata, atom_order: &[usize]) -> Result<String, Error> {
    if !metadata
        .groups
        .iter()
        .any(|group| group.kind != 0 && !group.atoms.is_empty())
    {
        return Ok(String::new());
    }
    let mut positions = vec![None; metadata.atoms.len()];
    for (position, &atom) in atom_order.iter().enumerate() {
        *positions
            .get_mut(atom)
            .ok_or_else(|| invalid("CX atom order is out of range"))? = Some(position);
    }
    let position = |atom: usize| {
        positions
            .get(atom)
            .copied()
            .flatten()
            .ok_or_else(|| invalid("CX stereo atom was not written"))
    };
    let mut fields = BTreeMap::<(u8, u32), BTreeSet<usize>>::new();
    let mut grouped = HashSet::new();
    for group in metadata.groups.iter().filter(|group| group.kind != 0) {
        let id = if group.write_id != 0 {
            group.write_id
        } else {
            group.read_id
        };
        for &atom in &group.atoms {
            grouped.insert(atom);
            fields
                .entry((group.kind, id))
                .or_default()
                .insert(position(atom)?);
        }
    }
    for (atom, meta) in metadata.atoms.iter().enumerate() {
        if matches!(meta.chiral_tag, 1 | 2 | 5) && !grouped.contains(&atom) {
            fields.entry((0, 0)).or_default().insert(position(atom)?);
        }
    }
    let fields = fields
        .into_iter()
        .map(|((kind, id), atoms)| {
            let atoms = atoms
                .into_iter()
                .map(|atom| atom.to_string())
                .collect::<Vec<_>>()
                .join(",");
            match kind {
                0 => format!("a:{atoms}"),
                1 => format!("o{id}:{atoms}"),
                _ => format!("&{id}:{atoms}"),
            }
        })
        .collect::<Vec<_>>();
    Ok(format!(" |{}|", fields.join(",")))
}
