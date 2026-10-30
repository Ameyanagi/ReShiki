use super::*;
use reshiki::{
    arrow_anchors::{self, Pick, Target},
    arrows::{ArrowStyle, Preset},
};

fn primary_join(app: &mut App, selected: &[u64]) {
    app.tab.selected = selected.into();
    let key = Key::Character("j".into());
    let message = key_message(&key, &key, Modifiers::COMMAND).unwrap();
    assert!(matches!(message, Message::Shortcut(Action::Join)));
    let _ = app.update(message);
    assert!(!app.error, "{}", app.status);
    app.tab.doc.validate().unwrap();
}

fn ready(doc: Document) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.saved = app.tab.doc.clone();
    app
}

fn link(doc: &mut Document, source: u64, target: u64) -> u64 {
    arrow_anchors::create(
        doc,
        &Pick::Atom(source),
        &Pick::Atom(target),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap()
}

fn targets(doc: &Document, id: u64, source: u64, target: u64) {
    let arrow = doc.arrows.iter().find(|a| a.id == id).unwrap();
    assert!(
        matches!(arrow.start_anchor.as_ref().unwrap().target, Target::Atom { atom } if atom == source)
    );
    assert!(
        matches!(arrow.end_anchor.as_ref().unwrap().target, Target::Atom { atom } if atom == target)
    );
}

#[test]
fn primary_j_first_and_second_chelate_donors_preserve_links_atom_identity_charge_and_each_history_frame()
 {
    for reversed in [false, true] {
        let mut doc = Document::from_json(include_bytes!(
            "../../../tests/fixtures/coordination/co-en3-before.rsk"
        ))
        .unwrap();
        // Seed CH2 labels as on a refreshed native load.
        for atom in &mut doc.atoms {
            if atom.element == "C" {
                atom.label_h = 2;
            }
        }
        let id = link(&mut doc, 2, 1);
        let mut app = ready(doc);
        let original = app.tab.doc.clone();
        let mut snapshots = vec![original.clone()];
        for donor in [2, 5] {
            let selected = if reversed { [1, donor] } else { [donor, 1] };
            primary_join(&mut app, &selected);
            assert_eq!(
                app.tab.doc.atoms, original.atoms,
                "all atom IDs, metadata and XYZ"
            );
            assert_eq!(&app.tab.doc.bonds[..9], original.bonds.as_slice());
            let contact = app.tab.doc.bonds.last().unwrap();
            assert_eq!((contact.a, contact.b, contact.order), (donor, 1, 5));
            assert_eq!(app.tab.doc.atom(1).unwrap().charge, 3);
            assert_eq!(app.tab.doc.atom(donor).unwrap().label_h, 2);
            targets(&app.tab.doc, id, 2, 1);
            snapshots.push(app.tab.doc.clone());
        }
        assert_eq!(app.tab.history.undo_frames().len(), 2);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, snapshots[1]);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, snapshots[0]);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, snapshots[1]);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, snapshots[2]);
        let count = app.tab.history.undo_frames().len();
        primary_join(&mut app, &[1, 5]);
        assert_eq!(app.tab.doc, snapshots[2]);
        assert_eq!(
            app.tab.history.undo_frames().len(),
            count,
            "duplicate contact has no extra frame"
        );
        let mut saved = app.tab.doc.clone();
        saved.version = saved.file_version();
        assert_eq!(
            Document::from_json(&serde_json::to_vec(&saved).unwrap()).unwrap(),
            saved
        );
    }
}

#[test]
fn the_same_primary_j_keeps_ordinary_sharing_and_visual_multiatom_merge_with_links() {
    let mut doc = Document::default();
    let host = doc.add_atom("C", Point::default());
    let source = doc.add_atom("C", Point::new(180., 0.));
    let tail = doc.add_atom("C", Point::new(222., 0.));
    doc.add_bond(source, tail, 1, "plain");
    let id = link(&mut doc, source, tail);
    let mut app = ready(doc);
    let before = app.tab.doc.clone();
    primary_join(&mut app, &[source, host]);
    assert_eq!(
        app.tab.doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>(),
        [host, tail]
    );
    assert_eq!(app.tab.doc.bonds.len(), 1);
    let bond = &app.tab.doc.bonds[0];
    assert_eq!(
        (bond.a.min(bond.b), bond.a.max(bond.b), bond.order),
        (host, tail, 1)
    );
    targets(&app.tab.doc, id, host, tail);
    let shared = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, shared);

    let mut doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    let id = link(&mut doc, 3, 4);
    let mut app = ready(doc);
    let before = app.tab.doc.clone();
    primary_join(&mut app, &[1, 3, 5]);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (4, 3));
    assert_eq!(app.tab.selected, [1]);
    assert_eq!(app.tab.doc.atom(1).unwrap().element, "N");
    assert_eq!(
        app.tab.doc.atom(1).unwrap().text_style,
        before.atom(1).unwrap().text_style
    );
    for terminal in [2, 4, 6] {
        assert_eq!(
            app.tab.doc.atom(terminal).unwrap().position,
            before.atom(terminal).unwrap().position
        );
    }
    targets(&app.tab.doc, id, 1, 4);
    let joined = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, joined);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
}
