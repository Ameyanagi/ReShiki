//! Persistent atom and bond paint, independent of molecular ink and selection.
use crate::{
    document::{Atom, Document},
    palette::Color,
};
use std::collections::{HashMap, HashSet};

/// Whether a drawing carries any highlight paint, including collapsed members.
pub fn any(doc: &Document) -> bool {
    doc.atoms.iter().any(|a| a.display.highlight.is_some())
        || doc.bonds.iter().any(|b| b.highlight.is_some())
        || doc.abbreviations.iter().any(|g| g.highlight.is_some())
}

/// A contracted label can have its own paint without recoloring its anchor
/// inside the expanded fragment.
pub(crate) fn atom_color(doc: &Document, atom: &Atom) -> Option<Color> {
    doc.abbreviation(atom.id)
        .and_then(|group| group.highlight)
        .or(atom.display.highlight)
}

/// Visible highlight colors for the selected objects. Hidden abbreviation
/// members retain their own colors without making the label's swatch mixed.
pub fn selected_colors(doc: &Document, ids: &[u64]) -> Vec<Option<Color>> {
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    doc.atoms
        .iter()
        .filter(|a| selected.contains(&a.id) && doc.atom_visible(a.id))
        .map(|a| atom_color(doc, a))
        .chain(
            doc.bonds
                .iter()
                .filter(|b| {
                    selected.contains(&b.a) && selected.contains(&b.b) && doc.bond_visible(b.a, b.b)
                })
                .map(|b| b.highlight),
        )
        .collect()
}

/// Apply or clear paint on selected atoms and bonds connecting them. Contracted
/// labels propagate new paint to unpainted internal members while preserving
/// their existing colors; the visible label has independent highlight paint.
/// Clearing a selected group removes all its highlights, including its members.
pub fn apply(doc: &mut Document, ids: &[u64], color: Option<Color>) -> usize {
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    let mut contracted = HashSet::new();
    for group in &mut doc.abbreviations {
        if group.members.iter().any(|id| selected.contains(id)) {
            group.highlight = color;
            contracted.extend(group.members.iter().copied());
        }
    }
    let visible_atoms: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| selected.contains(&a.id) && doc.atom_visible(a.id))
        .map(|a| a.id)
        .collect();
    let visible_bonds: HashSet<_> = doc
        .bonds
        .iter()
        .filter(|b| {
            selected.contains(&b.a) && selected.contains(&b.b) && doc.bond_visible(b.a, b.b)
        })
        .map(|b| (b.a, b.b))
        .collect();
    for atom in &mut doc.atoms {
        if selected.contains(&atom.id)
            && (color.is_none()
                || atom.display.highlight.is_none()
                || !contracted.contains(&atom.id))
        {
            atom.display.highlight = color;
        }
    }
    for bond in &mut doc.bonds {
        if selected.contains(&bond.a)
            && selected.contains(&bond.b)
            && (color.is_none()
                || bond.highlight.is_none()
                || visible_bonds.contains(&(bond.a, bond.b)))
        {
            bond.highlight = color;
        }
    }
    visible_atoms.len() + visible_bonds.len()
}

/// A shared atom/bond keeps the destination's paint when both were painted;
/// otherwise it inherits the source paint before that duplicate is removed.
pub(crate) fn inherit_merged(doc: &mut Document, mapping: &HashMap<u64, u64>) {
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .filter_map(|atom| Some((*mapping.get(&atom.id)?, atom.display.highlight?)))
        .collect();
    for (id, color) in atoms {
        if let Some(atom) = doc.atom_mut(id) {
            atom.display.highlight.get_or_insert(color);
        }
    }
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|bond| {
            Some((
                *mapping.get(&bond.a)?,
                *mapping.get(&bond.b)?,
                bond.highlight?,
            ))
        })
        .collect();
    for (a, b, color) in bonds {
        if let Some(bond) = doc
            .bonds
            .iter_mut()
            .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
        {
            bond.highlight.get_or_insert(color);
        }
    }
}
