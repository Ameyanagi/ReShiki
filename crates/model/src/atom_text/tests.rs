use super::*;
use crate::document::Point;

#[test]
fn typed_charges_follow_element_hydride_text_and_link_grammars() -> Result<(), String> {
    enum Expected {
        Element(&'static str, u32, i32, &'static str),
        Dummy,
        Link,
        Error(&'static str),
    }
    use Expected::{Dummy, Element, Error, Link};
    let mut source = Document::default();
    let left = source.add_atom("C", Point::default());
    let id = source.add_atom("C", Point::new(42., 0.));
    let right = source.add_atom("C", Point::new(84., 0.));
    source.add_bond(left, id, 1, "plain");
    source.add_bond(id, right, 1, "plain");
    for (text, expected) in [
        ("Pd2+", Element("Pd", 0, 2, "Pd2+")),
        ("Pd+2", Element("Pd", 0, 2, "Pd2+")),
        ("Fe3+", Element("Fe", 0, 3, "Fe3+")),
        ("Na+", Element("Na", 0, 1, "Na+")),
        ("Cl-", Element("Cl", 0, -1, "Cl-")),
        ("O2-", Element("O", 0, -2, "O2-")),
        ("Pd²⁺", Element("Pd", 0, 2, "Pd2+")),
        ("NH4+", Element("N", 4, 1, "NH4+")),
        ("OH-", Element("O", 1, -1, "OH-")),
        (
            "NH42+",
            Error("Write the charge after the sign, for example NH4+2"),
        ),
        ("Pd9+", Error("Enter a charge from -8 to +8")),
        ("SO4 2-", Dummy),
        ("CH2 1-3", Link),
        ("M2+", Dummy),
    ] {
        let result = apply(&source, id, text, Mode::Auto);
        if let Error(message) = expected {
            assert_eq!(result, Err(message.into()), "{text}");
            continue;
        }
        let result = result?;
        let atom = result.atom(id).ok_or("Missing atom")?;
        assert_eq!(result.bonds, source.bonds, "{text}");
        match expected {
            Element(element, hydrogens, charge, canonical) => {
                assert_eq!(atom.element, element, "{text}");
                assert_eq!(atom.charge, charge, "{text}");
                assert!(atom.no_implicit, "{text}");
                assert_eq!(atom.explicit_h, hydrogens, "{text}");
                // label_h is computed; the edit clears it until the next check.
                assert_eq!(atom.label_h, 0, "{text}");
                assert!(atom.display.variable.is_none(), "{text}");
                assert!(result.generic.links.is_empty(), "{text}");
                assert_eq!(entry(atom), canonical, "{text}");
                assert_eq!(
                    apply(&source, id, &entry(atom), Mode::Auto)?,
                    result,
                    "{text}"
                );
            }
            Dummy => {
                assert_eq!(atom.element, "*", "{text}");
                assert_eq!(atom.charge, 0, "{text}");
                assert_eq!(atom.display.variable.as_deref(), Some(text));
                assert!(result.generic.links.is_empty(), "{text}");
            }
            Link => assert_eq!(
                result.generic.links,
                vec![crate::generic::LinkNode {
                    atom: id,
                    min: 1,
                    max: 3,
                }]
            ),
            Error(_) => unreachable!(),
        }
    }
    Ok(())
}

#[test]
fn charged_hydrogen_and_negative_hydrides_reopen_as_the_same_atom() -> Result<(), String> {
    let mut source = Document::default();
    let left = source.add_atom("C", Point::default());
    let id = source.add_atom("C", Point::new(42., 0.));
    source.add_bond(left, id, 1, "plain");
    for (text, canonical) in [
        ("H+", "H+"),
        ("H-", "H-"),
        ("NH4−2", "NH4-2"),
        ("CH2⁻³", "CH2-3"),
        ("NH12-1", "NH12-1"),
    ] {
        let typed = apply(&source, id, text, Mode::Auto)?;
        let atom = typed.atom(id).ok_or("Missing atom")?;
        assert!(atom.no_implicit, "{text}");
        assert!(typed.generic.links.is_empty(), "{text}");
        assert_eq!(entry(atom), canonical, "{text}");
        assert_eq!(apply(&source, id, canonical, Mode::Auto)?, typed, "{text}");
    }
    // Hydrogen keeps its bare entry when it has no typed charge.
    let neutral = apply(&source, id, "H", Mode::Auto)?;
    assert_eq!(entry(neutral.atom(id).ok_or("Missing H")?), "H");
    Ok(())
}

#[test]
fn charged_label_shapes_normalization_and_existing_errors() -> Result<(), String> {
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    for (text, element, hydrogens, charge) in [
        ("Co2+", "Co", 0, 2),
        ("Hf2+", "Hf", 0, 2),
        ("Hg+", "Hg", 0, 1),
        ("Pd₂⁺", "Pd", 0, 2),
        ("Pd⁺²", "Pd", 0, 2),
        ("O⁻²", "O", 0, -2),
        ("Cl−", "Cl", 0, -1),
        ("Pd+8", "Pd", 0, 8),
        ("O8-", "O", 0, -8),
        ("NH4+2", "N", 4, 2),
        ("NH12+1", "N", 12, 1),
        ("H3O+", "O", 3, 1),
        ("Hf", "Hf", 0, 0),
        ("He", "He", 0, 0),
        ("H+", "H", 0, 1),
        ("H-", "H", 0, -1),
        ("H⁺", "H", 0, 1),
        ("NH4−2", "N", 4, -2),
        ("NH4-2", "N", 4, -2),
        ("CH2-3", "C", 2, -3),
        ("NH12-1", "N", 12, -1),
    ] {
        let result = apply(&source, id, text, Mode::Auto)?;
        let atom = result.atom(id).ok_or("Missing atom")?;
        assert_eq!(atom.element, element, "{text}");
        assert_eq!(atom.explicit_h, hydrogens, "{text}");
        assert_eq!(atom.charge, charge, "{text}");
        assert_eq!(
            apply(&source, id, &entry(atom), Mode::Auto)?,
            result,
            "{text}"
        );
    }
    for text in [
        "Pd2+2", "Pd++", "Pd 2+", "Na+K", "A+B", "CN-", "*2+", "R-X", "ligand-1",
    ] {
        let result = apply(&source, id, text, Mode::Auto)?;
        let atom = result.atom(id).ok_or("Missing atom")?;
        assert_eq!(atom.element, "*", "{text}");
        assert_eq!(atom.display.variable.as_deref(), Some(text));
    }
    for text in [
        "Pd+9",
        "Pd0+",
        "Pd+0",
        "Pd⁰⁺",
        "C60+",
        "Pd999999999999999999999+",
        "Pd+999999999999999999999",
        "NH3+9",
    ] {
        assert_eq!(
            apply(&source, id, text, Mode::Auto),
            Err("Enter a charge from -8 to +8".into()),
            "{text}"
        );
    }
    for text in ["CH32+", "NH4²⁺", "NH12-"] {
        assert_eq!(
            apply(&source, id, text, Mode::Auto),
            Err("Write the charge after the sign, for example NH4+2".into()),
            "{text}"
        );
    }
    assert_eq!(
        apply(&source, id, "NH3+foo", Mode::Auto),
        Err("Use a charge such as +, - or +2".into())
    );
    assert_eq!(
        apply(&source, id, "NH256", Mode::Auto),
        Err("Hydrogen count must be at most 255".into())
    );
    Ok(())
}

#[test]
fn charged_elements_have_specific_guidance_and_text_mode_stays_literal() -> Result<(), String> {
    const CHARGED: &str = "An element with the typed charge. Hydrogens are not added; type the element alone to restore automatic hydrogens.";
    const HYDRIDE: &str = "An atom with the entered hydrogen count. Checking chemistry will not rewrite this label. Type the element alone to restore automatic hydrogens.";
    assert_eq!(description("Pd2+", Mode::Auto), CHARGED);
    assert_eq!(description("NH4+", Mode::Auto), HYDRIDE);
    assert_eq!(description("NH0+", Mode::Auto), HYDRIDE);
    let mut source = Document::default();
    let id = source.add_atom("Pd", Point::default());
    let result = apply(&source, id, "Pd2+", Mode::Text)?;
    let atom = result.atom(id).ok_or("Missing atom")?;
    assert_eq!(atom.element, "*");
    assert_eq!(atom.charge, 0);
    assert_eq!(atom.display.variable.as_deref(), Some("Pd2+"));
    assert_eq!(
        description("Pd2+", Mode::Text),
        "Text on a dummy atom; existing bonds stay connected."
    );
    Ok(())
}

#[test]
fn text_and_unrecognized_automatic_entries_show_scripts_without_assigning_chemistry()
-> Result<(), String> {
    let mut source = Document::default();
    let left = source.add_atom("C", Point::default());
    let id = source.add_atom("Pd", Point::new(42., 0.));
    let right = source.add_atom("C", Point::new(84., 0.));
    source.add_bond(left, id, 1, "plain");
    source.add_bond(id, right, 1, "plain");
    for (text, mode, core, suffix, superscript) in [
        ("Pd2+", Mode::Text, "Pd", "2+", true),
        ("M2+", Mode::Auto, "M", "2+", true),
        ("R1", Mode::Auto, "R", "1", false),
        ("L2", Mode::Auto, "L", "2", false),
    ] {
        let result = apply(&source, id, text, mode)?;
        let atom = result.atom(id).ok_or("Atom")?;
        assert_eq!(atom.element, "*", "{text}");
        assert_eq!(atom.charge, 0, "{text}");
        assert_eq!(atom.explicit_h, 0, "{text}");
        assert!(atom.no_implicit, "{text}");
        assert_eq!(atom.display.variable.as_deref(), Some(text));
        assert_eq!(
            atom.position,
            source.atom(id).ok_or("Original atom")?.position
        );
        assert_eq!(result.bonds, source.bonds, "{text}");
        assert!(result.abbreviations.is_empty(), "{text}");
        assert!(result.generic.is_empty(), "{text}");
        assert_eq!(entry(atom), text);
        let runs: Vec<_> = crate::scene::primitives(&result)
            .into_iter()
            .filter_map(|primitive| match primitive {
                crate::scene::Primitive::Text {
                    text,
                    size,
                    position,
                    ..
                } => Some((text, size, position)),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 2, "{text}: {runs:?}");
        let normal = runs.first().ok_or("Normal run")?;
        let script = runs.last().ok_or("Script run")?;
        assert_eq!(normal.0, core, "{text}");
        assert_eq!(script.0, suffix, "{text}");
        assert_eq!(normal.1, crate::style::DEFAULT.font_size(), "{text}");
        assert!(script.1 < normal.1, "{text}");
        assert_eq!(script.2.y < normal.2.y, superscript, "{text}");
        if !superscript {
            assert!(script.2.y > normal.2.y, "{text}");
        }
    }
    Ok(())
}

#[test]
fn charged_entry_preserves_implicit_hydrogens_and_imported_isotopes() -> Result<(), String> {
    for element in ["N", "Pt", "H"] {
        let mut source = Document::default();
        let id = source.add_atom(element, Point::default());
        let atom = source.atom_mut(id).ok_or("Missing atom")?;
        atom.charge = 1;
        assert_eq!(entry(atom), element);
        atom.no_implicit = true;
        atom.isotope = 2;
        let text = entry(atom);
        assert_eq!(text, format!("{element}+"));
        assert_eq!(apply(&source, id, &text, Mode::Auto)?, source);
        let atom = source.atom_mut(id).ok_or("Missing atom")?;
        atom.charge = 0;
        assert_eq!(
            entry(atom),
            if element == "H" {
                "H".into()
            } else {
                format!("{element}H0")
            }
        );
    }
    Ok(())
}

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
        // A label such as R₁ can span multiple font runs when the requested
        // face lacks a glyph. Check visible text, not contiguous XML bytes.
        let svg = crate::scene::svg(&named);
        let xml = roxmltree::Document::parse(&svg)?;
        let rendered: String = xml
            .descendants()
            .filter(|node| node.has_tag_name("text"))
            .filter_map(|node| node.text())
            .collect();
        assert!(rendered.contains(label), "Missing label {label}: {svg}");
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
