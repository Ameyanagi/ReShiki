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
                sprout(&mut result, center, "O", BondPreset::Double, length, false)?;
                if templates::valence(&result, center) == 6 {
                    sprout(&mut result, center, "C", BondPreset::Single, length, false)?;
                }
                focus = center;
            }
            "9" | "K" => {
                let valence = templates::valence(&result, id);
                let center = if source.element == "C"
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
        let part = if size == 0 {
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
        let (point, mode) = if let Some(id) = atom {
            let target = doc.atom(id).ok_or("Missing ring target")?;
            let capacity = templates::capacity(target);
            let share = target.element == "C"
                && templates::valence(doc, id) + if aromatic { 6 } else { 4 } <= capacity;
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
        templates::place_with_mode(
            doc,
            &part,
            point,
            None,
            length * 0.1,
            templates::Anchor::Auto,
            mode,
        )
        .map_err(str::to_owned)
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{LocalEngine, Request};

    fn ethane() -> (Document, u64, u64) {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        (doc, a, b)
    }

    #[test]
    fn case_sensitive_groups_contain_real_atoms() -> Result<(), String> {
        let (doc, _, target) = ethane();
        for (key, label, carbons, oxygens) in [
            ("m", "Me", 1, 0),
            ("e", "Et", 2, 0),
            ("O", "OMe", 1, 1),
            ("A", "Ac", 2, 1),
            ("E", "CO2Me", 2, 2),
            ("F", "CF3", 1, 0),
            ("y", "Boc", 5, 2),
            ("H", "Cbz", 8, 2),
            ("Q", "Fmoc", 15, 2),
            ("N", "NO2", 0, 2),
            ("P", "Ph", 6, 0),
        ] {
            let (candidate, _) = atom_edit(&doc, target, key, 42.).ok_or("Unmapped group")??;
            let group = candidate
                .abbreviation(target)
                .ok_or("Missing real abbreviation")?;
            assert_eq!(group.label, label);
            let count = |element| {
                candidate
                    .atoms
                    .iter()
                    .filter(|a| group.members.contains(&a.id) && a.element == element)
                    .count()
            };
            assert_eq!(count("C"), carbons, "{key}: carbon count");
            assert_eq!(count("O"), oxygens, "{key}: oxygen count");
            candidate.validate()?;
        }
        for (key, expected) in [
            ("c", "C"),
            ("C", "Cl"),
            ("n", "N"),
            ("o", "O"),
            ("b", "Br"),
            ("B", "B"),
            ("s", "S"),
            ("S", "Si"),
        ] {
            let (candidate, _) = atom_edit(&doc, target, key, 42.).ok_or("Unmapped element")??;
            assert_eq!(
                candidate.atom(target).ok_or("Missing atom")?.element,
                expected
            );
            assert!(candidate.abbreviations.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn carbonyl_hotkey_produces_acetone_and_a_secondary_ketone() -> Result<(), String> {
        let (doc, _, end) = ethane();
        let (acetone, _) = atom_edit(&doc, end, "2", 42.).ok_or("Missing shortcut")??;
        assert_eq!(acetone.atoms.len(), 4);
        assert_eq!(acetone.bonds.iter().filter(|b| b.order == 2).count(), 1);
        let engine = LocalEngine::default();
        let analysis = engine
            .request(Request::molecule("analyze", acetone))
            .await?
            .analysis
            .ok_or("Missing analysis")?;
        assert_eq!(analysis.formula, "C3H6O");
        let mut propane = doc.clone();
        let next = propane.add_atom("C", Point::new(63., 36.373));
        propane.add_bond(end, next, 1, "plain");
        let (ketone, _) = atom_edit(&propane, end, "2", 42.).ok_or("Missing shortcut")??;
        assert_eq!(ketone.atoms.len(), 4);
        assert_eq!(
            engine
                .request(Request::molecule("analyze", ketone))
                .await?
                .analysis
                .ok_or("Missing analysis")?
                .formula,
            "C3H6O"
        );
        assert_eq!(doc.atoms.len(), 2);
        let (branched, _) = atom_edit(&doc, end, "K", 42.).ok_or("Missing tert-butyl")??;
        assert_eq!(branched.atoms.len(), 5);
        assert_eq!(
            engine
                .request(Request::molecule("analyze", branched))
                .await?
                .analysis
                .ok_or("Missing analysis")?
                .formula,
            "C5H12"
        );
        Ok(())
    }

    #[test]
    fn growth_respects_bond_length_and_focus_and_rejects_overvalence() -> Result<(), String> {
        let (doc, _, end) = ethane();
        for key in ["0", "1", "4", "5", "8", "z"] {
            let (candidate, focus) = atom_edit(&doc, end, key, 60.).ok_or("Missing growth")??;
            let new = candidate.atoms.last().ok_or("Missing new atom")?;
            let start = candidate.atom(end).ok_or("Missing start")?;
            assert!((start.position.distance(new.position) - 60.).abs() < 0.001);
            assert_eq!(focus, if key == "0" { end } else { new.id });
            candidate.validate()?;
        }
        let (candidate, focus) = atom_edit(&doc, end, "z", 42.).ok_or("Missing alkyne")??;
        let (extended, _) =
            atom_edit(&candidate, focus, "1", 42.).ok_or("Missing continuation")??;
        assert_eq!(extended.atoms.len(), 4);
        // An alkyne carbon already has valence 4 at the original endpoint.
        assert!(
            atom_edit(&candidate, end, "1", 42.)
                .ok_or("Missing growth")?
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn isotope_and_variables_preserve_their_distinct_semantics() -> Result<(), String> {
        let (doc, _, id) = ethane();
        let (isotope, _) = atom_edit(&doc, id, "d", 42.).ok_or("Missing deuterium")??;
        let atom = isotope.atom(id).ok_or("Missing isotope")?;
        assert_eq!((&*atom.element, atom.isotope), ("H", 2));
        for key in ["r", "x"] {
            let (variable, _) = atom_edit(&doc, id, key, 42.).ok_or("Missing variable")??;
            let atom = variable.atom(id).ok_or("Missing atom")?;
            assert_eq!(atom.element, "*");
            assert_eq!(atom.display.variable, Some(key.to_ascii_uppercase()));
        }
        Ok(())
    }

    #[test]
    fn rings_share_terminal_atoms_and_fuse_bonds_without_duplicate_vertices() -> Result<(), String>
    {
        let (doc, _, end) = ethane();
        for (key, size) in [("3", 6), ("6", 6), ("7", 5), ("v", 3), ("u", 4)] {
            let (candidate, _) =
                ring_edit(&doc, Some(end), None, key, 42.).ok_or("Missing ring")??;
            assert_eq!(candidate.atoms.len(), size + 1, "{key}");
            assert_eq!(candidate.bonds.len(), size + 1, "{key}");
            candidate.validate()?;
        }
        let (a, b) = doc
            .bonds
            .first()
            .map(|b| (b.a, b.b))
            .ok_or("Missing bond")?;
        for (key, size) in [
            ("v", 3),
            ("4", 4),
            ("5", 5),
            ("6", 6),
            ("7", 7),
            ("8", 8),
            ("9", 6),
            ("0", 6),
            ("a", 6),
            ("z", 5),
        ] {
            let (candidate, _) =
                ring_edit(&doc, None, Some((a, b)), key, 42.).ok_or("Missing fused ring")??;
            assert_eq!(candidate.atoms.len(), size, "{key}");
            assert_eq!(candidate.bonds.len(), size, "{key}");
            candidate.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod ring_angle_tests {
    use super::*;

    #[test]
    fn ring_hotkeys_bisect_the_incoming_bond_at_any_chain_rotation() -> Result<(), String> {
        for length in [24., 42., 73.] {
            for rotation in [0_f32, 7.3, 30., 67.5, 138., 223., 283., 345.] {
                for mirrored in [-1., 1.] {
                    let mut chain = Document::default();
                    let mut ids = Vec::new();
                    let mut point = Point::new(170., -80.);
                    for index in 0..4 {
                        ids.push(chain.add_atom("C", point));
                        let theta = (rotation + mirrored * if index % 2 == 0 { -30. } else { 30. })
                            .to_radians();
                        point = point.offset(length * theta.cos(), length * theta.sin());
                    }
                    for (index, pair) in ids.windows(2).enumerate() {
                        if let [a, b] = pair {
                            chain.add_bond(*a, *b, if index == 0 { 2 } else { 1 }, "plain");
                        }
                    }
                    let anchor = *ids.last().ok_or("Missing terminal atom")?;
                    let terminal = chain.atom(anchor).ok_or("Missing atom")?.position;
                    let neighbor = chain
                        .atoms
                        .iter()
                        .rev()
                        .nth(1)
                        .ok_or("Missing neighbor")?
                        .position;
                    let incoming = Point::new(neighbor.x - terminal.x, neighbor.y - terminal.y);
                    for (key, size) in [("3", 6), ("a", 6), ("6", 6), ("7", 5), ("v", 3), ("u", 4)]
                    {
                        let (placed, ring) = ring_edit(&chain, Some(anchor), None, key, length)
                            .ok_or("Missing ring key")??;
                        for atom in &chain.atoms {
                            assert_eq!(
                                placed.atom(atom.id).ok_or("Lost chain atom")?.position,
                                atom.position
                            );
                        }
                        let junctions: Vec<_> = placed
                            .bonds
                            .iter()
                            .filter_map(|b| {
                                let other = if b.a == anchor {
                                    b.b
                                } else if b.b == anchor {
                                    b.a
                                } else {
                                    return None;
                                };
                                ring.contains(&other).then_some(other)
                            })
                            .collect();
                        assert_eq!(junctions.len(), 2);
                        let expected_angle =
                            std::f32::consts::FRAC_PI_2 + std::f32::consts::PI / size as f32;
                        for id in junctions {
                            let p = placed.atom(id).ok_or("Missing ring neighbor")?.position;
                            let outgoing = Point::new(p.x - terminal.x, p.y - terminal.y);
                            let cosine = (incoming.x * outgoing.x + incoming.y * outgoing.y)
                                / (incoming.distance(Point::default())
                                    * outgoing.distance(Point::default()));
                            assert!(
                                (cosine - expected_angle.cos()).abs() < 0.0001,
                                "{key}, rotation {rotation}, length {length}, mirror {mirrored}: expected junction {expected_angle} rad, cosine {cosine}"
                            );
                        }
                        for bond in placed
                            .bonds
                            .iter()
                            .filter(|b| ring.contains(&b.a) && ring.contains(&b.b))
                        {
                            let a = placed.atom(bond.a).ok_or("Missing bond end")?.position;
                            let b = placed.atom(bond.b).ok_or("Missing bond end")?.position;
                            assert!((a.distance(b) - length).abs() < 0.001);
                        }
                        placed.validate()?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod drawing_quality_tests {
    use super::*;
    use crate::engine::{LocalEngine, Request};

    #[test]
    fn triple_edits_reject_overvalence_and_leave_cyclic_geometry_intact() -> Result<(), String> {
        let ring = crate::rings::Preset::Regular.document(42., false);
        let edge = ring.bonds.first().ok_or("Missing ring edge")?;
        let edited = bond_edit(&ring, edge.a, edge.b, BondPreset::Triple)?;
        for atom in &ring.atoms {
            assert_eq!(
                edited.atom(atom.id).ok_or("Missing atom")?.position,
                atom.position
            );
        }
        let mut crowded = ring.clone();
        let branch = crowded.add_atom("C", Point::new(100., 100.));
        crowded.add_bond(edge.a, branch, 1, "plain");
        let before = crowded.clone();
        assert!(bond_edit(&crowded, edge.a, edge.b, BondPreset::Triple).is_err());
        assert_eq!(crowded, before);
        assert!(
            atom_edit(&ring, edge.a, "z", 42.)
                .ok_or("Missing alkyne")?
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn straightening_keeps_a_remote_group_and_wedge_together() -> Result<(), String> {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        let b = doc.add_atom("C", Point::new(42., 0.));
        let c = doc.add_atom("C", Point::new(63., 36.373));
        let d = doc.add_atom("C", Point::new(105., 36.373));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        BondPreset::Wedge.place(&mut doc, c, d);
        doc = atom_text::apply(&doc, d, "Boc", Mode::Group)?;
        let before = doc.clone();
        let result = bond_edit(&doc, a, b, BondPreset::Triple)?;
        assert_eq!(result.abbreviations, before.abbreviations);
        for original in &before.bonds {
            let changed = result
                .bonds
                .iter()
                .find(|e| e.a == original.a && e.b == original.b)
                .ok_or("Lost bond")?;
            if original.a != a || original.b != b {
                assert_eq!(changed, original);
            }
            let distance = |doc: &Document| -> Result<f32, String> {
                Ok(doc
                    .atom(original.a)
                    .ok_or("Missing atom")?
                    .position
                    .distance(doc.atom(original.b).ok_or("Missing atom")?.position))
            };
            assert!((distance(&result)? - distance(&before)?).abs() < 0.001);
        }
        result.validate()?;
        Ok(())
    }

    #[tokio::test]
    async fn sulfonyl_replaces_terminal_and_internal_carbons_without_an_sh_label()
    -> Result<(), String> {
        let engine = LocalEngine::default();
        for terminal in [true, false] {
            let mut doc = Document::default();
            let left = doc.add_atom("C", Point::new(-36.373, -21.));
            let center = doc.add_atom("C", Point::default());
            doc.add_bond(left, center, 1, "plain");
            if !terminal {
                let right = doc.add_atom("C", Point::new(36.373, -21.));
                doc.add_bond(center, right, 1, "plain");
            }
            let (result, _) = atom_edit(&doc, center, "k", 42.).ok_or("Missing sulfonyl")??;
            assert_eq!(result.atom(center).ok_or("Missing sulfur")?.element, "S");
            assert_eq!(result.atoms.len(), 5);
            let response = engine.request(Request::molecule("analyze", result)).await?;
            assert_eq!(
                response.analysis.ok_or("Missing analysis")?.formula,
                "C2H6O2S"
            );
            assert_eq!(
                response
                    .document
                    .ok_or("Missing drawing")?
                    .atom(center)
                    .ok_or("Missing sulfur")?
                    .label_h,
                0
            );
        }
        Ok(())
    }

    #[test]
    fn branch_pairs_are_symmetric_and_tert_butyl_arms_have_even_angles() -> Result<(), String> {
        for rotation in [0_f32, 17.3, 90., 211.] {
            let mut doc = Document::default();
            let center = doc.add_atom("C", Point::default());
            for angle in [rotation + 210., rotation + 330.] {
                let theta = angle.to_radians();
                let other = doc.add_atom("C", Point::new(42. * theta.cos(), 42. * theta.sin()));
                doc.add_bond(center, other, 1, "plain");
            }
            let (gem, _) = atom_edit(&doc, center, "9", 42.).ok_or("Missing dimethyl")??;
            let added: Vec<_> = gem
                .atoms
                .iter()
                .filter(|a| doc.atom(a.id).is_none())
                .collect();
            assert_eq!(added.len(), 2);
            let expected = (rotation + 90.).to_radians();
            let sum = added.iter().fold(Point::default(), |p, a| {
                p.offset(a.position.x, a.position.y)
            });
            assert!((sum.x / sum.distance(Point::default()) - expected.cos()).abs() < 0.0001);
            assert!((sum.y / sum.distance(Point::default()) - expected.sin()).abs() < 0.0001);
            if let [a, b] = added.as_slice() {
                let cosine =
                    (a.position.x * b.position.x + a.position.y * b.position.y) / (42. * 42.);
                assert!((cosine - 0.5).abs() < 0.0001);
            }
            let mut ethane = Document::default();
            let c = ethane.add_atom("C", Point::default());
            let a = (rotation + 180.).to_radians();
            let other = ethane.add_atom("C", Point::new(42. * a.cos(), 42. * a.sin()));
            ethane.add_bond(c, other, 1, "plain");
            let (tbu, _) = atom_edit(&ethane, c, "K", 42.).ok_or("Missing tert-butyl")??;
            let mut angles: Vec<_> = tbu
                .atoms
                .iter()
                .filter(|a| a.id != c)
                .map(|a| {
                    a.position
                        .y
                        .atan2(a.position.x)
                        .rem_euclid(std::f32::consts::TAU)
                })
                .collect();
            angles.sort_by(f32::total_cmp);
            for (&a, &b) in angles
                .iter()
                .zip(angles.iter().cycle().skip(1))
                .take(angles.len())
            {
                assert!(
                    ((b - a).rem_euclid(std::f32::consts::TAU) - std::f32::consts::FRAC_PI_2).abs()
                        < 0.0001
                );
            }
        }
        Ok(())
    }

    #[test]
    fn isolated_ring_hotkeys_keep_the_requested_length() -> Result<(), String> {
        for element in ["C", "N"] {
            for length in [24., 60., 85.] {
                let mut doc = Document::default();
                let id = doc.add_atom(element, Point::default());
                for key in ["3", "6", "7", "v", "u"] {
                    let (result, _) =
                        ring_edit(&doc, Some(id), None, key, length).ok_or("Missing ring")??;
                    for b in &result.bonds {
                        let a = result.atom(b.a).ok_or("Missing endpoint")?.position;
                        let z = result.atom(b.b).ok_or("Missing endpoint")?.position;
                        assert!(
                            (a.distance(z) - length).abs() < 0.001,
                            "{key} on {element}: {} vs {length}",
                            a.distance(z)
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn triple_bond_edit_straightens_both_branches_preserving_lengths_and_remote_content()
    -> Result<(), String> {
        for rotation in [0_f32, 17.3, 83., 213.] {
            let mut doc = Document::default();
            let mut point = Point::default();
            let mut ids = Vec::new();
            for i in 0..6 {
                ids.push(doc.add_atom("C", point));
                let theta = (rotation + if i % 2 == 0 { -30. } else { 30. }).to_radians();
                point = point.offset(42. * theta.cos(), 42. * theta.sin());
            }
            for pair in ids.windows(2) {
                if let [a, b] = pair {
                    doc.add_bond(*a, *b, 1, "plain");
                }
            }
            let a = *ids.get(2).ok_or("Missing atom")?;
            let b = *ids.get(3).ok_or("Missing atom")?;
            let remote = doc.add_atom("O", Point::new(600., -400.));
            let original = doc.clone();
            let result = bond_edit(&doc, a, b, BondPreset::Triple)?;
            for id in [a, b, remote] {
                assert_eq!(
                    result.atom(id).ok_or("Missing atom")?.position,
                    doc.atom(id).ok_or("Missing atom")?.position
                );
            }
            for bond in &result.bonds {
                assert!(
                    (result
                        .atom(bond.a)
                        .ok_or("Missing atom")?
                        .position
                        .distance(result.atom(bond.b).ok_or("Missing atom")?.position)
                        - 42.)
                        .abs()
                        < 0.001
                );
            }
            for (index, other) in [(2, b), (3, a)] {
                let id = *ids.get(index).ok_or("Missing atom")?;
                let neighbor = *ids
                    .get(if index == 2 { 1 } else { 4 })
                    .ok_or("Missing neighbor")?;
                let p = result.atom(id).ok_or("Missing atom")?.position;
                let q = result.atom(other).ok_or("Missing atom")?.position;
                let r = result.atom(neighbor).ok_or("Missing atom")?.position;
                let cosine = ((q.x - p.x) * (r.x - p.x) + (q.y - p.y) * (r.y - p.y))
                    / (p.distance(q) * p.distance(r));
                assert!((cosine + 1.).abs() < 0.0001);
            }
            assert_eq!(doc, original);
        }
        Ok(())
    }
}
