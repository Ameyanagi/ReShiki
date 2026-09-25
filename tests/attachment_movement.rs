use reshiki::{
    attachments,
    document::{Document, Point},
    ligands, projection,
};

#[test]
fn moving_an_expanded_cp_star_point_carries_methyls_but_not_metal_or_second_ligand()
-> Result<(), String> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Fe", Point::default());
    let first = doc.add_atom("C", Point::new(-100., 0.));
    let second = doc.add_atom("C", Point::new(100., 0.));
    doc.add_bond(first, metal, 1, "plain");
    doc.add_bond(second, metal, 1, "plain");
    doc = ligands::replace(&doc, first, "Cp*")?;
    doc = ligands::replace(&doc, second, "Cp*")?;
    let first_members = doc.abbreviation(first).ok_or("Cp*")?.members.clone();
    doc.expand_abbreviations(&[first, second]);
    projection::tilt(&mut doc, &first_members, 50., false);
    let before = doc.clone();
    let selection = attachments::movement_selection(&doc, &[first]);
    assert_eq!(selection.len(), 11);
    assert!(first_members.iter().all(|id| selection.contains(id)));
    assert!(!selection.contains(&metal) && !selection.contains(&second));
    doc.translate(&selection, 13., -27.);
    for atom in &before.atoms {
        let moved = doc.atom(atom.id).ok_or("Atom")?;
        assert_eq!(moved.depth, atom.depth);
        assert_eq!(moved.centroid, atom.centroid);
        assert_eq!(
            moved.position,
            if selection.contains(&atom.id) {
                atom.position.offset(13., -27.)
            } else {
                atom.position
            }
        );
    }
    assert_eq!(doc.bonds, before.bonds);
    doc.validate()?;
    // The explicit point-only path leaves the ligand exactly where it was.
    let mut point_only = before.clone();
    point_only.translate(&[first], 12., 18.);
    for atom in &before.atoms {
        if atom.id != first {
            assert_eq!(point_only.atom(atom.id), Some(atom));
        }
    }
    Ok(())
}

#[test]
fn drawing_centroid_moves_with_members_and_an_ordinary_atom_does_not_expand() -> Result<(), String>
{
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-21., 0.));
    let b = doc.add_atom("C", Point::new(21., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = projection::add_centroid(&mut doc, &[a, b])?;
    let ids = attachments::movement_selection(&doc, &[c]);
    assert_eq!(ids, vec![a, b, c]);
    doc.translate(&ids, 30., 40.);
    assert_eq!(
        doc.atom(c).ok_or("centroid")?.position,
        Point::new(30., 40.)
    );
    assert_eq!(attachments::movement_selection(&doc, &[a]), vec![a]);
    doc.validate()
}
