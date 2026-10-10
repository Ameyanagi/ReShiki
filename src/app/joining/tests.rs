use super::*;
use crate::canvas::Edit;
use reshiki::document::{Document, Point};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("C", Point::default());
    let source = app.tab.doc.add_atom("C", Point::new(180., 0.));
    let end = app.tab.doc.add_atom("C", Point::new(222., 0.));
    app.tab.doc.add_bond(source, end, 1, "plain");
    app.tab.selected = vec![source];
    app
}
#[test]
fn joining_uses_the_preview_and_is_one_undo_step() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    assert_eq!(app.tab.doc, before);
    let state = app.tab.joining.as_ref().unwrap();
    let (expected, _) = state
        .prepared
        .place(
            Point::default(),
            None,
            10. / app.tab.camera.zoom,
            state.anchor,
            state.mode,
        )
        .unwrap();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
}
#[test]
fn cancel_switching_tools_and_invalid_targets_keep_the_original() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Canvas(Edit::Template(
        Point::new(500., 500.),
        None,
    )));
    assert!(app.tab.joining.is_some());
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Escape);
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Tool(Tool::Bond(2)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tool, Tool::Bond(2));
    assert_eq!(app.tab.doc, before);
}
#[test]
fn a_changed_document_cannot_be_overwritten_by_a_prepared_join() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::new(300., 100.));
    app.changed(before);
    let changed = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, changed);
    assert!(app.error);
}
#[test]
fn anchor_picker_switches_between_atom_and_bond_modes() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Bond(2, 3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().mode, Connection::FuseBond);
    let _ = app.update(Message::Join(Action::Mode(Connection::ShareAtom)));
    assert!(matches!(
        app.tab.joining.as_ref().unwrap().anchor,
        Anchor::Atom(_)
    ));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().anchor, Anchor::Atom(3));
}

fn coordination() -> (App, u64, u64, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let co = app.tab.doc.add_atom("Co", Point::default());
    app.tab.doc.atom_mut(co).unwrap().charge = 3;
    let n = app.tab.doc.add_atom("N", Point::new(40., -25.));
    let c = app.tab.doc.add_atom("C", Point::new(70., -15.));
    let c2 = app.tab.doc.add_atom("C", Point::new(70., 15.));
    let n2 = app.tab.doc.add_atom("N", Point::new(40., 25.));
    for (a, b) in [(n, c), (c, c2), (c2, n2)] {
        app.tab.doc.add_bond(a, b, 1, "plain");
    }
    for id in [n, n2] {
        app.tab.doc.atom_mut(id).unwrap().label_h = 2;
    }
    app.tab.selected = vec![n];
    (app, co, n, n2)
}

#[test]
fn first_and_second_chelate_contacts_are_in_place_cancellable_and_one_undo_each() {
    let (mut app, co, n, n2) = coordination();
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Join(Action::Mode(Connection::Coordinate)));
    assert_eq!(
        app.tab.joining.as_ref().unwrap().mode,
        Connection::Coordinate
    );
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(n))));
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert!(
        app.tab
            .doc
            .bonds
            .iter()
            .any(|b| (b.a, b.b, b.order) == (n, co, 5))
    );
    let first = app.tab.doc.clone();
    app.tab.selected = vec![n2];
    let _ = app.update(Message::Join(Action::BeginCoordination));
    assert!(app.tab.joining.is_some());
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, first);
    let _ = app.update(Message::Join(Action::BeginCoordination));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(n2))));
    let _ = app.update(Message::Canvas(Edit::Template(
        Point::default(),
        Some(Point::new(300., 200.)),
    )));
    let second = app.tab.doc.clone();
    assert_eq!(second.atoms, original.atoms);
    assert_eq!(second.bonds.len(), 5);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, first);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, second);
    app.tab.selected = vec![n];
    let _ = app.update(Message::Join(Action::BeginCoordination));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(n))));
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert_eq!(app.tab.doc, second);
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, first,
        "Duplicate contact must not add Undo history"
    );
}

#[test]
fn coordination_projection_flip_changes_tip_and_never_chemical_direction() {
    let (mut app, co, n, _) = coordination();
    app.tab.doc = reshiki::templates::coordinate_atoms(&app.tab.doc, n, co).unwrap();
    app.tab.selected = vec![n, co];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ApplyBondPreset(reshiki::bonds::BondPreset::Wedge));
    let donor_narrowed = app.tab.doc.clone();
    let _ = app.update(Message::ReverseBonds);
    let reversed = app.tab.doc.clone();
    let bond = reversed.bonds.iter().find(|b| b.order == 5).unwrap();
    assert_eq!(
        (bond.a, bond.b, bond.order, bond.display.as_str()),
        (n, co, 5, "wedge_end")
    );
    assert_eq!(reversed.atoms, before.atoms);
    assert_eq!(
        reshiki::bonds::BondPreset::of(bond),
        Some(reshiki::bonds::BondPreset::Wedge)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, donor_narrowed);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, reversed);
    let _ = app.update(Message::ApplyBondPreset(
        reshiki::bonds::BondPreset::HashedWedge,
    ));
    let bond = app.tab.doc.bonds.iter().find(|b| b.order == 5).unwrap();
    assert_eq!((bond.a, bond.b, bond.display.as_str()), (n, co, "hash_end"));
}

#[tokio::test]
async fn coordination_export_reports_the_measured_drawing_limits() -> Result<(), String> {
    let (app, co, n, _) = coordination();
    let mut doc = reshiki::templates::coordinate_atoms(&app.tab.doc, n, co)?;
    let bond = doc.bonds.iter_mut().find(|b| b.order == 5).unwrap();
    reshiki::bonds::BondPreset::Wedge.apply(bond);
    assert!(bond.reverse_projection());
    for format in ["cdxml", "cdx", "mol"] {
        let mut request = reshiki::engine::Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let response = app.engine.request(request).await?;
        assert!(response.output.is_some());
        let expected = if format == "mol" {
            "omits solid/hashed coordination projection appearance"
        } else {
            "ChemDraw 26 displays dative arrows"
        };
        assert!(
            response.warnings.iter().any(|w| w.contains(expected)),
            "Missing {format} limitation: {:?}",
            response.warnings
        );
    }
    Ok(())
}
