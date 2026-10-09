use super::*;
use crate::canvas::Edit;

fn fixture() -> App {
    let (mut app, _) = App::new();
    let mut doc = reshiki::document::Document::default();
    let (s, c) = 17.3_f32.to_radians().sin_cos();
    doc.add_atom("C", Point::new(42. * c, 42. * s));
    doc.add_atom("C", Point::new(84. * c, 84. * s));
    doc.add_atom("O", Point::new(84. * c + 21., 84. * s - 36.373_066));
    doc.add_bond(1, 2, 1, "wedge");
    doc.add_bond(2, 3, 1, "plain");
    app.tab.doc = doc;
    app.tab.selected = vec![1, 2];
    app.tab.history = Default::default();
    app.sync_numeric_transforms();
    app
}
fn act(app: &mut App, action: Action) {
    let _ = app.update(message(action));
}

#[test]
fn exact_alignment_ui_expands_the_fragment_preserves_chemistry_and_is_one_undo() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    act(&mut app, Action::Open(true));
    act(&mut app, Action::Align(0.));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.selected, vec![1, 2, 3]);
    assert!((app.tab.doc.atoms[0].position.y - app.tab.doc.atoms[1].position.y).abs() < 0.0001);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let aligned = app.tab.doc.clone();
    let revision = app.tab.revision;
    act(&mut app, Action::Align(0.));
    assert_eq!(app.tab.revision, revision);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, aligned);
}

#[test]
fn pinned_center_survives_selection_changes_and_copy_rotation_uses_the_same_point() {
    let mut app = fixture();
    act(&mut app, Action::PinCenter);
    let pivot = app.reference_pivot(&[1, 2, 3], Edge::Bond(1, 2)).unwrap();
    let pinned = (
        app.tab.numeric_transforms.reference.x.clone(),
        app.tab.numeric_transforms.reference.y.clone(),
    );
    act(&mut app, Action::Rotate(true));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), 6);
    assert_eq!(app.tab.numeric_transforms.reference.pivot, Pivot::Pinned);
    assert_eq!(
        (
            app.tab.numeric_transforms.reference.x.clone(),
            app.tab.numeric_transforms.reference.y.clone()
        ),
        pinned
    );
    assert_eq!(
        app.reference_pivot(&app.tab.selected, app.chosen_reference().unwrap().edge)
            .unwrap(),
        pivot
    );
    act(&mut app, Action::Turn("17.3".into()));
    act(&mut app, Action::Rotate(true));
    assert!(!app.error);
    assert_eq!(app.tab.doc.atoms.len(), 9);
    let epoch = app.tab.file_epoch + 1;
    app.tab.file_epoch = epoch;
    app.sync_numeric_transforms();
    assert_eq!(app.tab.numeric_transforms.reference.pivot, Pivot::Center);
}

#[test]
fn numeric_and_pointer_stretch_use_the_same_axis_branch_and_history() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    let plan = app.reference_stretch().unwrap();
    act(&mut app, Action::Length("21.6".into()));
    act(&mut app, Action::Stretch);
    assert!(!app.error, "{}", app.status);
    let expected = plan
        .apply(&original, original.drawing_style.world(21.6))
        .unwrap();
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    act(&mut app, Action::StretchDrag);
    assert_eq!(
        app.tool,
        Tool::StretchBond {
            fixed: 1,
            moving: 2
        }
    );
    let _ = app.update(Message::Canvas(Edit::StretchBond {
        fixed: 1,
        moving: 2,
        length: original.drawing_style.world(21.6),
    }));
    assert_eq!(app.tab.doc, expected);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn invalid_length_ring_or_partial_scope_cannot_modify_the_drawing() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    act(&mut app, Action::Length("0".into()));
    act(&mut app, Action::Stretch);
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    act(&mut app, Action::Scope(Scope::Selected));
    act(&mut app, Action::Align(90.));
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    app.tab.doc.add_bond(3, 1, 1, "plain");
    let ring = app.tab.doc.clone();
    act(&mut app, Action::StretchDrag);
    assert!(app.error);
    assert_eq!(app.tab.doc, ring);
}

#[test]
fn stretch_mode_and_queued_edits_cannot_cross_tabs_with_reused_atom_ids() {
    let mut app = fixture();
    let original_id = app.tab.id;
    let original = app.tab.doc.clone();
    app.add_tab();
    app.tab.doc = original.clone();
    app.tab.doc.translate(&[1, 2, 3], 300., 200.);
    let other_id = app.tab.id;
    let other = app.tab.doc.clone();
    app.select_tab(0);
    act(&mut app, Action::StretchDrag);
    assert!(app.reference_stretch_active(1, 2));
    app.in_tab(other_id, |app| app.sync_numeric_transforms())
        .unwrap();
    assert_eq!(app.tab.id, original_id);
    assert!(
        app.reference_stretch_active(1, 2),
        "A background callback is not a real tab switch"
    );
    app.select_tab(1);
    assert_eq!(app.tool, Tool::Select);
    let _ = app.update(Message::Canvas(Edit::StretchBond {
        fixed: 1,
        moving: 2,
        length: 63.,
    }));
    assert_eq!(app.tab.doc, other);
    assert!(!app.tab.history.can_undo());
    app.select_tab(0);
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn stretch_owner_is_cleared_on_file_epoch_change_and_reset() {
    let mut app = fixture();
    act(&mut app, Action::StretchDrag);
    app.tab.file_epoch = app.tab.file_epoch.wrapping_add(1);
    app.sync_numeric_transforms();
    assert_eq!(app.tool, Tool::Select);
    act(&mut app, Action::StretchDrag);
    assert!(app.reference_stretch_active(1, 2));
    app.reset_tab();
    assert_eq!(app.tool, Tool::Select);
    assert!(app.tab.doc.atoms.is_empty());
}
