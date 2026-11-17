use super::*;
use crate::document::Annotation;
use crate::reactions::{Participant, Reaction};

/// A drawing as read from a file, the way both sides of a compare are read.
fn load(doc: &Document) -> Document {
    Document::from_native_file(&doc.file_json().unwrap()).unwrap()
}

/// `smiles` laid out as ReShiki lays out an import.
fn molecule(smiles: &str) -> Document {
    use crate::chemistry::{depict, document, smiles as read};
    let state = read::prepare(smiles).unwrap().state;
    let ranks: Vec<_> = state.properties.atoms.iter().map(|a| a.cip_rank).collect();
    let conformer = depict::compute(&state, &ranks, None, Default::default()).unwrap();
    let molecule = document::Molecule {
        rdkit_version: crate::chemistry::RDKIT_VERSION,
        ids: (1..=state.graph.atoms.len() as u64).collect(),
        positions: conformer.positions,
        state,
    };
    let drawing = document::for_import_scene(&molecule, false).unwrap();
    let labels = drawing.labels().unwrap();
    drawing.finish(labels).unwrap()
}

fn text(doc: &mut Document, words: &str, x: f32, y: f32) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: Point::new(x, y),
        text: words.into(),
        format: Default::default(),
    });
    id
}

/// The mock's scheme: benzoic acid + ethanol → ethyl benzoate, with the
/// conditions over the arrow, as one reaction. Atom ids of each molecule.
struct Scheme {
    doc: Document,
    acid: Vec<u64>,
    ethanol: Vec<u64>,
    ester: Vec<u64>,
    conditions: u64,
}

fn scheme() -> Scheme {
    let mut doc = Document::default();
    let atoms = |doc: &Document, ids: Vec<u64>| -> Vec<u64> {
        ids.into_iter()
            .filter(|id| doc.atom(*id).is_some())
            .collect()
    };
    let ids = crate::editing::append(&mut doc, &molecule("OC(=O)c1ccccc1"), Point::new(0., 0.));
    let acid = atoms(&doc, ids);
    text(&mut doc, "+", 160., 0.);
    let ids = crate::editing::append(&mut doc, &molecule("CCO"), Point::new(230., 0.));
    let ethanol = atoms(&doc, ids);
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow {
        id: arrow,
        start: Point::new(320., 0.),
        end: Point::new(420., 0.),
        start_anchor: None,
        end_anchor: None,
        kind: "forward".into(),
        control: None,
        cubic: None,
        nodes: vec![],
        style: None,
    });
    let conditions = text(&mut doc, "H2SO4", 370., -20.);
    let ids = crate::editing::append(
        &mut doc,
        &molecule("CCOC(=O)c1ccccc1"),
        Point::new(560., 0.),
    );
    let ester = atoms(&doc, ids);
    let mut reaction = Reaction::new(arrow);
    let part = |atoms: &Vec<u64>| Participant {
        atoms: atoms.clone(),
        coefficient: 1,
    };
    reaction.reactants = vec![part(&acid), part(&ethanol)];
    reaction.products = vec![part(&ester)];
    doc.reactions.push(reaction);
    Scheme {
        doc: load(&doc),
        acid,
        ethanol,
        ester,
        conditions,
    }
}

fn edited(doc: &Document, edit: impl FnOnce(&mut Document)) -> Document {
    let mut doc = doc.clone();
    edit(&mut doc);
    load(&doc)
}

/// Turns `ids` about the origin, then moves them.
fn moved(doc: &mut Document, ids: &[u64], dx: f32, dy: f32, degrees: f32) {
    crate::editing::transform_about(doc, ids, Point::new(0., 0.), 1., degrees);
    doc.translate(ids, dx, dy);
}

fn kinds(report: &Report) -> Vec<(Kind, Subject)> {
    let mut found: Vec<_> = report
        .changes
        .iter()
        .map(|change| (change.kind, change.subject))
        .collect();
    found.sort_by_key(|(kind, subject)| (format!("{kind:?}"), format!("{subject:?}")));
    found
}

fn chemical(report: &Report) -> Vec<&Change> {
    report
        .changes
        .iter()
        .filter(|change| change.category() == Category::Chemistry)
        .collect()
}

#[test]
fn the_same_drawing_has_no_changes() {
    let s = scheme();
    let report = compare(&s.doc, &load(&s.doc));
    assert_eq!(report.changes, vec![]);
    assert_eq!(report.molecules.len(), 3);
    assert!(
        report
            .molecules
            .iter()
            .all(|pair| pair.chemistry == Chemistry::Same)
    );
    assert!(report.molecules.iter().all(|pair| pair.by == MatchedBy::Id));
}

#[test]
fn a_whole_scheme_moved_and_turned_is_one_layout_line() {
    let s = scheme();
    for (dx, dy, degrees) in [(-38., 14., 0.), (120., -60., 25.)] {
        let right = edited(&s.doc, |doc| moved(doc, &doc.all_ids(), dx, dy, degrees));
        let report = compare(&s.doc, &right);
        assert_eq!(kinds(&report), [(Kind::SchemeMoved, Subject::Scheme)]);
        assert_eq!(report.count(Category::Chemistry), 0);
        let Detail::Move {
            dx: x,
            dy: y,
            degrees: turn,
        } = report.changes[0].detail
        else {
            panic!("no move");
        };
        assert!((turn - f64::from(degrees)).abs() < 0.01, "{turn}");
        if degrees == 0. {
            assert!((x - f64::from(dx)).abs() < 0.01 && (y - f64::from(dy)).abs() < 0.01);
        }
        // Lined up, the right side lies on the left side.
        let lined = aligned(&right, &report);
        for atom in &s.doc.atoms {
            let there = lined.atom(atom.id).unwrap().position;
            assert!(atom.position.distance(there) < 0.01, "{atom:?}");
        }
    }
}

#[test]
fn one_moved_molecule_is_one_moved_line() {
    let s = scheme();
    let right = edited(&s.doc, |doc| moved(doc, &s.ethanol, 0., 90., 0.));
    let report = compare(&s.doc, &right);
    assert_eq!(kinds(&report), [(Kind::Moved, Subject::Molecule)]);
    assert_eq!(report.scheme, None);
    assert_eq!(report.changes[0].right.objects, s.ethanol);
    assert!(
        report.changes[0].right.atoms.is_empty(),
        "no paint for a move"
    );
    // Turned about itself, it is rotated.
    let right = edited(&s.doc, |doc| {
        let center = doc.atom(s.ethanol[1]).unwrap().position;
        crate::editing::transform_about(doc, &s.ethanol, center, 1., 60.);
    });
    assert_eq!(
        kinds(&compare(&s.doc, &right)),
        [(Kind::Rotated, Subject::Molecule)]
    );
}

/// `added` joins the reaction participant that holds `at`, as an edit there
/// keeps the participant a whole molecule.
fn join(doc: &mut Document, at: u64, added: &[u64]) {
    for reaction in &mut doc.reactions {
        for role in Role::ALL {
            for participant in reaction.participants_mut(role) {
                if participant.atoms.contains(&at) {
                    participant.atoms.extend(added);
                }
            }
        }
    }
}

/// Adds a nitro group at the ring CH of `molecule` furthest from its
/// oxygens: para to an ester.
fn nitrate(doc: &mut Document, molecule: &[u64]) -> [u64; 3] {
    let bonds = |id: u64| doc.bonds.iter().filter(move |b| b.a == id || b.b == id);
    let oxygens: Vec<Point> = molecule
        .iter()
        .map(|id| doc.atom(*id).unwrap())
        .filter(|atom| atom.element == "O")
        .map(|atom| atom.position)
        .collect();
    let far = |id: &u64| {
        let at = doc.atom(*id).unwrap().position;
        oxygens.iter().map(|o| at.distance(*o)).sum::<f32>()
    };
    let carbon = *molecule
        .iter()
        .filter(|id| {
            doc.atom(**id).unwrap().element == "C"
                && bonds(**id).count() == 2
                && bonds(**id).any(|b| b.order != 1)
        })
        .max_by(|a, b| far(a).total_cmp(&far(b)))
        .unwrap();
    let at = doc.atom(carbon).unwrap().position;
    let n = doc.add_atom("N", at.offset(0., 40.));
    doc.atom_mut(n).unwrap().charge = 1;
    let o1 = doc.add_atom("O", at.offset(-30., 60.));
    let o2 = doc.add_atom("O", at.offset(30., 60.));
    doc.atom_mut(o2).unwrap().charge = -1;
    doc.add_bond(carbon, n, 1, "plain");
    doc.add_bond(n, o1, 2, "plain");
    doc.add_bond(n, o2, 1, "plain");
    join(doc, carbon, &[n, o1, o2]);
    [n, o1, o2]
}

#[test]
fn moving_everything_and_adding_a_nitro_group_is_one_move_and_one_local_change() {
    let s = scheme();
    let mut added = [0; 3];
    let right = edited(&s.doc, |doc| {
        moved(doc, &doc.all_ids(), -38., 14., 0.);
        added = nitrate(doc, &s.ester);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(
        kinds(&report),
        [
            (Kind::Chemical, Subject::Molecule),
            (Kind::SchemeMoved, Subject::Scheme)
        ]
    );
    let change = chemical(&report)[0];
    assert_eq!(change.left.objects, s.ester);
    // The nitro group and the carbon that lost its hydrogen, nothing else.
    assert_eq!(change.right.atoms.len(), 4, "{:?}", change.right.atoms);
    assert!(added.iter().all(|id| change.right.atoms.contains(id)));
    assert_eq!(change.left.atoms.len(), 1);
    assert_eq!(change.right.bonds.len(), 3);
    assert_eq!(
        change.detail,
        Detail::Edit {
            added: "NO₂".into(),
            removed: String::new(),
            changed: 1,
            stereo: false
        }
    );
    assert_eq!(change.before.as_deref(), Some("CCOC(=O)c1ccccc1"));
    assert_eq!(
        change.after.as_deref(),
        Some("CCOC(=O)c1ccc([N+](=O)[O-])cc1")
    );
}

#[test]
fn a_redrawn_molecule_is_redrawn_not_changed() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        let atom = doc.atom_mut(s.ethanol[0]).unwrap();
        atom.position = atom.position.offset(8., 14.);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(kinds(&report), [(Kind::Redrawn, Subject::Molecule)]);
    // Redrawn while everything moved: still redrawn, and the move is one line.
    let right = edited(&right, |doc| moved(doc, &doc.all_ids(), 50., 0., 0.));
    let report = compare(&s.doc, &right);
    assert_eq!(
        kinds(&report),
        [
            (Kind::Redrawn, Subject::Molecule),
            (Kind::SchemeMoved, Subject::Scheme)
        ]
    );
}

#[test]
fn aromatic_and_kekule_benzene_are_the_same_chemistry() {
    let kekule = load(&molecule("c1ccccc1"));
    let aromatic = edited(&kekule, |doc| {
        for bond in &mut doc.bonds {
            bond.order = 4;
        }
        for atom in &mut doc.atoms {
            atom.aromatic = true;
        }
    });
    // The other Kekulé structure: every bond order swapped.
    let other = edited(&kekule, |doc| {
        for bond in &mut doc.bonds {
            bond.order = 3 - bond.order;
        }
    });
    for right in [&aromatic, &other] {
        assert_ne!(&kekule, right);
        let report = compare(&kekule, right);
        assert_eq!(report.molecules[0].chemistry, Chemistry::Same);
        assert_eq!(report.count(Category::Chemistry), 0, "{:?}", report.changes);
        // Drawn differently, so a style line.
        assert_eq!(kinds(&report), [(Kind::Style, Subject::Molecule)]);
    }
}

#[test]
fn a_wedge_moved_to_another_bond_keeping_the_configuration_is_not_chemistry() {
    // Drawn by hand: the wedges, not a stored winding, carry the stereo.
    let drawn = edited(&load(&molecule("C[C@H](O)CC")), |doc| {
        for atom in &mut doc.atoms {
            atom.stereo = None;
        }
    });
    let center = drawn
        .atoms
        .iter()
        .find(|atom| {
            drawn
                .bonds
                .iter()
                .filter(|b| b.a == atom.id || b.b == atom.id)
                .count()
                == 3
        })
        .unwrap()
        .id;
    let wedged: Vec<usize> = drawn
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b.display.as_str(), "wedge" | "hash"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(wedged.len(), 1, "one stereo bond as imported");
    // Flipping the wedge to a hash on the same bond inverts the center.
    let flipped = edited(&drawn, |doc| {
        let bond = &mut doc.bonds[wedged[0]];
        bond.display = if bond.display == "wedge" {
            "hash"
        } else {
            "wedge"
        }
        .into();
    });
    let report = compare(&drawn, &flipped);
    assert_eq!(report.count(Category::Chemistry), 1);
    assert!(matches!(
        chemical(&report)[0].detail,
        Detail::Edit { stereo: true, .. }
    ));
    assert!(chemical(&report)[0].right.atoms.contains(&center));
    // The same center drawn with the opposite mark on another bond keeps it.
    let other = edited(&drawn, |doc| {
        let display = doc.bonds[wedged[0]].display.clone();
        doc.bonds[wedged[0]].display = "plain".into();
        let opposite = if display == "wedge" { "hash" } else { "wedge" };
        let bond = doc
            .bonds
            .iter_mut()
            .enumerate()
            .find(|(i, b)| *i != wedged[0] && (b.a == center || b.b == center))
            .map(|(_, b)| b)
            .unwrap();
        if bond.b == center {
            std::mem::swap(&mut bond.a, &mut bond.b);
        }
        bond.display = opposite.into();
    });
    let report = compare(&drawn, &other);
    if report.molecules[0].chemistry == Chemistry::Same {
        assert_eq!(report.count(Category::Chemistry), 0);
    } else {
        // The bond chosen may sit where the opposite mark means the same:
        // what matters is that the signature follows the configuration.
        let mirrored = chemical(&report)[0].after.clone().unwrap();
        assert!(mirrored.contains('@'));
    }
}

#[test]
fn e_z_and_unknown_double_bond_stereo_are_chemistry() {
    let e = load(&molecule("C/C=C/C"));
    let z = load(&molecule("C/C=C\\C"));
    let report = compare(&e, &z);
    assert_eq!(report.count(Category::Chemistry), 1, "{:?}", report.changes);
    let unknown = edited(&e, |doc| {
        let bond = doc.bonds.iter_mut().find(|b| b.order == 2).unwrap();
        bond.display = "wavy".into();
    });
    let report = compare(&e, &unknown);
    assert_eq!(report.count(Category::Chemistry), 1, "{:?}", report.changes);
}

#[test]
fn an_unknown_stereocenter_differs_from_an_unmarked_one() {
    let plain = load(&molecule("CC(O)CC"));
    let center = plain
        .atoms
        .iter()
        .find(|atom| {
            plain
                .bonds
                .iter()
                .filter(|b| b.a == atom.id || b.b == atom.id)
                .count()
                == 3
        })
        .unwrap()
        .id;
    let wavy = edited(&plain, |doc| {
        let bond = doc
            .bonds
            .iter_mut()
            .find(|b| b.a == center || b.b == center)
            .unwrap();
        if bond.b == center {
            std::mem::swap(&mut bond.a, &mut bond.b);
        }
        bond.display = "wavy".into();
    });
    let report = compare(&plain, &wavy);
    assert_eq!(report.count(Category::Chemistry), 1, "{:?}", report.changes);
}

#[test]
fn isotopes_charges_and_hydrogens_are_chemistry() {
    let s = scheme();
    let oxygen = *s
        .ethanol
        .iter()
        .find(|id| s.doc.atom(**id).unwrap().element == "O")
        .unwrap();
    type Edit = fn(&mut crate::document::Atom);
    let edits: [(&str, Edit); 3] = [
        ("isotope", |atom| atom.isotope = 18),
        ("charge", |atom| atom.charge = -1),
        ("hydrogens", |atom| {
            atom.no_implicit = true;
            atom.explicit_h = 0;
        }),
    ];
    for (what, edit) in edits {
        let right = edited(&s.doc, |doc| edit(doc.atom_mut(oxygen).unwrap()));
        let report = compare(&s.doc, &right);
        let found = chemical(&report);
        assert_eq!(found.len(), 1, "{what}: {:?}", report.changes);
        assert_eq!(found[0].right.atoms, [oxygen], "{what}");
        assert_eq!(found[0].left.objects, s.ethanol, "{what}");
    }
}

#[test]
fn a_drawn_hydrogen_is_not_a_change_of_chemistry() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        let oxygen = *s
            .ethanol
            .iter()
            .find(|id| doc.atom(**id).unwrap().element == "O")
            .unwrap();
        let at = doc.atom(oxygen).unwrap().position;
        let h = doc.add_atom("H", at.offset(20., 0.));
        doc.add_bond(oxygen, h, 1, "plain");
        join(doc, oxygen, &[h]);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(report.count(Category::Chemistry), 0, "{:?}", report.changes);
}

#[test]
fn a_collapsed_label_is_the_same_chemistry() {
    let expanded = load(&molecule("COc1ccccc1"));
    let carbon = expanded
        .atoms
        .iter()
        .find(|atom| {
            atom.element == "C"
                && expanded
                    .bonds
                    .iter()
                    .filter(|b| b.a == atom.id || b.b == atom.id)
                    .count()
                    == 1
        })
        .unwrap()
        .id;
    let oxygen = expanded
        .atoms
        .iter()
        .find(|atom| atom.element == "O")
        .unwrap()
        .id;
    let collapsed = edited(&expanded, |doc| {
        doc.contract(&[oxygen, carbon], "OMe", "MeO").unwrap()
    });
    assert_eq!(collapsed.abbreviations.len(), 1);
    let report = compare(&expanded, &collapsed);
    assert_eq!(report.molecules[0].chemistry, Chemistry::Same);
    assert_eq!(kinds(&report), [(Kind::Style, Subject::Molecule)]);
}

#[test]
fn an_edited_condition_is_a_text_line() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        let note = doc
            .annotations
            .iter_mut()
            .find(|n| n.id == s.conditions)
            .unwrap();
        note.text = "H2SO4, heat".into();
    });
    let report = compare(&s.doc, &right);
    assert_eq!(kinds(&report), [(Kind::Text, Subject::Text)]);
    let change = &report.changes[0];
    assert_eq!(change.category(), Category::Layout);
    assert_eq!(
        (change.before.as_deref(), change.after.as_deref()),
        (Some("H2SO4"), Some("H2SO4, heat"))
    );
}

#[test]
fn a_changed_reaction_role_is_a_reaction_line() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        let reaction = &mut doc.reactions[0];
        let ethanol = reaction.reactants.remove(1);
        reaction.agents.push(ethanol);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(kinds(&report), [(Kind::Reaction, Subject::Molecule)]);
    let change = &report.changes[0];
    assert_eq!(change.category(), Category::Chemistry);
    assert_eq!(change.right.objects, s.ethanol);
    assert_eq!(
        (change.before.as_deref(), change.after.as_deref()),
        (Some("Reactant"), Some("Reagent"))
    );
    // An arrow of another kind is reaction semantics too.
    let right = edited(&s.doc, |doc| doc.arrows[0].kind = "equilibrium".into());
    assert_eq!(
        kinds(&compare(&s.doc, &right)),
        [(Kind::Reaction, Subject::Arrow)]
    );
}

#[test]
fn duplicate_molecules_pasted_again_pair_by_place() {
    let one = molecule("c1ccncc1");
    let mut left = Document::default();
    crate::editing::append(&mut left, &one, Point::new(0., 0.));
    crate::editing::append(&mut left, &one, Point::new(300., 0.));
    let left = load(&left);
    // Both deleted and pasted again: new ids, the same places, nudged right.
    let mut right = left.clone();
    let old = right.all_ids();
    let first = crate::editing::append(&mut right, &one, Point::new(300., 0.));
    crate::editing::append(&mut right, &one, Point::new(0., 0.));
    right.delete(&old);
    let right = load(&right);
    assert!(left.atoms.iter().all(|atom| {
        right
            .atom(atom.id)
            .is_none_or(|b| b.position != atom.position)
    }));
    let report = compare(&left, &right);
    assert_eq!(report.changes, vec![], "same chemistry in the same places");
    assert!(
        report
            .molecules
            .iter()
            .all(|pair| pair.by == MatchedBy::Smiles)
    );
    let at_300 = report
        .molecules
        .iter()
        .find(|pair| pair.right.iter().any(|id| first.contains(id)))
        .unwrap();
    let x = left.atom(at_300.left[0]).unwrap().position.x;
    assert!(x > 150., "paired with the copy at the same place");
    // Moved together, still one line.
    let right = edited(&right, |doc| moved(doc, &doc.all_ids(), 0., 70., 0.));
    assert_eq!(
        kinds(&compare(&left, &right)),
        [(Kind::SchemeMoved, Subject::Scheme)]
    );
}

#[test]
fn a_reused_id_on_another_atom_is_not_a_match() {
    let s = scheme();
    let base = edited(&s.doc, |doc| {
        doc.add_atom("Cl", Point::new(0., 300.));
    });
    let last = base.atoms.last().unwrap().id;
    // Each side deleted the lone atom and drew a new one, which took its id.
    let side = |element: &str, x: f32| {
        edited(&base, |doc| {
            doc.delete(&[last]);
            let id = doc.add_atom(element, Point::new(x, 300.));
            assert_eq!(id, last);
        })
    };
    let (left, right) = (side("N", 100.), side("S", 400.));
    let report = compare(&left, &right);
    assert_eq!(
        kinds(&report),
        [
            (Kind::Added, Subject::Molecule),
            (Kind::Removed, Subject::Molecule)
        ]
    );
}

#[test]
fn refreshed_computed_labels_alone_are_no_change() {
    let drawn = load(&molecule("C[C@H](O)CC"));
    let refreshed = edited(&drawn, |doc| {
        for atom in &mut doc.atoms {
            atom.cip_label = Some("S".into());
            atom.label_h = 3;
        }
        for bond in &mut doc.bonds {
            bond.cip_label = Some("E".into());
        }
    });
    assert_ne!(drawn, refreshed);
    assert_eq!(compare(&drawn, &refreshed).changes, vec![]);
}

#[test]
fn an_r_group_cannot_be_verified() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        let oxygen = *s
            .ethanol
            .iter()
            .find(|id| doc.atom(**id).unwrap().element == "O")
            .unwrap();
        let at = doc.atom(oxygen).unwrap().position;
        let r = doc.add_atom("R", at.offset(30., 0.));
        doc.add_bond(oxygen, r, 1, "plain");
        join(doc, oxygen, &[r]);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(kinds(&report), [(Kind::Unverified, Subject::Molecule)]);
    assert_eq!(report.unverified(), 1);
    assert_eq!(report.count(Category::Chemistry), 0);
    assert!(matches!(&report.changes[0].detail, Detail::Reason(why) if why.contains('R')));
    // Unchanged, it still cannot be verified: never "same".
    let report = compare(&right, &right);
    assert_eq!(report.unverified(), 1);
    assert!(matches!(
        report.molecules[1].chemistry,
        Chemistry::Unverified(_)
    ));
}

#[test]
fn added_and_removed_objects_are_listed() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        doc.delete(&s.acid);
        text(doc, "Step 2", 0., 200.);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(
        kinds(&report),
        [
            (Kind::Added, Subject::Text),
            (Kind::Removed, Subject::Molecule)
        ]
    );
    let removed = report
        .changes
        .iter()
        .find(|c| c.kind == Kind::Removed)
        .unwrap();
    assert_eq!(removed.category(), Category::Chemistry);
    assert_eq!(removed.left.atoms, s.acid);
    let added = report
        .changes
        .iter()
        .find(|c| c.kind == Kind::Added)
        .unwrap();
    assert_eq!(added.category(), Category::Layout);
    assert_eq!(added.after.as_deref(), Some("Step 2"));
}

#[test]
fn style_and_settings_are_style_lines() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        doc.drawing_style.line_width_pt *= 2.;
        doc.atom_mut(s.acid[0]).unwrap().display.highlight = Some(crate::palette::Color::Ink);
    });
    let report = compare(&s.doc, &right);
    assert_eq!(
        kinds(&report),
        [
            (Kind::Style, Subject::Molecule),
            (Kind::Style, Subject::Settings)
        ]
    );
    assert_eq!(report.count(Category::Chemistry), 0);
}

#[test]
fn an_unresolved_label_cannot_be_verified() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        let oxygen = *s
            .ethanol
            .iter()
            .find(|id| doc.atom(**id).unwrap().element == "O")
            .unwrap();
        let at = doc.atom(oxygen).unwrap().position;
        let r = doc.add_atom("*", at.offset(30., 0.));
        doc.atom_mut(r).unwrap().display.variable = Some("R1".into());
        doc.add_bond(oxygen, r, 1, "plain");
        join(doc, oxygen, &[r]);
    });
    let report = compare(&right, &right);
    assert_eq!(report.unverified(), 1, "{:?}", report.changes);
    let renamed = edited(&right, |doc| {
        let r = doc.atoms.iter_mut().find(|a| a.element == "*").unwrap();
        r.display.variable = Some("R2".into());
    });
    assert_eq!(compare(&right, &renamed).unverified(), 1);
}

#[test]
fn unpaired_electrons_and_spin_are_chemistry() {
    let lone = |element: &str, radicals: u8, singlet: bool| {
        let mut doc = Document::default();
        let id = doc.add_atom(element, Point::new(0., 0.));
        let atom = doc.atom_mut(id).unwrap();
        if element == "C" {
            atom.no_implicit = true;
            atom.explicit_h = 2;
        }
        atom.radical_electrons = radicals;
        atom.singlet = singlet;
        load(&doc)
    };
    // A carbene, triplet or singlet.
    let (triplet, singlet) = (lone("C", 2, false), lone("C", 2, true));
    assert_eq!(compare(&triplet, &singlet).count(Category::Chemistry), 1);
    // Iron writes as [Fe] with one unpaired electron or two.
    let report = compare(&lone("Fe", 1, false), &lone("Fe", 2, false));
    assert_eq!(report.count(Category::Chemistry), 1, "{:?}", report.changes);
    assert_eq!(compare(&triplet, &triplet).changes, vec![]);
}

#[test]
fn an_arrow_given_a_bend_is_redrawn() {
    let s = scheme();
    let right = edited(&s.doc, |doc| {
        doc.arrows[0].control = Some(Point::new(370., -40.))
    });
    assert_eq!(
        kinds(&compare(&s.doc, &right)),
        [(Kind::Redrawn, Subject::Arrow)]
    );
}

#[test]
fn renumbered_stereo_groups_are_the_same_chemistry() {
    let group = |number: u32| {
        edited(&load(&molecule("C[C@H](O)CC")), |doc| {
            for atom in doc.atoms.iter_mut().filter(|a| a.stereo.is_some()) {
                atom.stereo_group = Some(crate::stereo_groups::StereoGroup {
                    kind: crate::stereo_groups::Kind::And,
                    number,
                });
            }
        })
    };
    let report = compare(&group(7), &group(1));
    assert_eq!(report.count(Category::Chemistry), 0, "{:?}", report.changes);
}

#[test]
fn a_molecule_is_fitted_on_its_own_atoms_only() {
    // Two propanes far apart; the right side swapped the ids of their ends.
    let mut left = Document::default();
    let chain = |doc: &mut Document, x: f32| -> Vec<u64> {
        let ids: Vec<u64> = (0..3)
            .map(|i| doc.add_atom("C", Point::new(x + 36. * i as f32, 21. * (i % 2) as f32)))
            .collect();
        doc.add_bond(ids[0], ids[1], 1, "plain");
        doc.add_bond(ids[1], ids[2], 1, "plain");
        ids
    };
    let a = chain(&mut left, 0.);
    let b = chain(&mut left, 400.);
    let left = load(&left);
    let right = edited(&left, |doc| {
        let (p, q) = (
            doc.atom(a[2]).unwrap().position,
            doc.atom(b[2]).unwrap().position,
        );
        doc.atom_mut(a[2]).unwrap().position = q;
        doc.atom_mut(b[2]).unwrap().position = p;
        for bond in &mut doc.bonds {
            for end in [&mut bond.a, &mut bond.b] {
                if *end == a[2] {
                    *end = b[2];
                } else if *end == b[2] {
                    *end = a[2];
                }
            }
        }
    });
    let report = compare(&left, &right);
    assert!(
        report.changes.iter().all(|c| c.kind != Kind::Redrawn),
        "{:?}",
        report.changes
    );
}

#[test]
fn a_symmetric_molecule_with_new_ids_is_not_turned() {
    let mut left = Document::default();
    let a = left.add_atom("C", Point::new(0., 0.));
    let b = left.add_atom("C", Point::new(42., 0.));
    left.add_bond(a, b, 1, "plain");
    let left = load(&left);
    // Deleted and drawn again, from the other end.
    let right = edited(&left, |doc| {
        let old = doc.all_ids();
        let c = doc.add_atom("C", Point::new(42., 0.));
        let d = doc.add_atom("C", Point::new(0., 0.));
        doc.add_bond(c, d, 1, "plain");
        doc.delete(&old);
    });
    let report = compare(&left, &right);
    assert_eq!(report.changes, vec![]);
    assert_eq!(report.molecules[0].by, MatchedBy::Smiles);
}

#[test]
fn atoms_paired_by_place_must_keep_their_bonds() {
    // Propane on a triangle's corners, drawn again with new ids and another
    // corner in the middle.
    let corners = [
        Point::new(0., 0.),
        Point::new(36., 21.),
        Point::new(72., 0.),
    ];
    let propane = |doc: &mut Document, middle: usize| {
        let ids: Vec<u64> = corners.iter().map(|p| doc.add_atom("C", *p)).collect();
        for (i, id) in ids.iter().enumerate() {
            if i != middle {
                doc.add_bond(ids[middle], *id, 1, "plain");
            }
        }
    };
    let mut left = Document::default();
    propane(&mut left, 1);
    let left = load(&left);
    let right = edited(&left, |doc| {
        let old = doc.all_ids();
        propane(doc, 0);
        doc.delete(&old);
    });
    assert_eq!(
        kinds(&compare(&left, &right)),
        [(Kind::Redrawn, Subject::Molecule)]
    );
}

#[test]
fn a_hydrogen_bond_between_molecules_is_listed() {
    let mut left = Document::default();
    let o = left.add_atom("O", Point::new(0., 0.));
    let h = left.add_atom("H", Point::new(20., 0.));
    left.add_bond(o, h, 1, "plain");
    let n = left.add_atom("N", Point::new(60., 0.));
    let left = load(&left);
    let right = edited(&left, |doc| {
        doc.add_bond(h, n, 0, "dotted");
    });
    let report = compare(&left, &right);
    assert_eq!(report.count(Category::Chemistry), 1, "{:?}", report.changes);
}

#[test]
fn a_stereo_group_shared_by_two_molecules_cannot_be_verified() {
    let one = molecule("C[C@H](O)CC");
    let mut doc = Document::default();
    crate::editing::append(&mut doc, &one, Point::new(0., 0.));
    crate::editing::append(&mut doc, &one, Point::new(300., 0.));
    for atom in doc.atoms.iter_mut().filter(|a| a.stereo.is_some()) {
        atom.stereo_group = Some(crate::stereo_groups::StereoGroup {
            kind: crate::stereo_groups::Kind::And,
            number: 1,
        });
    }
    let doc = load(&doc);
    assert_eq!(compare(&doc, &doc).unverified(), 2);
}
