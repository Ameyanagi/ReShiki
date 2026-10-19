//! Bonds meet the attachment atom's glyph inside a multi-atom label.
use super::*;
use crate::{
    atom_labels::LabelAttachment,
    atom_text::{Mode, apply},
};

/// A carbon bonded to a labeled atom from the left (`from_left`) or right.
fn labeled(text: &str, mode: Mode, from_left: bool) -> Result<(Document, u64), String> {
    let mut doc = Document::default();
    let id = doc.add_atom("C", Point::new(40., 20.));
    let other = doc.add_atom("C", Point::new(if from_left { -2. } else { 82. }, 20.));
    doc.add_bond(other, id, 1, "plain");
    Ok((apply(&doc, id, text, mode)?, id))
}

/// The drawn character whose advance spans the atom position.
fn glyph_at_atom(doc: &Document, id: u64) -> Result<char, String> {
    let atom = doc.atom(id).ok_or("Atom")?;
    for run in atom_label(atom, doc) {
        let Primitive::Text {
            position,
            text,
            size,
            style,
            ..
        } = run
        else {
            continue;
        };
        let mut x = position.x;
        for c in text.chars() {
            let advance = crate::style::glyph_metrics(c, &style).1 * size;
            if (x..x + advance).contains(&atom.position.x) {
                return Ok(c);
            }
            x += advance;
        }
    }
    Err("No glyph at the atom".into())
}

#[test]
fn text_labels_attach_bonds_to_their_attachment_atom() -> Result<(), String> {
    for (text, from_left, expected) in [
        ("SPh", true, 'S'),
        ("SPh", false, 'S'),
        ("AcS", false, 'S'),
        ("PhS", false, 'S'),
        ("MeO", false, 'O'),
        ("OMe", true, 'O'),
        ("OMe", false, 'O'),
        ("OAc", true, 'O'),
        ("OAc", false, 'O'),
        ("AcO", false, 'O'),
        ("CO2Me", true, 'C'),
        ("MeO2C", false, 'C'),
        ("BnO", false, 'O'),
        ("tBuS", false, 'S'),
        ("MeO+", false, 'O'),
        ("MeO +", false, 'O'),
        ("MeO 2+", false, 'O'),
    ] {
        let (doc, id) = labeled(text, Mode::Text, from_left)?;
        let side = if from_left { "left" } else { "right" };
        assert_eq!(
            glyph_at_atom(&doc, id)?,
            expected,
            "{text}, bond from {side}"
        );
    }
    Ok(())
}

#[test]
fn text_spelled_for_the_other_side_stays_centered_and_keeps_its_bond() -> Result<(), String> {
    // A bond from the left would have to cross Me or Ac to reach O or S.
    for (text, centered) in [("MeO", 'e'), ("AcS", 'c')] {
        let (doc, id) = labeled(text, Mode::Text, true)?;
        assert_eq!(glyph_at_atom(&doc, id)?, centered, "{text}");
        let bond = doc.bonds.first().ok_or("Bond")?;
        assert!(bond_segment(&doc, bond, None).is_some(), "{text}");
    }
    Ok(())
}

#[test]
fn automatic_internal_labels_stay_centered_and_keep_both_bonds() -> Result<(), String> {
    let mut doc = Document::default();
    let id = doc.add_atom("C", Point::new(0., 0.));
    let left = doc.add_atom("C", Point::new(-42., 0.));
    let right = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(left, id, 1, "plain");
    doc.add_bond(id, right, 1, "plain");
    let mut doc = apply(&doc, id, "MeO", Mode::Text)?;
    assert_eq!(glyph_at_atom(&doc, id)?, 'e');
    for bond in &doc.bonds {
        assert!(bond_segment(&doc, bond, None).is_some());
    }
    // An explicit choice still applies to an internal label.
    doc.atom_mut(id).ok_or("Atom")?.display.label_attachment = LabelAttachment::Right;
    assert_eq!(glyph_at_atom(&doc, id)?, 'O');
    Ok(())
}

#[test]
fn vertical_bonds_attach_at_the_first_atom() -> Result<(), String> {
    for text in ["MeO", "AcS"] {
        let mut doc = Document::default();
        let id = doc.add_atom("C", Point::new(40., 20.));
        let other = doc.add_atom("C", Point::new(40., 62.));
        doc.add_bond(other, id, 1, "plain");
        let doc = apply(&doc, id, text, Mode::Text)?;
        let expected = text.chars().last().ok_or("Text")?;
        assert_eq!(glyph_at_atom(&doc, id)?, expected, "{text}");
    }
    Ok(())
}

#[test]
fn abbreviations_attach_bonds_to_their_anchor_atom_in_both_directions() -> Result<(), String> {
    for (text, expected) in [("OAc", 'O'), ("CO2Me", 'C'), ("OMe", 'O')] {
        for from_left in [true, false] {
            let (doc, id) = labeled(text, Mode::Auto, from_left)?;
            assert!(doc.abbreviation(id).is_some(), "{text}");
            assert_eq!(glyph_at_atom(&doc, id)?, expected, "{text} {from_left}");
        }
    }
    Ok(())
}

#[test]
fn reversed_labels_without_a_reverse_spelling_keep_the_attachment_atom() {
    // Ac is also actinium; it is a nickname and never takes the bond.
    fn anchor(text: &str, left: bool) -> &str {
        &text[crate::abbreviations::anchor_range(text, left)]
    }
    assert_eq!(anchor("OAc", true), "O");
    assert_eq!(anchor("AcS", false), "S");
    assert_eq!(anchor("CO2Me", false), "C");
    assert_eq!(anchor("MeO2C", true), "C");
    assert_eq!(anchor("H3CO", true), "O");
    assert_eq!(anchor("Boc", true), "c");
}

#[test]
fn label_attachment_overrides_choose_flush_side_or_character() -> Result<(), String> {
    for (text, attachment, expected) in [
        ("MeO2C", LabelAttachment::Left, 'O'),
        ("MeO2C", LabelAttachment::Right, 'C'),
        ("MeO", LabelAttachment::Character(0), 'M'),
        ("MeO", LabelAttachment::Character(30), 'O'),
        ("SPh", LabelAttachment::Character(2), 'h'),
        ("Boc", LabelAttachment::Left, 'B'),
        ("MeO", LabelAttachment::Center, 'e'),
    ] {
        let (mut doc, id) = labeled(text, Mode::Text, false)?;
        doc.atom_mut(id).ok_or("Atom")?.display.label_attachment = attachment;
        assert_eq!(glyph_at_atom(&doc, id)?, expected, "{text} {attachment}");
    }
    Ok(())
}

#[test]
fn automatic_text_without_an_atom_stays_centered() -> Result<(), String> {
    let (doc, id) = labeled("Boc", Mode::Text, false)?;
    assert_eq!(glyph_at_atom(&doc, id)?, 'o');
    Ok(())
}

#[test]
fn bonds_trim_at_the_attachment_glyph_not_the_label_center() -> Result<(), String> {
    // MeO bonded from the right: the bond stops at O, beside the atom, rather
    // than at the right edge of a label centered over the atom.
    let (doc, id) = labeled("MeO", Mode::Text, false)?;
    let atom = doc.atom(id).ok_or("Atom")?;
    let (_, hi) = atom_label_ink_bounds(atom, &doc).ok_or("Ink")?;
    let size = STYLE.world(doc.drawing_style.text_style().size_pt);
    assert!(hi.x - atom.position.x < size * 0.6, "{hi:?}");
    let bond = doc.bonds.first().ok_or("Bond")?;
    let (start, end) = bond_segment(&doc, bond, None).ok_or("Visible bond")?;
    let label_end = if start.x > end.x { end } else { start };
    assert!(label_end.x > hi.x && label_end.x - hi.x < size * 0.4);
    Ok(())
}

#[test]
fn label_attachment_round_trips_in_native_files() -> Result<(), String> {
    for attachment in [
        LabelAttachment::Auto,
        LabelAttachment::Left,
        LabelAttachment::Right,
        LabelAttachment::Character(2),
    ] {
        let (mut doc, id) = labeled("MeO", Mode::Text, false)?;
        doc.atom_mut(id).ok_or("Atom")?.display.label_attachment = attachment;
        let bytes = doc.file_json()?;
        let json = String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?;
        assert_eq!(json.contains("label_attachment"), !attachment.is_auto());
        let restored = Document::from_native_file(&bytes)?;
        let atom = restored.atom(id).ok_or("Atom")?;
        assert_eq!(atom.display.label_attachment, attachment);
    }
    Ok(())
}

#[test]
fn changing_the_text_resets_a_character_attachment_only() {
    let mut display = crate::atom_labels::AtomDisplay {
        label_attachment: LabelAttachment::Character(2),
        ..Default::default()
    };
    display.variable = Some("MeO".into());
    // Formatting-only edits reapply the same text.
    display.set_variable(Some("MeO".into()));
    assert_eq!(display.label_attachment, LabelAttachment::Character(2));
    display.set_variable(Some("OMe".into()));
    assert_eq!(display.label_attachment, LabelAttachment::Auto);
    display.label_attachment = LabelAttachment::Right;
    display.set_variable(Some("MeO".into()));
    assert_eq!(display.label_attachment, LabelAttachment::Right);
    display.label_attachment = LabelAttachment::Character(32);
    assert!(display.validate().is_err());
}

#[test]
fn generic_labels_meet_a_horizontal_bond_with_their_near_edge() -> Result<(), String> {
    for text in ["[Cl,Br,I]", "[OMe,OPh]"] {
        for from_left in [true, false] {
            let (doc, id) = labeled(text, Mode::Auto, from_left)?;
            assert!(doc.generic.owns_label(id), "{text}");
            let atom = doc.atom(id).ok_or("Atom")?;
            let (lo, hi) = atom_label_ink_bounds(atom, &doc).ok_or("Ink")?;
            let bond = doc.bonds.first().ok_or("Bond")?;
            let (start, end) = bond_segment(&doc, bond, None).ok_or("Visible bond")?;
            let (near, far) = (start.x.min(end.x), start.x.max(end.x));
            let x = atom.position.x;
            if from_left {
                // The text extends right, away from the molecule.
                assert!(lo.x >= x - 0.01 && hi.x > x + 1., "{text}: {lo:?} {hi:?}");
                assert!(far < lo.x, "{text}: bond {near}..{far} enters {lo:?}");
            } else {
                assert!(hi.x <= x + 0.01 && lo.x < x - 1., "{text}: {lo:?} {hi:?}");
                assert!(near > hi.x, "{text}: bond {near}..{far} enters {hi:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn generic_labels_stay_centered_on_vertical_or_internal_bonds() -> Result<(), String> {
    let centered = |doc: &Document, id| -> Result<(), String> {
        let atom = doc.atom(id).ok_or("Atom")?;
        let (lo, hi) = atom_label_ink_bounds(atom, doc).ok_or("Ink")?;
        let x = atom.position.x;
        assert!((x - lo.x - (hi.x - x)).abs() < 2., "{lo:?} {hi:?}");
        for bond in &doc.bonds {
            assert!(bond_segment(doc, bond, None).is_some(), "{lo:?} {hi:?}");
        }
        Ok(())
    };
    let mut doc = Document::default();
    let id = doc.add_atom("C", Point::new(40., 20.));
    let other = doc.add_atom("C", Point::new(40., 62.));
    doc.add_bond(other, id, 1, "plain");
    centered(&apply(&doc, id, "[Cl,Br,I]", Mode::Auto)?, id)?;
    // A link node always has two bonds, so it never takes the one-bond side.
    for text in ["[Cl,Br,I]", "[CH2]1-3"] {
        let mut doc = Document::default();
        let id = doc.add_atom("C", Point::new(0., 0.));
        for x in [-120., 120.] {
            let other = doc.add_atom("C", Point::new(x, 0.));
            doc.add_bond(other, id, 1, "plain");
        }
        let doc = apply(&doc, id, text, Mode::Auto)?;
        assert!(doc.generic.owns_label(id), "{text}");
        centered(&doc, id)?;
    }
    Ok(())
}
