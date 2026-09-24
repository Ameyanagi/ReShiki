//! Transactional chemistry edits shared by contextual drawing shortcuts.
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

/// Returns None for an unassigned key. Errors leave the source unchanged.
pub fn atom_edit(
    doc: &Document,
    id: u64,
    key: &str,
    length: f32,
) -> Option<Result<(Document, u64), String>> {
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
                for _ in 0..count {
                    sprout(&mut result, center, "C", BondPreset::Single, length, false)?;
                }
                focus = center;
            }
            "k" => {
                // A sulfonyl group has a real sulfur and two double-bonded oxygens.
                let center = sprout(&mut result, id, "S", BondPreset::Single, length, false)?;
                for _ in 0..2 {
                    sprout(&mut result, center, "O", BondPreset::Double, length, false)?;
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
    if atom.no_implicit
        || atom.explicit_h != 0
        || atom.stereo.is_some()
        || templates::valence(doc, id) + order * 2 > capacity
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
    let proposed = editing::bond_extension(doc, start, Some(id), order as u8);
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
