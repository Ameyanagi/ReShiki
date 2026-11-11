//! The reshiki-chemistry core and reshiki-model adapters, plus this crate's adapters.
pub use reshiki_model::chemistry::*;

pub mod cdxml;
pub mod cleanup;
pub(crate) mod layout_refinement;
pub mod molfile;
pub mod reaction;

pub(crate) const ADDED_AXIS_WEDGE_NOTICE: &str =
    "Removed a wedge that would have added an atropisomer axis the file does not define";
pub(crate) const STEREO_GROUP_BONDS_NOTICE: &str = "Enhanced stereo group bond members (atropisomer axes) cannot be stored in the drawing and were dropped; atom members were retained";

/// Remove #1064's unintended axes while keeping source allene configurations
/// at the other terminal wherever the drawing can show them without an axis.
pub(crate) fn drop_added_axis_wedges(
    drawing: &mut crate::document::Document,
    source: &document::Molecule,
) -> Option<String> {
    let has_axis = |molecule: &document::Molecule| {
        let bonds = &molecule.state.metadata.bonds;
        bonds.iter().any(|bond| matches!(bond.stereo, 6 | 7))
    };
    if has_axis(source)
        || !drawing
            .bonds
            .iter()
            .any(|bond| !bond.projection && bond.order == 2 && bond.display == "wavy")
    {
        return None;
    }
    let prepared = document::prepare(drawing).ok()?;
    if !has_axis(&prepared) {
        return None;
    }
    let codes = |molecule: &document::Molecule| {
        stereo::cip::label::assign(&molecule.state, &Default::default())
            .map(|labeled| {
                molecule
                    .ids
                    .iter()
                    .zip(labeled.state.properties.atoms)
                    .filter_map(|(&id, atom)| atom.cip_code.map(|code| (id, code)))
                    .collect::<std::collections::BTreeMap<_, _>>()
            })
            .unwrap_or_default()
    };
    let source_codes = codes(source);
    let before_codes = codes(&prepared);
    let allenes: Vec<_> = source
        .ids
        .iter()
        .enumerate()
        .filter_map(|(center, &id)| {
            let terminals = stereo::allene::terminals(&source.state.graph, center)?
                .into_iter()
                .filter_map(|index| source.ids.get(index).copied())
                .collect::<Vec<_>>();
            Some((id, terminals))
        })
        .collect();
    let mut after_codes = before_codes.clone();
    let mut ends = std::collections::BTreeSet::new();
    let mut unspecified = std::collections::BTreeSet::new();
    let mut removed = 0;
    let mut prepared = Ok(prepared);
    // Clearing an axis or allene wedge can expose another axis or specify an
    // allene the file left open. Resolve both until no more wedges are removed.
    while let Ok(molecule) = &prepared {
        let (clearing_ends, allene) = if has_axis(molecule) {
            let axis_ends: Vec<_> = molecule
                .state
                .graph
                .bonds
                .iter()
                .zip(&molecule.state.metadata.bonds)
                .filter(|(_, metadata)| matches!(metadata.stereo, 6 | 7))
                .flat_map(|(bond, _)| [bond.a, bond.b])
                .filter_map(|index| molecule.ids.get(index).copied())
                .collect();
            ends.extend(axis_ends.iter().copied());
            (axis_ends, None)
        } else if let Some((id, terminals)) = allenes.iter().find(|(id, _)| {
            !source_codes.contains_key(id)
                && !before_codes.contains_key(id)
                && after_codes.contains_key(id)
        }) {
            (terminals.clone(), Some(*id))
        } else {
            break;
        };
        let mut cleared = 0;
        for bond in &mut drawing.bonds {
            if !bond.projection
                && bond.order == 1
                && clearing_ends.contains(&bond.a)
                && matches!(
                    bond.display.as_str(),
                    "wedge" | "hollow_wedge" | "bold" | "hash" | "hashed"
                )
            {
                bond.display = "plain".into();
                cleared += 1;
            }
        }
        if cleared == 0 {
            break;
        }
        if let Some(id) = allene {
            unspecified.insert(id);
        } else {
            removed += cleared;
        }
        prepared = document::prepare(drawing);
        if let Ok(molecule) = &prepared {
            after_codes = codes(molecule);
        }
    }
    let mut notice = if removed == 1 {
        ADDED_AXIS_WEDGE_NOTICE.into()
    } else {
        format!(
            "Removed {removed} wedges that would have added atropisomer axes the file does not define"
        )
    };
    notice.push_str(
        &"; the allene's other wedge was removed too, so it stays unspecified as in the file"
            .repeat(unspecified.len()),
    );
    if prepared.is_err() {
        return Some(notice);
    }
    // Recover source allene configurations at the other terminal if possible.
    for (id, terminals) in allenes {
        if source_codes.get(&id) == after_codes.get(&id) {
            continue;
        }
        let Some(expected) = source_codes.get(&id) else {
            continue;
        };
        let mut recovered = false;
        'recover: for terminal in terminals.into_iter().filter(|id| !ends.contains(id)) {
            let plain: Vec<_> = drawing
                .bonds
                .iter()
                .enumerate()
                .filter(|(_, bond)| {
                    !bond.projection
                        && bond.order == 1
                        && bond.display == "plain"
                        && (bond.a == terminal || bond.b == terminal)
                })
                .map(|(index, _)| index)
                .collect();
            for index in plain {
                for display in ["wedge", "hash"] {
                    let mut trial = drawing.clone();
                    if let Some(bond) = trial.bonds.get_mut(index) {
                        if bond.a != terminal {
                            std::mem::swap(&mut bond.a, &mut bond.b);
                        }
                        bond.display = display.into();
                    }
                    if let Ok(read) = document::prepare(&trial)
                        && !has_axis(&read)
                    {
                        let trial_codes = codes(&read);
                        if trial_codes.get(&id) == Some(expected) {
                            *drawing = trial;
                            after_codes = trial_codes;
                            recovered = true;
                            break 'recover;
                        }
                    }
                }
            }
        }
        if recovered {
            notice.push_str(&format!(
                "; the allene's ({expected}) configuration is now wedged at its other end"
            ));
        } else {
            if let Some(atom) = drawing.atom_mut(id) {
                atom.cip_label = None;
            }
            notice.push_str(&format!(
                "; the allene's ({expected}) configuration could not be kept"
            ));
        }
    }
    Some(notice)
}

#[cfg(test)]
mod allene_generated_tests;
#[cfg(test)]
mod allene_tests;
#[cfg(test)]
mod axial_tests;
