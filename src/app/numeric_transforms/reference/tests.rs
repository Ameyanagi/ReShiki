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
fn rejected_document_replacement_keeps_selection_history_revision_and_status() {
    for with_history in [false, true] {
        let mut app = fixture();
        if with_history {
            act(&mut app, Action::Align(0.));
        }
        let original = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let can_undo = app.tab.history.can_undo();
        let can_redo = app.tab.history.can_redo();
        let revision = app.tab.revision;
        app.status = "Status before replacement".into();
        app.error = with_history;
        app.tab.erase_stroke = true;
        let mut candidate = original.clone();
        candidate.atoms.push(candidate.atoms[0].clone());
        assert_eq!(
            app.try_replace_document(candidate, vec![3]),
            Err("Duplicate or zero object ID".into())
        );
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.history.can_undo(), can_undo);
        assert_eq!(app.tab.history.can_redo(), can_redo);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.status, "Status before replacement");
        assert_eq!(app.error, with_history);
        assert!(app.tab.erase_stroke);
        if with_history {
            assert!(app.tab.history.undo(&mut app.tab.doc));
            assert!(!app.tab.history.can_undo());
            assert!(app.tab.history.redo(&mut app.tab.doc));
            assert_eq!(app.tab.doc, original);
        }
    }
}

#[test]
fn reference_edits_that_leave_a_named_molecule_in_place_detach_its_offset_caption()
-> Result<(), String> {
    // A loaded drawing can link a caption away from its computed position.
    // Reference edits reconcile the caption twice, as they always have: the
    // first pass moves it back, and the second detaches it because no named
    // atom moved.
    for stretch_other in [false, true] {
        let (mut app, _) = App::new();
        let mut doc = Document::from_json(include_bytes!(
            "../../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
        ))?;
        let named = [1, 2, 3];
        let a = doc.add_atom("C", Point::new(200., 0.));
        let b = doc.add_atom("C", Point::new(242., 0.));
        doc.add_bond(a, b, 1, "plain");
        let id = reshiki::molecule_names::show(
            &mut doc,
            &named,
            "ethan-1-ol".into(),
            "CCO".into(),
            Default::default(),
            true,
        )?;
        let caption = |doc: &Document| doc.annotations.iter().find(|a| a.id == id).cloned();
        // Insertion reconciles the new caption into its computed position.
        let shown = doc.clone();
        reshiki::molecule_names::reconcile(&mut doc, &shown);
        assert_eq!(doc.molecule_names.len(), 1);
        let computed = caption(&doc).ok_or("caption")?.position;
        if let Some(caption) = doc.annotations.iter_mut().find(|a| a.id == id) {
            caption.position = computed.offset(5., 5.);
        }
        let atoms =
            |doc: &Document| -> Vec<_> { named.iter().map(|id| doc.atom(*id).cloned()).collect() };
        let named_atoms = atoms(&doc);
        app.tab.doc = doc;
        app.tab.history = Default::default();
        if stretch_other {
            app.tab.selected = vec![a, b];
            app.sync_numeric_transforms();
            act(&mut app, Action::Length("21.6".into()));
            act(&mut app, Action::Stretch);
        } else {
            app.tab.selected = vec![1, 2];
            app.sync_numeric_transforms();
            act(&mut app, Action::Turn("0".into()));
            act(&mut app, Action::Rotate(false));
        }
        assert!(!app.error, "{}", app.status);
        assert_eq!(atoms(&app.tab.doc), named_atoms);
        let caption = caption(&app.tab.doc).ok_or("caption")?;
        assert_eq!(
            (caption.text.as_str(), caption.position),
            ("ethan-1-ol", computed)
        );
        assert!(app.tab.doc.molecule_names.is_empty());
        assert!(app.tab.history.can_undo());
    }
    Ok(())
}

#[test]
fn reference_actions_keep_parse_reference_scope_and_pivot_error_precedence() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    app.tab.selected.clear();
    act(&mut app, Action::Turn("invalid".into()));
    act(&mut app, Action::Rotate(false));
    assert_eq!(app.status, "Enter a finite rotation angle");
    act(&mut app, Action::Turn("90".into()));
    act(&mut app, Action::Rotate(false));
    assert_eq!(app.status, "Select a reference bond or edge");
    act(&mut app, Action::Target("invalid".into()));
    act(&mut app, Action::AlignTarget);
    assert_eq!(app.status, "Enter a finite target direction");
    act(&mut app, Action::Align(0.));
    assert_eq!(
        app.status,
        "Select both endpoints of a bond, or a shape with a straight edge"
    );
    app.tab.selected = vec![1, 2];
    act(&mut app, Action::Pivot(Pivot::Pinned));
    act(&mut app, Action::X("invalid".into()));
    act(&mut app, Action::Align(f64::NAN));
    assert_eq!(app.status, "Enter a finite pivot X coordinate");
    act(&mut app, Action::Scope(Scope::Selected));
    act(&mut app, Action::Align(f64::NAN));
    const PARTIAL_SCOPE: &str = "Select the complete connected fragment to keep its exterior bonds and stereochemistry intact";
    assert_eq!(app.status, PARTIAL_SCOPE);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
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

#[test]
fn composed_docked_nmr_tab_switch_restores_camera_and_reset_cancels_stretch() {
    use crate::app::nmr::Action as NmrAction;
    let mut app = fixture();
    let original = app.tab.doc.clone();
    app.tab.camera.center = Point::new(73., -29.);
    app.tab.camera.zoom = 2.5;
    let camera = app.tab.camera;
    let inspector = (app.inspector_open, app.inspector_tab);
    let _ = app.nmr_action(NmrAction::Open);
    let _ = app.nmr_action(NmrAction::Dock);
    act(&mut app, Action::StretchDrag);
    assert!(app.reference_stretch_active(1, 2));
    app.add_tab();
    assert_eq!(app.tool, Tool::Select);
    app.select_tab(0);
    assert_eq!(
        (app.tab.camera.center, app.tab.camera.zoom),
        (camera.center, camera.zoom)
    );
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Predict);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    act(&mut app, Action::StretchDrag);
    assert!(app.reference_stretch_active(1, 2));
    app.reset_tab();
    assert_eq!(app.tool, Tool::Select);
    assert!(app.tab.doc.atoms.is_empty());
    assert_eq!((app.inspector_open, app.inspector_tab), inspector);
    assert!(!app.tab.nmr.open);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn resetting_an_office_tab_preserves_docked_nmr_camera_and_cancels_reference_stretch() {
    use crate::app::{InspectorTab, nmr::Action as NmrAction};
    use crate::session::{Binding, Host, Lease, Phase};
    use std::path::PathBuf;

    // Extend the optional Windows46 reset regression with a real stretch
    // owner and real NMR dock actions, rather than setting their flags.
    let drawing = fixture().tab.doc.clone();
    let (mut app, _) = App::new();
    let path = PathBuf::from("nmr-reference-office.rsk");
    let lease = Lease::new([7; 16], || true);
    let _ = app.update(Message::OfficePrepared(
        Binding {
            path: path.clone(),
            host: Host::Office,
            lease: Some(lease.clone()),
        },
        Some((
            path.clone(),
            Ok(crate::app::files::Prepared::Native(Box::new(drawing))),
        )),
    ));
    let original = app.tab.id;
    let epoch = app.tab.file_epoch;
    let before = app.tab.doc.clone();
    app.tab.doc.translate(&[3], 5., 0.);
    app.changed(before);
    app.tab.selected = vec![1, 2];
    let source = app.tab.doc.clone();
    let history = (
        app.tab.history.undo_frames().cloned().collect::<Vec<_>>(),
        app.tab.history.redo_frames().cloned().collect::<Vec<_>>(),
    );
    assert!(app.tab.history.can_undo());
    let inspector = (app.inspector_open, app.inspector_tab);
    let _ = app.update(Message::Nmr(NmrAction::Open));
    let _ = app.update(Message::Nmr(NmrAction::Dock));
    app.tab.camera.center = Point::new(73., -29.);
    app.tab.camera.zoom = 2.5;
    let camera = app.tab.camera;
    act(&mut app, Action::StretchDrag);
    assert!(app.reference_stretch_active(1, 2));
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);

    app.reset_tab();

    let destination = app.tab.id;
    assert_ne!(destination, original);
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.tool, Tool::Select);
    assert!(!app.reference_stretch_active(1, 2));
    assert_eq!((app.inspector_open, app.inspector_tab), inspector);
    assert!(app.tab.doc.object_ids().next().is_none());
    assert!(app.tab.doc.generic.is_empty());
    assert!(app.tab.office.is_none());
    assert!(app.tab.path.is_none());
    assert!(!app.tab.nmr.open);
    assert_eq!(app.tab.history.frames(), 0);
    let blank = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::StretchBond {
        fixed: 1,
        moving: 2,
        length: 63.,
    }));
    assert_eq!(app.tab.doc, blank);
    assert_eq!(app.tab.history.frames(), 0);
    let parked = app
        .tabs
        .background
        .iter()
        .find(|tab| tab.id == original)
        .unwrap();
    assert_eq!(parked.doc, source);
    assert_eq!(parked.file_epoch, epoch);
    let (docked, restore_dock, _) = parked.nmr.docking_state_for_test();
    assert!(parked.nmr.open && docked && restore_dock);
    assert_eq!(parked.office.as_ref().unwrap().path, path);
    assert_eq!(lease.phase(), Phase::Open);

    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(original)));
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert_eq!(
        (app.tab.camera.center, app.tab.camera.zoom),
        (camera.center, camera.zoom)
    );
    assert_eq!(app.tab.nmr.docking_state_for_test().2, Some(inspector));
    assert_eq!(app.tool, Tool::Select);
    assert!(!app.reference_stretch_active(1, 2));
    let _ = app.update(Message::Canvas(Edit::StretchBond {
        fixed: 1,
        moving: 2,
        length: 63.,
    }));
    assert_eq!(app.tab.doc, source);
    assert_eq!(app.tab.file_epoch, epoch);
    assert_eq!(
        (
            app.tab.history.undo_frames().cloned().collect::<Vec<_>>(),
            app.tab.history.redo_frames().cloned().collect::<Vec<_>>()
        ),
        history,
    );
    assert_eq!(app.drawing_save_target(false).0, Some(path));
    assert_eq!(
        app.tab
            .office
            .as_ref()
            .unwrap()
            .lease
            .as_ref()
            .unwrap()
            .token,
        lease.token
    );
    assert_eq!(lease.phase(), Phase::Open);
}

#[tokio::test]
#[ignore = "Opt-in real renderer reference accessibility ids"]
async fn reference_grid_fields_publish_unique_ids_with_units_in_their_names() -> Result<(), String>
{
    use iced::advanced::{renderer::Headless, widget::operation};
    use reshiki::accessibility::Collect;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut app = fixture();
    act(&mut app, Action::Open(true));
    act(&mut app, Action::Pivot(Pivot::Pinned));
    let size = iced::Size::new(268., 1600.);
    let mut ui = iced_runtime::UserInterface::build(
        app.reference_transform_panel(),
        size,
        iced_runtime::user_interface::Cache::new(),
        &mut renderer,
    );
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    let snapshot = collect.snapshot();
    assert!(
        snapshot.duplicate_ids.is_empty(),
        "{:?}",
        snapshot.duplicate_ids
    );
    let node = |id: &str| {
        snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("Missing {id}"))
    };
    for (id, name) in [
        ("reference-x", "Pivot X (pt)"),
        ("reference-y", "Pivot Y (pt)"),
        ("reference-direction", "Direction (°)"),
        ("reference-turn", "Turn (°)"),
        ("reference-length", "Length (pt)"),
    ] {
        let node = node(id);
        assert_eq!(node.name, name);
        assert!(node.visible_bounds.is_some(), "Invisible {id}");
    }
    for id in [
        "reference-controls",
        "reference-pin-center",
        "reference-horizontal",
        "reference-vertical",
        "reference-to-angle",
        "reference-rotate",
        "reference-copy-rotate",
        "reference-swap-fixed",
        "reference-stretch",
        "reference-stretch-drag",
    ] {
        assert!(node(id).visible_bounds.is_some(), "Invisible {id}");
    }
    assert_eq!(node("reference-controls").expanded, Some(true));
    assert_eq!(node("reference-turn").value.as_deref(), Some("90"));
    Ok(())
}
