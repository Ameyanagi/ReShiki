use super::*;
use crate::document::{Arrow, Point};
use crate::palette::{Color, Hue, Row};

const JOINS: &str =
    "This joins separate reaction participants. Clear their reaction roles before joining them.";
const BLUE: Color = Color::Palette(Hue::Blue, Row::Strong);

/// Two carbons joined by one bond of `order`.
fn ethane(order: u8) -> (Document, [u64; 2]) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, order, "plain");
    (doc, [a, b])
}

/// Reactant a–b and product c on one arrow.
fn reaction() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(300., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(100., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    crate::reactions::assign(&mut doc, arrow, &[a], crate::reactions::Role::Reactant).unwrap();
    crate::reactions::assign(&mut doc, arrow, &[c], crate::reactions::Role::Product).unwrap();
    assert_eq!(doc.reactions[0].reactants[0].atoms, [a, b]);
    assert_eq!(doc.reactions[0].products[0].atoms, [c]);
    (doc, [a, b, c])
}

/// The ether a–b–c with b–c contracted to OMe; c is the hidden member.
fn methoxy() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    assert_eq!(doc.abbreviations.len(), 1);
    assert_eq!(doc.abbreviations[0].members, [b, c]);
    (doc, [a, b, c])
}

/// Every undo frame, newest first, plus whether redo is available.
fn frames(mut history: History, mut doc: Document) -> (Vec<Document>, bool) {
    let redo = history.can_redo();
    let mut frames = vec![];
    while history.undo(&mut doc) {
        frames.push(doc.clone());
    }
    (frames, redo)
}

#[test]
fn reaction_join_restores_before_and_reports_reactions() {
    let (mut doc, [_, b, c]) = reaction();
    let before = doc.clone();
    doc.add_bond(b, c, 1, "plain");
    let rejection = reconcile(&mut doc, before.clone()).err().unwrap();
    assert_eq!(rejection, Rejection::Reactions(JOINS.into()));
    assert_eq!(rejection.message(), JOINS);
    assert_eq!(doc, before);
}

#[test]
fn invalid_change_restores_before_and_reports_validation_message() {
    let (mut doc, _) = ethane(1);
    let before = doc.clone();
    doc.bonds[0].order = 8;
    let rejection = reconcile(&mut doc, before.clone()).err().unwrap();
    assert_eq!(
        rejection,
        Rejection::Invalid("Invalid bond endpoints or order".into())
    );
    assert_eq!(rejection.message(), "Invalid bond endpoints or order");
    assert_eq!(doc, before);
}

#[test]
fn unchanged_document_skips_validation_and_group_reconcile() {
    let (mut doc, [a, b]) = ethane(1);
    let c = doc.add_atom("C", Point::new(84., 0.));
    doc.group_selection(&[a, b]).unwrap();
    doc.add_bond(b, c, 1, "plain");
    let before = doc.clone();
    let mut absorbed = doc.clone();
    absorbed.reconcile_molecule_groups();
    assert!(absorbed.groups[0].members.contains(&c));

    let reconciled = reconcile(&mut doc, before.clone()).unwrap();
    assert_eq!(reconciled.before(), &before);
    assert_eq!(doc.groups, before.groups);
    assert!(!doc.groups[0].members.contains(&c));
    assert_eq!(doc, before);
}

#[test]
fn labels_survive_reconcile_and_clear_only_on_chemistry_commit() {
    for chemistry in [false, true] {
        let (mut doc, [a, _]) = ethane(2);
        doc.atom_mut(a).unwrap().cip_label = Some("R".into());
        doc.bonds[0].cip_label = Some("E".into());
        let mut history = History::default();
        let before = doc.clone();
        if chemistry {
            doc.atom_mut(a).unwrap().element = "N".into();
        } else {
            doc.bonds[0].color = BLUE;
        }

        let reconciled = reconcile(&mut doc, before.clone()).unwrap();
        assert_eq!(reconciled.before(), &before, "chemistry {chemistry}");
        assert_eq!(doc.atom(a).unwrap().cip_label.as_deref(), Some("R"));
        assert_eq!(doc.bonds[0].cip_label.as_deref(), Some("E"));

        let committed = reconciled.commit(&mut doc, &mut history, false);
        assert_eq!(
            committed,
            Committed {
                recorded: true,
                chemistry_changed: chemistry,
                drawing_style_changed: false,
            }
        );
        let labeled = doc.atom(a).unwrap().cip_label.is_some();
        assert_eq!(labeled, !chemistry, "chemistry {chemistry}");
        assert_eq!(doc.bonds[0].cip_label.is_some(), !chemistry);

        assert!(history.undo(&mut doc));
        assert_eq!(doc, before, "chemistry {chemistry}");
    }
}

#[test]
fn commit_reports_style_change_and_noop_records_nothing() {
    let (mut doc, _) = ethane(1);
    let mut history = History::default();
    let before = doc.clone();
    doc.drawing_style.name = "Custom".into();
    let committed =
        reconcile(&mut doc, before.clone())
            .unwrap()
            .commit(&mut doc, &mut history, false);
    assert_eq!(
        committed,
        Committed {
            recorded: true,
            chemistry_changed: false,
            drawing_style_changed: true,
        }
    );

    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.can_redo());
    let unchanged = doc.clone();
    let committed = reconcile(&mut doc, unchanged)
        .unwrap()
        .commit(&mut doc, &mut history, false);
    assert_eq!(
        committed,
        Committed {
            recorded: false,
            chemistry_changed: false,
            drawing_style_changed: false,
        }
    );
    assert_eq!(doc, before);
    assert!(!history.can_undo());
    assert!(history.can_redo());
}

#[test]
fn continuing_commits_keep_the_first_snapshot() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let initial = doc.clone();
    let mut history = History::default();
    for continuing in [false, true, true] {
        let before = doc.clone();
        doc.atom_mut(a).unwrap().position.x += 1.;
        let committed =
            reconcile(&mut doc, before)
                .unwrap()
                .commit(&mut doc, &mut history, continuing);
        assert!(committed.recorded, "continuing {continuing}");
        assert!(committed.chemistry_changed, "continuing {continuing}");
    }
    assert!(history.undo(&mut doc));
    assert_eq!(doc, initial);
    assert!(!history.can_undo());

    // A continuing frame on an empty history still records its snapshot.
    let mut history = History::default();
    let before = doc.clone();
    doc.atom_mut(a).unwrap().position.y += 1.;
    let committed =
        reconcile(&mut doc, before.clone())
            .unwrap()
            .commit(&mut doc, &mut history, true);
    assert!(committed.recorded);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
}

#[test]
fn apply_matches_reconcile_then_commit() {
    type Edit = fn(&mut Document, [u64; 3]);
    let corpus: [(&str, Edit, &str); 6] = [
        ("color", |doc, _| doc.bonds[0].color = BLUE, "display"),
        (
            "move",
            |doc, [a, ..]| doc.atom_mut(a).unwrap().position.y += 10.,
            "chemistry",
        ),
        (
            "element",
            |doc, [a, ..]| doc.atom_mut(a).unwrap().element = "N".into(),
            "chemistry",
        ),
        ("order 8", |doc, _| doc.bonds[0].order = 8, "invalid"),
        (
            "join",
            |doc, [_, b, c]| doc.add_bond(b, c, 1, "plain"),
            "reactions",
        ),
        ("no-op", |_, _| {}, "noop"),
    ];
    for (name, edit, expected) in corpus {
        for continuing in [false, true] {
            let context = format!("{name}, continuing {continuing}");
            let (start, ids) = reaction();
            let mut history = History::default();
            assert!(history.commit(Document::default(), &start));
            let before = start.clone();
            let mut edited = start;
            edit(&mut edited, ids);

            let mut applied = (edited.clone(), history);
            let applied_result = apply(&mut applied.0, &mut applied.1, before.clone(), continuing);
            let mut history = History::default();
            assert!(history.commit(Document::default(), &before));
            let mut phased = (edited, history);
            let phased_result = reconcile(&mut phased.0, before)
                .map(|reconciled| reconciled.commit(&mut phased.0, &mut phased.1, continuing));

            assert_eq!(applied_result, phased_result, "{context}");
            assert_eq!(applied.0, phased.0, "{context}");
            let (applied, phased) = (frames(applied.1, applied.0), frames(phased.1, phased.0));
            assert_eq!(applied, phased, "{context}");
            let outcome = match applied_result {
                Ok(c) if !c.recorded => "noop",
                Ok(c) if c.chemistry_changed => "chemistry",
                Ok(_) => "display",
                Err(Rejection::Reactions(_)) => "reactions",
                Err(Rejection::Invalid(_)) => "invalid",
            };
            assert_eq!(outcome, expected, "{context}");
        }
    }
}

#[test]
fn chemistry_changed_ignores_display_only_fields() {
    type Edit<'a> = &'a dyn Fn(&mut Document);
    let (base, [a, b]) = ethane(1);
    let edited = |edit: Edit| {
        let mut doc = base.clone();
        edit(&mut doc);
        assert_ne!(doc, base);
        doc
    };
    let display: [(&str, Edit); 8] = [
        ("z_order", &|doc| doc.bonds[0].z_order = 3),
        ("color", &|doc| doc.bonds[0].color = BLUE),
        ("highlight", &|doc| doc.bonds[0].highlight = Some(BLUE)),
        ("bond cip_label", &|doc| {
            doc.bonds[0].cip_label = Some("E".into())
        }),
        ("atom cip_label", &|doc| {
            doc.atom_mut(a).unwrap().cip_label = Some("R".into())
        }),
        ("label_h", &|doc| doc.atom_mut(a).unwrap().label_h = 2),
        ("text_style", &|doc| {
            doc.atom_mut(a).unwrap().text_style = Some(Default::default())
        }),
        ("marks", &|doc| {
            doc.atom_mut(a)
                .unwrap()
                .marks
                .push(crate::scientific::AtomMark {
                    kind: crate::scientific::MarkKind::Radical,
                    offset: Point::new(5., 5.),
                    angle: 0.,
                    size_pt: None,
                })
        }),
    ];
    for (name, edit) in display {
        assert!(!chemistry_changed(&base, &edited(edit)), "{name}");
    }
    let chemistry: [(&str, Edit); 3] = [
        ("element", &|doc| {
            doc.atom_mut(b).unwrap().element = "N".into()
        }),
        ("order", &|doc| doc.bonds[0].order = 2),
        ("position", &|doc| doc.atom_mut(b).unwrap().position.x += 1.),
    ];
    for (name, edit) in chemistry {
        assert!(chemistry_changed(&base, &edited(edit)), "{name}");
    }

    // Aromatic bonds and bonds that are projections on both sides ignore their
    // display and direction; other bonds do not.
    let reversed = |doc: &mut Document| {
        doc.bonds[0].reverse();
        doc.bonds[0].display = "bold".into();
    };
    for (order, projection, changed) in [(4, false, false), (1, true, false), (1, false, true)] {
        let mut before = base.clone();
        before.bonds[0].order = order;
        before.bonds[0].projection = projection;
        let mut after = before.clone();
        reversed(&mut after);
        assert_eq!(
            chemistry_changed(&before, &after),
            changed,
            "order {order}, projection {projection}"
        );
    }
    let mut before = base.clone();
    before.bonds[0].projection = true;
    let mut after = before.clone();
    after.bonds[0].projection = false;
    assert!(chemistry_changed(&before, &after));
}

#[test]
fn stage_order_is_fixed() {
    // A ring fill whose cycle lost a bond is pruned, not rejected.
    {
        let mut doc = crate::rings::Preset::Regular.document(42., false);
        let ids = doc.all_ids();
        assert_eq!(crate::ring_fills::apply(&mut doc, &ids, Some(BLUE)), 1);
        let before = doc.clone();
        doc.bonds.remove(0);
        assert_eq!(doc.ring_fills.len(), 1);
        assert!(crate::ring_fills::validate(&doc).is_err());
        let reconciled = reconcile(&mut doc, before.clone()).unwrap();
        assert!(doc.ring_fills.is_empty());
        assert_eq!(reconciled.before(), &before);
    }
    // An outside bond to the hidden member drops the abbreviation before
    // validation, which would reject it.
    {
        let (mut doc, [_, _, c]) = methoxy();
        let before = doc.clone();
        let d = doc.add_atom("C", Point::new(84., 0.));
        doc.add_bond(c, d, 1, "plain");
        assert_eq!(
            doc.validate(),
            Err("Only the abbreviation's attachment atom can connect outside it".into())
        );
        assert!(reconcile(&mut doc, before).is_ok());
        assert!(doc.abbreviations.is_empty());
    }
    // Validation runs before group reconciliation, whose prune would repair the
    // stale member.
    {
        let mut doc = Document::default();
        let x = doc.add_atom("C", Point::default());
        let y = doc.add_atom("C", Point::new(42., 0.));
        let z = doc.add_atom("C", Point::new(84., 0.));
        doc.group_selection(&[x, y, z]).unwrap();
        let before = doc.clone();
        doc.atoms.retain(|t| t.id != z);
        assert!(doc.groups[0].members.contains(&z));
        assert_eq!(
            reconcile(&mut doc, before.clone()).err(),
            Some(Rejection::Invalid("Invalid group ID or membership".into()))
        );
        assert_eq!(doc, before);
    }
    // Reactions reconcile before validation, which would reject order 8.
    {
        let (mut doc, [_, b, c]) = reaction();
        let before = doc.clone();
        doc.add_bond(b, c, 8, "plain");
        assert!(doc.validate().is_err());
        assert_eq!(
            reconcile(&mut doc, before.clone()).err(),
            Some(Rejection::Reactions(JOINS.into()))
        );
        assert_eq!(doc, before);
    }
}
