use super::*;

#[test]
fn group_hotkeys_attach_at_para_ring_ch_using_bond_growth() -> Result<(), String> {
    let ring = crate::rings::Preset::Benzene.document(42., false);
    let ipso = ring.atoms.first().ok_or("Missing ipso carbon")?.id;
    let para = ring.atoms.get(3).ok_or("Missing para carbon")?.id;
    let (source, _) = atom_edit(&ring, ipso, "1", 42.).ok_or("Missing bond key")??;
    let before = source.clone();
    let (grown, endpoint) = atom_edit(&source, para, "1", 42.).ok_or("Missing bond key")??;
    for (key, label, members) in [
        ("m", "Me", 1),
        ("e", "Et", 2),
        ("y", "Boc", 7),
        ("P", "Ph", 6),
        ("A", "Ac", 3),
        ("E", "CO2Me", 4),
        ("F", "CF3", 4),
        ("H", "Cbz", 10),
        ("N", "NO2", 3),
        ("O", "OMe", 2),
        ("Q", "Fmoc", 17),
        ("M", "MgBr", 2),
        ("Z", "N3", 3),
    ] {
        let (candidate, focus) =
            atom_edit(&source, para, key, 42.).ok_or("Missing group key")??;
        assert_eq!(focus, endpoint, "{key}");
        assert_ne!(focus, para, "{key}");
        let group = candidate.abbreviation(focus).ok_or("Missing group")?;
        assert_eq!(group.label, label);
        assert_eq!(group.members.len(), members, "{key}");
        assert_eq!(candidate.atoms.len(), 7 + members, "{key}");
        if matches!(key, "N" | "M" | "Z") {
            assert_eq!(candidate.bonds.len(), 7 + members, "{key}");
        }
        assert_eq!(
            candidate.atom(para).ok_or("Missing para carbon")?.element,
            "C"
        );
        for bond in &source.bonds {
            assert!(
                candidate.bonds.contains(bond),
                "{key}: lost an existing bond"
            );
        }
        for atom in &source.atoms {
            assert_eq!(
                candidate
                    .atom(atom.id)
                    .ok_or("Missing original atom")?
                    .position,
                atom.position
            );
        }
        assert_eq!(
            (candidate.clone(), focus),
            label_edit(&grown, endpoint, label, Mode::Group)?
        );
        candidate.validate()?;
    }
    assert_eq!(source, before);
    Ok(())
}

#[test]
fn group_hotkeys_attach_at_an_internal_chain_atom() -> Result<(), String> {
    let mut source = Document::default();
    let left = source.add_atom("C", Point::new(-36.373, -21.));
    let center = source.add_atom("C", Point::default());
    let right = source.add_atom("C", Point::new(36.373, -21.));
    source.add_bond(left, center, 1, "plain");
    source.add_bond(center, right, 1, "plain");
    for (key, label, members) in [("N", "NO2", 3), ("M", "MgBr", 2), ("Z", "N3", 3)] {
        let (candidate, focus) =
            atom_edit(&source, center, key, 42.).ok_or("Missing group key")??;
        assert_ne!(focus, center);
        assert_eq!(
            candidate.abbreviation(focus).ok_or("Missing group")?.label,
            label
        );
        assert_eq!(candidate.atoms.len(), 3 + members);
        assert_eq!(candidate.bonds.len(), 2 + members);
        assert_eq!(
            candidate
                .atom(center)
                .ok_or("Missing chain carbon")?
                .element,
            "C"
        );
        candidate.validate()?;
    }
    let refusal = "Group keys replace an endpoint or attach at an atom with a free H";
    let mut radical = source.clone();
    radical.add_atom("C", Point::new(0., 42.));
    let third = radical.atoms.last().ok_or("Missing branch")?.id;
    radical.add_bond(center, third, 1, "plain");
    radical
        .atom_mut(center)
        .ok_or("Missing chain carbon")?
        .radical_electrons = 1;
    let error = atom_edit(&radical, center, "N", 42.)
        .ok_or("Missing group key")?
        .unwrap_err();
    assert_eq!(error, refusal);
    for (no_implicit, explicit_h) in [(true, 0), (true, 2), (false, 2)] {
        let mut fixed = source.clone();
        let atom = fixed.atom_mut(center).ok_or("Missing chain carbon")?;
        atom.no_implicit = no_implicit;
        atom.explicit_h = explicit_h;
        let before = fixed.clone();
        for key in ["N", "M", "Z"] {
            let error = atom_edit(&fixed, center, key, 42.)
                .ok_or("Missing group key")?
                .unwrap_err();
            assert_eq!(error, refusal, "{key}");
            assert_eq!(fixed, before);
        }
    }
    Ok(())
}

#[test]
fn group_hotkeys_replace_endpoints_and_isolated_atoms() -> Result<(), String> {
    for attached in [false, true] {
        let mut source = Document::default();
        let target = source.add_atom("C", Point::default());
        if attached {
            let other = source.add_atom("C", Point::new(-42., 0.));
            source.add_bond(other, target, 1, "plain");
        }
        for key in [
            "m", "e", "y", "P", "A", "E", "F", "H", "N", "O", "Q", "M", "Z",
        ] {
            let (label, mode) = atom_label(key).ok_or("Missing group label")?;
            let result = atom_edit(&source, target, key, 42.).ok_or("Missing group key")??;
            assert_eq!(result.1, target);
            assert_eq!(result, label_edit(&source, target, label, mode)?);
        }
    }
    Ok(())
}

#[test]
fn saturated_atoms_and_groups_keep_the_same_plain_group_refusal() -> Result<(), String> {
    let mut saturated = Document::default();
    let center = saturated.add_atom("C", Point::default());
    for position in [
        Point::new(42., 0.),
        Point::new(0., 42.),
        Point::new(-42., 0.),
        Point::new(0., -42.),
    ] {
        let other = saturated.add_atom("C", position);
        saturated.add_bond(center, other, 1, "plain");
    }
    for collapsed in [false, true] {
        let mut source = saturated.clone();
        if collapsed {
            source.contract(&[center], "C", "")?;
        }
        let before = source.clone();
        let errors: Vec<_> = ["N", "M", "Z"]
            .into_iter()
            .map(|key| atom_edit(&source, center, key, 42.).unwrap().unwrap_err())
            .collect();
        assert_eq!(source, before);
        assert!(errors.iter().all(|error| error == &errors[0]), "{errors:?}");
        assert!(!errors[0].contains("Invalid abbreviation definition"));
    }
    Ok(())
}
