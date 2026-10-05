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
    let (extended, _) = atom_edit(&candidate, focus, "1", 42.).ok_or("Missing continuation")??;
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
fn rings_share_terminal_atoms_and_fuse_bonds_without_duplicate_vertices() -> Result<(), String> {
    let (doc, _, end) = ethane();
    for (key, size) in [("3", 6), ("6", 6), ("7", 5), ("v", 3), ("u", 4)] {
        let (candidate, _) = ring_edit(&doc, Some(end), None, key, 42.).ok_or("Missing ring")??;
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
