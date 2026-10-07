//! Transactional chemistry edits shared by contextual drawing shortcuts.
mod ligands;
use crate::{
    atom_text::{self, Mode},
    bonds::BondPreset,
    document::{Document, Point},
    editing, templates,
};

/// Case is significant: lower-case letters and Shift-letter have different meanings.
pub fn atom_label(key: &str) -> Option<(&'static str, Mode)> {
    let element = match key {
        "b" => "Br",
        "B" => "B",
        "c" => "C",
        "C" | "l" => "Cl",
        "f" => "F",
        "h" => "H",
        "i" => "I",
        "L" => "Li",
        "n" | "w" => "N",
        "o" | "q" => "O",
        "p" => "P",
        "s" => "S",
        "S" => "Si",
        _ => "",
    };
    if !element.is_empty() {
        return Some((element, Mode::Auto));
    }
    let group = match key {
        "A" => "Ac",
        "e" => "Et",
        "E" => "CO2Me",
        "F" => "CF3",
        "H" => "Cbz",
        "m" => "Me",
        "M" => "MgBr",
        "Z" => "N3",
        "N" => "NO2",
        "O" => "OMe",
        "P" => "Ph",
        "Q" => "Fmoc",
        "y" => "Boc",
        _ => "",
    };
    if !group.is_empty() {
        return Some((group, Mode::Group));
    }
    match key {
        "r" => Some(("R", Mode::Text)),
        "x" => Some(("X", Mode::Text)),
        _ => None,
    }
}

pub fn bond_preset(key: &str) -> Option<BondPreset> {
    use BondPreset::*;
    Some(match key {
        "1" => Single,
        "2" => Double,
        "3" => Triple,
        "d" => Dashed,
        "D" => DashedDouble,
        "b" => Bold,
        "B" => BoldDouble,
        "w" => Wedge,
        "h" | "W" => HashedWedge,
        "H" => Hashed,
        "y" => Wavy,
        _ => return None,
    })
}

/// Change a bond as one transaction, straightening acyclic alkyne substituents.
pub fn bond_edit(doc: &Document, a: u64, b: u64, preset: BondPreset) -> Result<Document, String> {
    doc.validate()?;
    if doc
        .abbreviations
        .iter()
        .any(|g| g.members.contains(&a) || g.members.contains(&b))
    {
        return Err("Expand the abbreviation before changing its bonds".into());
    }
    let mut result = doc.clone();
    let original = doc
        .bonds
        .iter()
        .find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
        .ok_or("The bond is no longer available")?;
    if !preset.preserves_chemistry(original) {
        result.invalidate_chemistry(&[a, b]);
    }
    let bond = result
        .bonds
        .iter_mut()
        .find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
        .ok_or("The bond is no longer available")?;
    preset.apply(bond);
    if preset == BondPreset::Triple {
        for id in [a, b] {
            let atom = result.atom(id).ok_or("Missing triple-bond endpoint")?;
            if templates::valence(&result, id) + 2 * atom.explicit_h > templates::capacity(atom) {
                return Err(
                    "This endpoint has too many bonds or hydrogens for a triple bond".into(),
                );
            }
        }
        straighten_alkyne(&mut result, a, b)?;
    }
    result.validate()?;
    Ok(result)
}

fn straighten_alkyne(doc: &mut Document, a: u64, b: u64) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    let mut adjacency = HashMap::<u64, Vec<u64>>::new();
    for bond in &doc.bonds {
        adjacency.entry(bond.a).or_default().push(bond.b);
        adjacency.entry(bond.b).or_default().push(bond.a);
    }
    let mut rotations = Vec::new();
    for (anchor, other) in [(a, b), (b, a)] {
        let adjacent: Vec<_> = adjacency
            .get(&anchor)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| *id != other)
            .collect();
        let [start] = adjacent.as_slice() else {
            continue;
        };
        let mut component = HashSet::new();
        let mut pending = vec![*start];
        while let Some(id) = pending.pop() {
            if id == anchor || !component.insert(id) {
                continue;
            }
            pending.extend(
                adjacency
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|id| *id != anchor),
            );
        }
        // A ring cannot be straightened by rotating an independent branch.
        if component.contains(&other) {
            continue;
        }
        if doc.abbreviations.iter().any(|g| {
            g.members.iter().any(|id| component.contains(id))
                && !g.members.iter().all(|id| component.contains(id))
        }) {
            continue;
        }
        let pivot = doc.atom(anchor).ok_or("Missing alkyne anchor")?.position;
        let other = doc.atom(other).ok_or("Missing alkyne endpoint")?.position;
        let start = doc.atom(*start).ok_or("Missing substituent")?.position;
        let angle = (pivot.y - other.y).atan2(pivot.x - other.x)
            - (start.y - pivot.y).atan2(start.x - pivot.x);
        rotations.push((
            component.into_iter().collect::<Vec<_>>(),
            pivot,
            angle.to_degrees(),
        ));
    }
    for (ids, pivot, angle) in rotations {
        editing::transform_about(doc, &ids, pivot, 1., angle);
    }
    crate::projection::sync_centroids(doc);
    Ok(())
}

/// Returns None for an unassigned key. Errors leave the source unchanged.
pub fn atom_edit(
    doc: &Document,
    id: u64,
    key: &str,
    length: f32,
) -> Option<Result<(Document, u64), String>> {
    if matches!(key, "j" | "J") {
        return Some(ligands::add(
            doc,
            id,
            if key == "j" { 5 } else { 6 },
            length,
        ));
    }
    if let Some((label, mode)) = atom_label(key) {
        return Some(atom_text::apply(doc, id, label, mode).map(|mut candidate| {
            if mode == Mode::Auto
                && let Some(atom) = candidate.atom_mut(id)
            {
                atom.display.hydrogens = Some(true);
            }
            (candidate, id)
        }));
    }
    if ![
        "d", "+", "-", "0", "1", "2", "4", "5", "8", "9", "z", "K", "k",
    ]
    .contains(&key)
    {
        return None;
    }
    Some((|| {
        doc.validate()?;
        if !length.is_finite() || length <= 0. {
            return Err("Invalid bond length".into());
        }
        let source = doc.atom(id).ok_or("The atom is no longer available")?;
        if doc.abbreviations.iter().any(|g| g.members.contains(&id)) || !source.centroid.is_empty()
        {
            return Err("Expand this group before changing its atoms or bonds".into());
        }
        let mut result = doc.clone();
        let mut focus = id;
        match key {
            "d" => {
                result = atom_text::apply(doc, id, "H", Mode::Auto)?;
                result.atom_mut(id).ok_or("Missing isotope atom")?.isotope = 2;
            }
            "+" | "-" => {
                let atom = result.atom_mut(id).ok_or("Missing atom")?;
                atom.charge = atom
                    .charge
                    .checked_add(if key == "+" { 1 } else { -1 })
                    .filter(|charge| (-15..=15).contains(charge))
                    .ok_or("Charge limit reached")?;
            }
            "2" => {
                // Secondary carbon becomes a ketone. A terminal carbon becomes
                // acetyl; crowded/aromatic targets receive an acetyl substituent.
                let crowded =
                    source.element != "C" || source.aromatic || templates::valence(doc, id) > 4;
                let center = if crowded {
                    sprout(&mut result, id, "C", BondPreset::Single, length, false)?
                } else {
                    id
                };
                // Reserve the normal zigzag continuation for carbon. Placing
                // oxygen first would put it in that slot and grow methyl out
                // of the other side. An internal ketone needs no new carbon.
                focus = if templates::valence(&result, center) == 2 {
                    sprout(&mut result, center, "C", BondPreset::Single, length, false)?
                } else {
                    center
                };
                sprout(&mut result, center, "O", BondPreset::Double, length, false)?;
            }
            "9" | "K" => {
                let valence = templates::valence(&result, id);
                // Dimethyl grows directly on a labeled atom when it has room
                // for two bonds. A new carbon would change N,N-dimethyl into
                // N-isopropyl. Keep carbon's documented crowded-site fallback
                // and tert-butyl's carbon center, but reject unavailable
                // heteroatom valence instead of changing the intended group.
                let center = if key == "9" && source.element != "C" {
                    if valence.saturating_add(4) > templates::capacity(source) {
                        return Err(
                            "This atom has no available valence for two methyl groups".into()
                        );
                    }
                    id
                } else if source.element == "C"
                    && ((key == "9" && valence <= 4) || (key == "K" && valence <= 2))
                {
                    id
                } else {
                    sprout(&mut result, id, "C", BondPreset::Single, length, false)?
                };
                let count = if key == "K" { 3 } else { 2 };
                for angle in branch_angles(&result, center, count)? {
                    sprout_at(
                        &mut result,
                        center,
                        "C",
                        BondPreset::Single,
                        length,
                        false,
                        Some(angle),
                    )?;
                }
                focus = center;
            }
            "k" => {
                // Replace a suitable chain carbon, retaining its C-S-C framework.
                // Terminal sites receive the second substituent before the oxo pair.
                let center = if source.element == "C"
                    && !source.aromatic
                    && templates::valence(doc, id) <= 4
                    && doc
                        .bonds
                        .iter()
                        .filter(|b| b.a == id || b.b == id)
                        .all(|b| b.order == 1)
                {
                    if source.no_implicit || source.explicit_h != 0 || source.stereo.is_some() {
                        return Err(
                            "Edit explicit hydrogens or stereochemistry before replacing this atom"
                                .into(),
                        );
                    }
                    result = atom_text::apply(doc, id, "S", Mode::Auto)?;
                    id
                } else {
                    sprout(&mut result, id, "S", BondPreset::Single, length, false)?
                };
                while result
                    .bonds
                    .iter()
                    .filter(|b| b.a == center || b.b == center)
                    .count()
                    < 2
                {
                    sprout(&mut result, center, "C", BondPreset::Single, length, false)?;
                }
                for angle in branch_angles(&result, center, 2)? {
                    sprout_at(
                        &mut result,
                        center,
                        "O",
                        BondPreset::Double,
                        length,
                        false,
                        Some(angle),
                    )?;
                }
                focus = center;
            }
            _ => {
                let preset = match key {
                    "4" => BondPreset::Wedge,
                    "5" => BondPreset::HashedWedge,
                    "8" => BondPreset::Double,
                    "z" => BondPreset::Triple,
                    _ => BondPreset::Single,
                };
                focus = sprout(&mut result, id, "C", preset, length, key == "0")?;
            }
        }
        result.invalidate_chemistry(&[id, focus]);
        result.validate()?;
        Ok((result, focus))
    })())
}

fn sprout(
    doc: &mut Document,
    id: u64,
    element: &str,
    preset: BondPreset,
    length: f32,
    keep_center: bool,
) -> Result<u64, String> {
    sprout_at(doc, id, element, preset, length, keep_center, None)
}

fn branch_angles(doc: &Document, id: u64, count: usize) -> Result<Vec<f32>, String> {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_6, PI};
    let origin = doc.atom(id).ok_or("Missing branch atom")?.position;
    let neighbors: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            let id = if b.a == id {
                b.b
            } else if b.b == id {
                b.a
            } else {
                return None;
            };
            doc.atom(id).map(|a| a.position)
        })
        .collect();
    let direction = editing::open_angle(origin, &neighbors);
    Ok(match (count, neighbors.len()) {
        (2, 2) => vec![direction - FRAC_PI_6, direction + FRAC_PI_6],
        (2, _) => vec![direction - PI / 3., direction + PI / 3.],
        (3, 1) => vec![direction - FRAC_PI_2, direction, direction + FRAC_PI_2],
        (3, _) => vec![
            direction,
            direction + 2. * PI / 3.,
            direction - 2. * PI / 3.,
        ],
        _ => return Err("Unsupported branch count".into()),
    })
}

fn sprout_at(
    doc: &mut Document,
    id: u64,
    element: &str,
    preset: BondPreset,
    length: f32,
    keep_center: bool,
    angle: Option<f32>,
) -> Result<u64, String> {
    let atom = doc.atom(id).ok_or("Missing growth atom")?;
    let order = match preset {
        BondPreset::Double => 2,
        BondPreset::Triple => 3,
        _ => 1,
    };
    let capacity = if atom.element == "S" && atom.charge == 0 {
        12
    } else {
        templates::capacity(atom)
    };
    let replacing_h = crate::projection::growth::replaces_hydrogen(doc, id, preset);
    if atom.stereo.is_some()
        || !replacing_h
            && (atom.no_implicit
                || atom.explicit_h != 0
                || templates::valence(doc, id) + order * 2 > capacity)
    {
        return Err(
            "This atom has no available valence; edit its hydrogens or stereochemistry first"
                .into(),
        );
    }
    doc.next_id()
        .checked_add(1)
        .ok_or("Object ID limit exceeded")?;
    if doc.atoms.len() >= 100_000 {
        return Err("Atom limit exceeded".into());
    }
    let start = atom.position;
    let depth = atom.depth;
    if let Some(plane) = crate::projection::growth::Plane::at(doc, id) {
        let end = if let Some(angle) = angle {
            plane.endpoint(
                start.offset(length * angle.cos(), length * angle.sin()),
                crate::chains::BondDrawing {
                    length,
                    fixed_angles: false,
                    fixed_length: true,
                },
            )
        } else {
            plane.outward(length)
        }
        .ok_or("Cannot place a bond in the ring plane")?;
        if doc.nearest(end.position, length * 0.15).is_some() {
            return Err(
                "The new atom would overlap another atom; choose a different direction".into(),
            );
        }
        let (drawing, end) = crate::projection::growth::place(doc, id, end, element, preset)?;
        *doc = drawing;
        return Ok(if keep_center { id } else { end });
    }
    let proposed = angle
        .map(|angle| start.offset(length * angle.cos(), length * angle.sin()))
        .unwrap_or_else(|| editing::bond_extension(doc, start, Some(id), order as u8));
    let distance = proposed.distance(start).max(0.001);
    let position = Point::new(
        start.x + (proposed.x - start.x) * length / distance,
        start.y + (proposed.y - start.y) * length / distance,
    );
    if doc.nearest(position, length * 0.15).is_some() {
        return Err("The new atom would overlap another atom; choose a different direction".into());
    }
    let end = doc.add_atom(element, position);
    doc.atom_mut(end).ok_or("Missing new atom")?.depth = depth;
    preset.place(doc, id, end);
    Ok(if keep_center { id } else { end })
}

/// Ring insertion uses the same valence checks and fusion rules as the template tool.
pub fn ring_edit(
    doc: &Document,
    atom: Option<u64>,
    bond: Option<(u64, u64)>,
    key: &str,
    length: f32,
) -> Option<Result<(Document, Vec<u64>), String>> {
    let size = match (atom.is_some(), key) {
        (true, "3" | "a") | (false, "a") => 6,
        (true, "6") => 6,
        (true, "7") => 5,
        (_, "v") => 3,
        (true, "u") => 4,
        (false, "4") => 4,
        (false, "5") => 5,
        (false, "6") => 6,
        (false, "7") => 7,
        (false, "8") => 8,
        (false, "9" | "0" | "z") => 0,
        _ => return None,
    };
    Some((|| {
        let aromatic = key == "a" || atom.is_some() && key == "3";
        let mut part = if size == 0 {
            match key {
                "9" => crate::rings::Preset::ChairUp,
                "0" => crate::rings::Preset::ChairDown,
                _ => crate::rings::Preset::Cyclopentadiene,
            }
            .document(length, false)
        } else {
            let mut part = Document::default();
            let radius = length / (2. * (std::f32::consts::PI / size as f32).sin());
            let ids: Vec<_> = (0..size)
                .map(|i| {
                    let theta = i as f32 * std::f32::consts::TAU / size as f32;
                    part.add_atom("C", Point::new(radius * theta.cos(), radius * theta.sin()))
                })
                .collect();
            for (index, (&a, &b)) in ids
                .iter()
                .zip(ids.iter().cycle().skip(1))
                .take(ids.len())
                .enumerate()
            {
                part.add_bond(
                    a,
                    b,
                    if aromatic && index % 2 == 0 { 2 } else { 1 },
                    "plain",
                );
            }
            part
        };
        let mut anchor = templates::Anchor::Auto;
        let (point, mode) = if let Some(id) = atom {
            let target = doc.atom(id).ok_or("Missing ring target")?;
            let capacity = templates::capacity(target);
            let share = templates::valence(doc, id).saturating_add(if aromatic { 6 } else { 4 })
                <= capacity;
            if share && target.element != "C" {
                // The hotspot remains the ring vertex, including a nitrogen
                // in a saturated ring. Give the temporary source anchor the
                // same identity so the existing template compatibility and
                // valence checks can share it without relaxing their rules.
                let source = part.atoms.first_mut().ok_or("Missing source ring atom")?;
                source.element = target.element.clone();
                source.charge = target.charge;
                source.isotope = target.isotope;
                anchor = templates::Anchor::Atom(source.id);
            }
            (
                target.position,
                if share {
                    templates::Connection::ShareAtom
                } else {
                    templates::Connection::Connect
                },
            )
        } else if let Some((a, b)) = bond {
            let a = doc.atom(a).ok_or("Missing bond endpoint")?.position;
            let b = doc.atom(b).ok_or("Missing bond endpoint")?.position;
            (
                Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.),
                templates::Connection::FuseBond,
            )
        } else {
            return Err("Point to an atom or bond to attach a ring".into());
        };
        templates::place_with_mode(doc, &part, point, None, length * 0.1, anchor, mode)
            .map_err(str::to_owned)
    })())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ring_angle_tests;

#[cfg(test)]
mod drawing_quality_tests;
