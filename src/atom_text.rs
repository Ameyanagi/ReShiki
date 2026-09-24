//! Text at an atom endpoint: an element, a real collapsed group, or a named dummy.
use crate::{abbreviations::PRESETS, document::Document, editing::ELEMENTS};
fn known_group(text: &str) -> bool {
    PRESETS.contains(&text) || crate::common_groups::LABELS.contains(&text)
}
mod hydride;
pub use hydride::entry;

/// Unambiguous condensed spellings of existing structural definitions.
fn formula_group(text: &str) -> Option<(&'static str, &'static str)> {
    match text {
        "C2H5" | "C₂H₅" => Some(("Et", "H5C2")),
        "CH2CH3" | "CH₂CH₃" => Some(("Et", "H3CH2C")),
        "OCH3" | "OCH₃" => Some(("OMe", "H3CO")),
        "OC2H5" | "OC₂H₅" => Some(("OEt", "H5C2O")),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Auto,
    Text,
    Group,
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic",
            Self::Text => "Text label (dummy atom)",
            Self::Group => "Chemical abbreviation",
        })
    }
}
pub fn description(text: &str, mode: Mode) -> &'static str {
    if mode == Mode::Text {
        "Text on a dummy atom; existing bonds stay connected."
    } else if crate::ligands::LABELS.contains(&text.trim()) {
        "A real Cp (C5H5−) or Cp* (C10H15−) ligand with a five-center attachment. Metal charge stays as entered."
    } else if formula_group(text.trim()).is_some()
        || mode == Mode::Group
        || known_group(text.trim()) && !ELEMENTS.contains(&text.trim())
    {
        "A real chemical group. Expand it later from Abbreviations. Requires a single-bond endpoint."
    } else if hydride::parse(text.trim()).is_ok_and(|h| h.is_some()) {
        "An atom with the entered hydrogen count. Checking chemistry will not rewrite this label. Type the element alone to restore automatic hydrogens."
    } else if ELEMENTS.contains(&text.trim()) || text.trim() == "*" {
        "An element symbol (or * for a dummy atom)."
    } else {
        "Text on a dummy atom; existing bonds stay connected."
    }
}

/// Returns a complete candidate; failures never partially modify the drawing.
pub fn apply(doc: &Document, id: u64, text: &str, mode: Mode) -> Result<Document, String> {
    if text.chars().any(char::is_control) {
        return Err("Use a single line of printable text".into());
    }
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 32 {
        return Err("Enter 1–32 characters for the atom label".into());
    }
    let atom = doc.atom(id).ok_or("The atom is no longer available")?;
    if mode != Mode::Text && crate::ligands::LABELS.contains(&text) {
        return crate::ligands::replace(doc, id, text);
    }
    if !atom.centroid.is_empty() {
        return Err(
            "This is a tracked attachment point. Edit the atom bonded to it instead.".into(),
        );
    }
    let element = ELEMENTS.contains(&text) || text == "*";
    let formula = (mode != Mode::Text).then(|| formula_group(text)).flatten();
    if mode == Mode::Group
        && !known_group(text)
        && formula.is_none()
        && let Some(group) = doc.abbreviation(id)
    {
        let mut result = doc.clone();
        let renamed = result
            .abbreviations
            .iter_mut()
            .find(|g| g.anchor == id)
            .ok_or("Missing group")?;
        if group.label != text {
            renamed.label = text.into();
            renamed.reverse_label.clear();
        }
        result.validate()?;
        return Ok(result);
    }
    if mode == Mode::Group
        || formula.is_some()
        || mode == Mode::Auto && !element && known_group(text)
    {
        let selected = doc
            .abbreviation(id)
            .map(|a| a.members.clone())
            .unwrap_or_else(|| vec![id]);
        if doc.abbreviation(id).is_some_and(|a| a.label == text) {
            return Ok(doc.clone());
        }
        let mut result = crate::chemistry::abbreviations::replace(
            doc,
            &selected,
            formula.map(|f| f.0).unwrap_or(text),
        )
        .map_err(|e| format!("{e}. Choose Text label to use the text without a chemical group."))?;
        // The template is planar; retain the original anchor's position and depth.
        let anchor = result
            .atom(id)
            .ok_or("The abbreviation has no attachment atom")?;
        let shift = crate::document::Point::new(
            atom.position.x - anchor.position.x,
            atom.position.y - anchor.position.y,
        );
        let members = result
            .abbreviation(id)
            .ok_or("The abbreviation is unavailable")?
            .members
            .clone();
        for a in result.atoms.iter_mut().filter(|a| members.contains(&a.id)) {
            a.position = a.position.offset(shift.x, shift.y);
            a.depth = atom.depth;
        }
        if let Some((_, reverse)) = formula
            && let Some(group) = result.abbreviations.iter_mut().find(|g| g.anchor == id)
        {
            group.label = text.into();
            group.reverse_label = reverse.into();
        }
        result.invalidate_chemistry(&selected);
        result.validate()?;
        return Ok(result);
    }
    if doc.abbreviations.iter().any(|a| a.members.contains(&id)) {
        return Err(
            "Expand this abbreviation before replacing an individual atom with text or an element."
                .into(),
        );
    }
    let explicit = if mode != Mode::Text && !element {
        hydride::parse(text)?
    } else {
        None
    };
    let new_element = if let Some(h) = &explicit {
        h.element.as_str()
    } else if mode != Mode::Text && element {
        text
    } else {
        "*"
    };
    let variable =
        (new_element == "*" && (text != "*" || mode == Mode::Text)).then(|| text.to_owned());
    let mut result = doc.clone();
    let a = result
        .atom_mut(id)
        .ok_or("The atom is no longer available")?;
    let previous = (a.element.clone(), a.charge, a.explicit_h, a.no_implicit);
    if a.element != new_element {
        a.element = new_element.into();
        a.charge = 0;
        a.isotope = 0;
        a.explicit_h = 0;
        a.label_h = 0;
        a.no_implicit = new_element == "*";
        a.radical_electrons = 0;
        a.stereo = None;
    }
    a.display.variable = variable;
    if let Some(h) = &explicit {
        a.explicit_h = h.hydrogens;
        a.no_implicit = true;
        a.label_h = h.hydrogens;
        a.charge = h.charge;
    } else if mode != Mode::Text && element && new_element != "*" {
        a.explicit_h = 0;
        a.no_implicit = false;
    }
    if new_element == "*" {
        a.no_implicit = true;
        a.explicit_h = 0;
        a.label_h = 0;
    }
    if new_element == "C" {
        a.display.carbons = Some(crate::atom_labels::Carbons::All);
    }
    if previous != (a.element.clone(), a.charge, a.explicit_h, a.no_implicit) {
        result.invalidate_chemistry(&[id]);
    }
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Point;

    #[test]
    fn free_labels_retain_connections_style_coordinates_and_roundtrip()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut doc = Document::default();
        let metal = doc.add_atom("Cu", Point::new(0., 0.));
        let ligand = doc.add_atom("N", Point::new(60., 0.));
        doc.add_bond(ligand, metal, 5, "plain");
        let original = doc.clone();
        for label in ["M", "L", "X", "R₁", "custom ligand", "Boc"] {
            let mode = if label == "Boc" {
                Mode::Text
            } else {
                Mode::Auto
            };
            let named = apply(&doc, metal, label, mode)?;
            assert_eq!(named.bonds, original.bonds);
            assert_eq!(
                named.atom(metal).ok_or("Missing atom")?.position,
                Point::new(0., 0.)
            );
            assert_eq!(named.atom(metal).ok_or("Missing atom")?.element, "*");
            assert_eq!(
                named
                    .atom(metal)
                    .ok_or("Missing atom")?
                    .display
                    .variable
                    .as_deref(),
                Some(label)
            );
            assert!(named.atom(metal).ok_or("Missing atom")?.no_implicit);
            assert!(crate::scene::svg(&named).contains(label));
            let saved = serde_json::to_string(&named)?;
            let loaded: Document = serde_json::from_str(&saved)?;
            loaded.validate()?;
            assert_eq!(loaded, named);
            let copy = crate::editing::selection(&named, &[metal, ligand]);
            assert_eq!(
                copy.atom(metal)
                    .ok_or("Missing atom")?
                    .display
                    .variable
                    .as_deref(),
                Some(label)
            );
            let restored = apply(&named, metal, "Fe", Mode::Auto)?;
            assert_eq!(restored.atom(metal).ok_or("Missing atom")?.element, "Fe");
            assert!(
                restored
                    .atom(metal)
                    .ok_or("Missing atom")?
                    .display
                    .variable
                    .is_none()
            );
            assert_eq!(restored.bonds, original.bonds);
        }
        assert_eq!(doc, original);
        Ok(())
    }

    #[test]
    fn boc_is_a_real_expandable_group_and_invalid_input_is_atomic()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut doc = Document::default();
        let n = doc.add_atom("N", Point::new(0., 0.));
        let endpoint = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(n, endpoint, 1, "plain");
        let before = doc.clone();
        let boc = apply(&doc, endpoint, "Boc", Mode::Auto)?;
        assert!(boc.atoms.len() > 2);
        let group = boc.abbreviation(endpoint).ok_or("Missing abbreviation")?;
        assert_eq!(group.label, "Boc");
        assert_eq!(
            boc.atom(endpoint).ok_or("Missing atom")?.position,
            doc.atom(endpoint).ok_or("Missing atom")?.position
        );
        assert!(boc.bonds.contains(doc.bonds.first().ok_or("Missing bond")?));
        crate::chemistry::document::prepare(&boc)?;
        let mut expanded = boc.clone();
        assert_eq!(expanded.expand_abbreviations(&[endpoint]), 1);
        assert_eq!(expanded.atoms, boc.atoms);
        assert_eq!(expanded.bonds, boc.bonds);
        assert!(apply(&boc, endpoint, "M", Mode::Auto).is_err());
        for label in ["", "  ", "X\nY", "abcdefghijklmnopqrstuvwxyz1234567"] {
            assert!(apply(&doc, endpoint, label, Mode::Text).is_err());
        }
        assert_eq!(doc, before);
        doc.add_bond(n, endpoint, 2, "plain");
        assert!(apply(&doc, endpoint, "Boc", Mode::Auto).is_err());
        assert!(apply(&doc, endpoint, "Boc", Mode::Text).is_ok());
        Ok(())
    }
}
