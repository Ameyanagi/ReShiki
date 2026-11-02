//! Turn a drawn fragment into a catalog entry for the user's own groups.
use super::catalog::{Entry, FORMAT, Preset};
use crate::{
    chemistry::{document, smiles},
    document::Document,
};
use std::collections::HashSet;

/// A user group drawn on the canvas: the entry to save and the fragment atoms
/// that become the collapsed group in the drawing.
#[derive(Debug)]
pub struct Defined {
    pub entry: Entry,
    pub members: Vec<u64>,
}

/// Build an entry from the selected fragment. Its one outside single bond
/// becomes the `*` attachment. A fragment with no outside bond instead needs
/// one selected `*` atom on a single bond, which becomes the attachment.
pub fn from_selection(
    doc: &Document,
    selection: &[u64],
    label: &str,
    reverse: &str,
) -> Result<Defined, String> {
    let label = label.trim();
    let reverse = reverse.trim();
    let mut members: HashSet<u64> = doc
        .expand_abbreviation_selection(selection)
        .into_iter()
        .filter(|id| doc.atom(*id).is_some())
        .collect();
    if members.is_empty() {
        return Err("Select the atoms of the group first".into());
    }
    // A repeat, link or alternative stands for more than the drawn atoms.
    let generic = &doc.generic;
    if generic
        .repeats
        .iter()
        .any(|unit| unit.members.iter().any(|id| members.contains(id)))
        || generic
            .links
            .iter()
            .any(|link| members.contains(&link.atom))
        || generic
            .alternatives
            .iter()
            .any(|site| members.contains(&site.atom))
    {
        return Err("Expand the generic structure before defining a group from it".into());
    }
    let boundary: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| members.contains(&b.a) != members.contains(&b.b))
        .collect();
    let attachment = match boundary.as_slice() {
        [bond] if bond.order == 1 => {
            if members.contains(&bond.a) {
                bond.b
            } else {
                bond.a
            }
        }
        [] => {
            let stars: Vec<_> = members
                .iter()
                .copied()
                .filter(|id| doc.atom(*id).is_some_and(|a| a.element == "*"))
                .collect();
            let [star] = stars.as_slice() else {
                return Err(ATTACHMENT.into());
            };
            let bonds: Vec<_> = doc
                .bonds
                .iter()
                .filter(|b| b.a == *star || b.b == *star)
                .collect();
            let [bond] = bonds.as_slice() else {
                return Err(ATTACHMENT.into());
            };
            if bond.order != 1 {
                return Err(ATTACHMENT.into());
            }
            members.remove(star);
            *star
        }
        _ => return Err(ATTACHMENT.into()),
    };
    // A standalone copy: the fragment plus its attachment as a dummy atom.
    let mut part = Document {
        version: doc.version,
        ..Document::default()
    };
    for atom in &doc.atoms {
        if members.contains(&atom.id) {
            part.atoms.push(atom.clone());
        } else if atom.id == attachment {
            let mut dummy = atom.clone();
            dummy.element = "*".into();
            dummy.charge = 0;
            dummy.isotope = 0;
            dummy.explicit_h = 0;
            dummy.label_h = 0;
            dummy.radical_electrons = 0;
            dummy.singlet = false;
            dummy.aromatic = false;
            dummy.stereo = None;
            dummy.map_num = 0;
            part.atoms.push(dummy);
        }
    }
    part.bonds = doc
        .bonds
        .iter()
        .filter(|b| {
            let inside = |id| members.contains(&id) || id == attachment;
            inside(b.a) && inside(b.b)
        })
        .cloned()
        .collect();
    let molecule = document::prepare(&part).map_err(|e| format!("Check the fragment: {e}"))?;
    // SMILES output drops wavy and unknown stereo, so check before writing it.
    super::catalog::check_annotations(&molecule.state)?;
    let root = molecule
        .ids
        .iter()
        .position(|id| *id == attachment)
        .ok_or("The attachment point is unavailable")?;
    let written = smiles::write::write(
        &molecule.state,
        smiles::write::Options {
            root: Some(root),
            ..Default::default()
        },
    )
    .map_err(|e| format!("Could not write the group as SMILES: {e}"))?;
    let entry = Entry {
        label: label.into(),
        reverse: reverse.into(),
        smiles: written.text,
        formula: None,
        category: "user".into(),
        names: Vec::new(),
        aliases: Vec::new(),
        contract: true,
    };
    let preset = Preset::from_entry(entry.clone(), true)?;
    let mut members: Vec<_> = members.into_iter().collect();
    members.sort_unstable();
    Ok(Defined {
        entry: Entry {
            formula: Some(preset.formula.clone()),
            ..entry
        },
        members,
    })
}

const ATTACHMENT: &str = "Select a fragment with exactly one single bond to the rest of the drawing, \
or include one * attachment atom";

/// Add or replace (by label) one entry in the text of a user catalog file,
/// keeping the other entries, including skipped ones, in their order.
/// An unreadable file is an error rather than something to overwrite.
pub fn upsert(existing: Option<&str>, entry: Entry) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rows {
        format: u32,
        groups: Vec<serde_json::Value>,
    }
    let file = match existing.filter(|text| !text.trim().is_empty()) {
        Some(text) => serde_json::from_str::<Rows>(text).map_err(|e| {
            format!("Your abbreviation file is not valid ({e}); fix or move it first")
        })?,
        None => Rows {
            format: FORMAT,
            groups: Vec::new(),
        },
    };
    if file.format != FORMAT {
        return Err(format!(
            "Your abbreviation file uses format {}; this version writes format {FORMAT}",
            file.format
        ));
    }
    let mut groups = file.groups;
    let row = serde_json::to_value(&entry).map_err(|e| e.to_string())?;
    match groups
        .iter_mut()
        .find(|old| old.get("label").and_then(|l| l.as_str()) == Some(entry.label.as_str()))
    {
        Some(slot) => *slot = row,
        None => groups.push(row),
    }
    let rows = groups
        .iter()
        .map(|g| serde_json::to_string(g).map(|row| format!("    {row}")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{{\n  \"format\": {FORMAT},\n  \"groups\": [\n{}\n  ]\n}}\n",
        rows.join(",\n")
    ))
}

#[cfg(test)]
mod tests;
