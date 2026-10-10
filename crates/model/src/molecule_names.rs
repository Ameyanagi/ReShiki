//! Native associations for locally verified chemical-name captions.
//!
//! The visible caption is an ordinary annotation. The optional native link
//! makes it reusable, follows layout edits and removes obsolete names. Export
//! formats that cannot retain the link still retain the annotation text.
use crate::{
    chemistry,
    document::{Annotation, Document, Point},
    typography::{Script, TextAlign, TextFormat},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoleculeName {
    pub annotation: u64,
    pub atoms: Vec<u64>,
    /// Canonical isomeric identity; atom maps are deliberately excluded.
    pub smiles: String,
    /// The text last generated. User-edited text becomes an ordinary caption.
    pub text: String,
}

/// A chemical connected component, independent of drawing groups and captions.
pub fn component(doc: &Document, seed: u64) -> Vec<u64> {
    if doc.atom(seed).is_none() {
        return vec![];
    }
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for bond in &doc.bonds {
        adjacent.entry(bond.a).or_default().push(bond.b);
        adjacent.entry(bond.b).or_default().push(bond.a);
    }
    let mut found = HashSet::from([seed]);
    let mut pending = vec![seed];
    while let Some(id) = pending.pop() {
        for id in adjacent.get(&id).into_iter().flatten() {
            if found.insert(*id) {
                pending.push(*id);
            }
        }
    }
    doc.atoms
        .iter()
        .filter(|atom| found.contains(&atom.id))
        .map(|atom| atom.id)
        .collect()
}

fn same_atoms(a: &[u64], b: &[u64]) -> bool {
    a.len() == b.len() && a.iter().all(|id| b.contains(id))
}

pub fn label<'a>(doc: &'a Document, atoms: &[u64]) -> Option<&'a MoleculeName> {
    doc.molecule_names
        .iter()
        .find(|name| same_atoms(&name.atoms, atoms))
}

/// Copying a whole molecule also copies its name. Copying the caption alone
/// keeps ordinary editable text, without a dangling chemical association.
pub fn include_annotations(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut selected: HashSet<_> = ids.iter().copied().collect();
    for name in &doc.molecule_names {
        if name.atoms.iter().all(|id| selected.contains(id)) {
            selected.insert(name.annotation);
        }
    }
    doc.all_ids()
        .into_iter()
        .filter(|id| selected.contains(id))
        .collect()
}

#[cfg(test)]
thread_local! { pub(crate) static IDENTITY_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
/// Exact native chemical identity for a complete component. This does not
/// generate a name and never replaces unsupported stereo with a plain graph.
fn identity(doc: &Document, atoms: &[u64]) -> Result<String, String> {
    #[cfg(test)]
    IDENTITY_ENTRIES.with(|v| v.set(v.get() + 1));
    let seed = atoms.first().copied().ok_or("Select a molecule")?;
    if atoms.len() > 512 || !same_atoms(&component(doc, seed), atoms) {
        return Err("The named molecule changed its connected component".into());
    }
    // Use the shared fragment path so abbreviations and unresolved semantic
    // metadata cannot disappear before chemical validation.
    let part = crate::editing::analysis_document(doc, atoms);
    if part
        .bonds
        .iter()
        .any(|bond| !bond.projection && bond.display == "wavy")
    {
        return Err("Unknown stereochemistry invalidates a generated name".into());
    }
    let mut state = chemistry::document::prepare(&part)
        .map_err(|error| error.to_string())?
        .state;
    if state
        .graph
        .atoms
        .iter()
        .any(|atom| atom.atomic_number == 0 || atom.radical_electrons != 0)
        || state
            .graph
            .bonds
            .iter()
            .any(|bond| !(1..=4).contains(&bond.order))
        || !state.metadata.groups.is_empty()
        || state.properties.atoms.iter().any(|atom| atom.unknown)
        || state
            .metadata
            .bonds
            .iter()
            .any(|bond| bond.unknown_stereo || bond.stereo == 1 || bond.stereo > 5)
        || state.metadata.atoms.iter().any(|atom| atom.chiral_tag > 2)
    {
        return Err("Unsupported chemical semantics invalidate a generated name".into());
    }
    for atom in &mut state.metadata.atoms {
        atom.map_number = 0;
        atom.map_present = false;
    }
    chemistry::smiles::write::write(
        &state,
        chemistry::smiles::write::Options {
            ignore_maps: true,
            ..Default::default()
        },
    )
    .map(|smiles| smiles.text)
    .map_err(|error| error.to_string())
}

fn position(doc: &Document, atoms: &[u64], caption: &Annotation) -> Option<Point> {
    let (lo, hi) = crate::scene::selection_bounds(doc, atoms)?;
    let width = caption.size().0;
    let gap = doc.drawing_style.font_size().max(12.) * 0.5;
    Some(Point::new((lo.x + hi.x - width) / 2., hi.y + gap))
}

/// Insert or reuse the verified caption; all visible text uses the regular
/// renderer, editing, native save and figure/clipboard export paths.
pub fn show(
    doc: &mut Document,
    atoms: &[u64],
    text: String,
    smiles: String,
    mut format: TextFormat,
) -> Result<u64, String> {
    if text.trim().is_empty() || text.len() > 32_768 || identity(doc, atoms)? != smiles {
        return Err("The generated name does not match this molecule".into());
    }
    let (lo, hi) =
        crate::scene::selection_bounds(doc, atoms).ok_or("The molecule has no bounds")?;
    format.spans.clear();
    format.style.formula = false;
    format.style.script = Script::Normal;
    format.style.size_pt = format
        .style
        .size_pt
        .max(doc.drawing_style.font_size_pt)
        .max(8.);
    format.alignment = TextAlign::Center;
    // Wrap long names while keeping every line centered below the molecule.
    let natural_width = crate::typography::layout(&text, &format).width;
    let width = (hi.x - lo.x)
        .max(doc.drawing_style.bond_length_world * 3.)
        .max(natural_width.min(doc.drawing_style.bond_length_world * 6.));
    format.width_pt = Some(width * doc.drawing_style.points_per_world());
    format.validate(&text)?;
    let id = label(doc, atoms)
        .map(|name| name.annotation)
        .unwrap_or_else(|| doc.next_id());
    let mut caption = Annotation {
        id,
        position: Point::default(),
        text: text.clone(),
        format,
    };
    caption.position = position(doc, atoms, &caption).ok_or("The molecule has no bounds")?;
    if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == id) {
        *current = caption;
    } else {
        doc.annotations.push(caption);
    }
    doc.molecule_names.retain(|name| name.annotation != id);
    doc.molecule_names.push(MoleculeName {
        annotation: id,
        atoms: atoms.to_vec(),
        smiles,
        text,
    });
    Ok(id)
}

pub fn hide(doc: &mut Document, atoms: &[u64]) -> bool {
    let Some(id) = label(doc, atoms).map(|name| name.annotation) else {
        return false;
    };
    doc.delete(&[id]);
    true
}

/// Called in the same transaction as the edit. Obsolete automatic text is
/// removed; user-edited text, or a caption dragged independently, is detached.
pub fn reconcile(doc: &mut Document, before: &Document) {
    let names = std::mem::take(&mut doc.molecule_names);
    for name in names {
        let Some(caption) = doc.annotations.iter().find(|a| a.id == name.annotation) else {
            continue;
        };
        if caption.text != name.text {
            continue;
        }
        let previous = before.annotations.iter().find(|a| a.id == name.annotation);
        let molecule_moved = name.atoms.iter().any(|id| {
            doc.atom(*id).map(|atom| (atom.position, atom.depth))
                != before.atom(*id).map(|atom| (atom.position, atom.depth))
        });
        if previous.is_some_and(|a| a.position != caption.position) && !molecule_moved {
            continue;
        }
        if !identity(doc, &name.atoms).is_ok_and(|actual| actual == name.smiles) {
            doc.delete(&[name.annotation]);
            continue;
        }
        if let Some(at) = position(doc, &name.atoms, caption)
            && let Some(caption) = doc.annotations.iter_mut().find(|a| a.id == name.annotation)
        {
            caption.position = at;
        }
        doc.molecule_names.push(name);
    }
}

/// Prune references after explicit deletion. The selection extractor drops
/// links first, so copying a caption alone never deletes its visible text.
pub fn prune(doc: &mut Document) {
    let mut removed = Vec::new();
    doc.molecule_names.retain(|name| {
        let keep = name
            .atoms
            .iter()
            .all(|id| doc.atoms.iter().any(|a| a.id == *id));
        if !keep
            && doc
                .annotations
                .iter()
                .any(|a| a.id == name.annotation && a.text == name.text)
        {
            removed.push(name.annotation);
        }
        keep && doc.annotations.iter().any(|a| a.id == name.annotation)
    });
    doc.annotations.retain(|a| !removed.contains(&a.id));
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let mut captions = HashSet::new();
    let mut atoms = HashSet::new();
    for name in &doc.molecule_names {
        if !captions.insert(name.annotation)
            || !doc.annotations.iter().any(|a| a.id == name.annotation)
            || name.atoms.is_empty()
            || name.atoms.len() > 512
            || name
                .atoms
                .iter()
                .any(|id| !atoms.insert(*id) || doc.atom(*id).is_none())
            || name.smiles.is_empty()
            || name.smiles.len() > 32_768
            || name.text.trim().is_empty()
            || name.text.len() > 32_768
        {
            return Err("Invalid molecule-name caption association".into());
        }
    }
    Ok(())
}
/// Transfer an existing association through an exact remap/common translation.
/// This proves inheritance of a complete unchanged component, not the truth of
/// arbitrary native SMILES/name claims. The caller precharges component scans,
/// endpoint searches, string comparisons and snapshots under its shared budget.
pub(crate) fn certify_inherited(
    source: &Document,
    part: &Document,
    mapping: &HashMap<u64, u64>,
    offset: Point,
) -> Result<(), String> {
    if source.molecule_names.len() != part.molecule_names.len() {
        return Err("Prepared insertion changed name associations".into());
    }
    for (old, new) in source.molecule_names.iter().zip(&part.molecule_names) {
        let seed = old
            .atoms
            .first()
            .copied()
            .ok_or("Empty inherited named component")?;
        let complete: HashSet<_> = component(source, seed).into_iter().collect();
        if complete.len() != old.atoms.len() || old.atoms.iter().any(|id| !complete.contains(id)) {
            return Err("Prepared insertion requires a complete inherited named component".into());
        }
        let ids: Option<Vec<_>> = old
            .atoms
            .iter()
            .map(|id| mapping.get(id).copied())
            .collect();
        if mapping.get(&old.annotation) != Some(&new.annotation)
            || ids.as_ref() != Some(&new.atoms)
            || old.smiles != new.smiles
            || old.text != new.text
        {
            return Err("Prepared insertion changed inherited name metadata".into());
        }
        let caption = source
            .annotations
            .iter()
            .find(|a| a.id == old.annotation)
            .ok_or("Missing inherited caption")?;
        let actual = part
            .annotations
            .iter()
            .find(|a| a.id == new.annotation)
            .ok_or("Missing remapped caption")?;
        if caption.text != old.text
            || actual.text != caption.text
            || actual.format != caption.format
            || actual.position != caption.position.offset(offset.x, offset.y)
        {
            return Err(
                "Prepared insertion changed inherited caption text, style or translation".into(),
            );
        }
        for &id in &old.atoms {
            let old_atom = source.atom(id).ok_or("Missing inherited named atom")?;
            let mut expected = old_atom.clone();
            expected.id = *mapping.get(&id).ok_or("Missing inherited atom remap")?;
            expected.position = expected.position.offset(offset.x, offset.y);
            for id in &mut expected.centroid {
                *id = *mapping.get(id).ok_or("Missing inherited centroid remap")?;
            }
            if let Some(stereo) = &mut expected.stereo {
                for id in &mut stereo.neighbors {
                    *id = *mapping.get(id).ok_or("Missing inherited stereo remap")?;
                }
            }
            if part.atom(expected.id) != Some(&expected) {
                return Err(
                    "Prepared insertion changed named-atom chemistry or presentation".into(),
                );
            }
        }
        if source.bonds.len() != part.bonds.len() {
            return Err("Prepared insertion changed inherited bond storage".into());
        }
        for (bond, actual) in source.bonds.iter().zip(&part.bonds) {
            if !complete.contains(&bond.a) && !complete.contains(&bond.b) {
                continue;
            }
            let mut expected = bond.clone();
            expected.a = *mapping.get(&bond.a).ok_or("Missing inherited bond remap")?;
            expected.b = *mapping.get(&bond.b).ok_or("Missing inherited bond remap")?;
            for id in &mut expected.stereo_atoms {
                *id = *mapping
                    .get(id)
                    .ok_or("Missing inherited bond stereo remap")?;
            }
            if actual != &expected {
                return Err("Prepared insertion changed inherited named-component bonds".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
