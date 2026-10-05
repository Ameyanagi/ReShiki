use super::*;
fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::document::Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app
}
#[test]
fn page_setup_is_an_explicit_atomic_edit_and_does_not_scale_objects() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    app.tab.saved = before.clone();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::Letter)));
    let _ = app.update(Message::Pages(Action::Input(Field::Columns, "2".into())));
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Pages(Action::Apply));
    let expected = app.tab.doc.clone();
    assert!(app.dirty());
    assert_eq!(app.recovery_document().page_layout, expected.page_layout);
    assert_eq!(expected.page_layout.as_ref().unwrap().count(), 2);
    assert_eq!(expected.atoms, before.atoms);
    assert_eq!(expected.bonds, before.bonds);
    assert_eq!(expected.page_layout.as_ref().unwrap().width_pt, 612.);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Input(Field::Width, "bad".into())));
    let _ = app.update(Message::Pages(Action::Apply));
    assert_eq!(app.tab.doc, expected);
    assert!(app.error);
    let _ = app.update(Message::Pages(Action::Cancel));
    assert_eq!(app.tab.doc, expected);
}
#[test]
fn setup_preserves_concurrent_drawing_edits_but_rejects_a_changed_file() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("N", Point::new(100., 0.));
    app.changed(before);
    let atoms = app.tab.doc.atoms.clone();
    let _ = app.update(Message::Pages(Action::Apply));
    assert_eq!(app.tab.doc.atoms, atoms);
    let _ = app.update(Message::Pages(Action::Open));
    app.tab.file_epoch += 1;
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Pages(Action::Apply));
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
}
#[test]
fn page_navigation_is_view_only_and_centering_is_undoable() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Input(Field::Rows, "2".into())));
    let _ = app.update(Message::Pages(Action::Apply));
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::Pages(Action::Navigate(1)));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.pages.active, 1);
    let center = app.tab.camera.center;
    let _ = app.update(Message::Viewport(iced::Size::new(800., 500.)));
    assert_eq!(app.tab.camera.center, center);
    app.tab.selected = vec![1];
    let _ = app.update(Message::Pages(Action::Center(true)));
    assert_ne!(app.tab.doc.atoms, before.atoms);
    assert_eq!(app.tab.doc.bonds, before.bonds);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Pages(Action::Remove));
    assert!(app.tab.doc.page_layout.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn selecting_page_artwork_keeps_page_controls_and_centers_only_the_selection() {
    use crate::canvas::Edit;
    use reshiki::{
        arrows::{ArrowStyle, Preset as ArrowPreset},
        document::{Annotation, Arrow},
        graphics::{Graphic, GraphicKind, GraphicStyle},
    };
    let mut app = ready();
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(10., 35.),
        text: "Methanol".into(),
        format: Default::default(),
    });
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(200., 0.),
        Point::new(300., 0.),
        ArrowPreset::Forward,
        ArrowStyle::default(),
    ));
    let graphic = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        graphic,
        GraphicKind::Rectangle,
        Point::new(200., 100.),
        Point::new(300., 150.),
        GraphicStyle::default(),
        Default::default(),
        false,
    ));
    app.tab.doc.page_layout = Some(Layout {
        columns: 2,
        ..Default::default()
    });
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::Pages(Action::Show));
    let _ = app.update(Message::Pages(Action::Fit(None)));
    let _ = app.update(Message::SelectAll);
    assert_eq!(app.tab.selected, before.all_ids());
    assert_eq!(app.inspector_tab, InspectorTab::Pages);
    for ids in [
        vec![caption],
        vec![arrow],
        vec![graphic],
        vec![1, 2, caption],
    ] {
        let _ = app.update(Message::Canvas(Edit::Select(ids.clone())));
        assert_eq!(app.tab.selected, ids);
        assert_eq!(app.inspector_tab, InspectorTab::Pages);
        assert!(app.inspector_open);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
    }
    let _ = app.update(Message::Pages(Action::Navigate(1)));
    let _ = app.update(Message::Pages(Action::Center(true)));
    assert_eq!(app.tab.selected, vec![1, 2, caption]);
    assert_eq!(app.inspector_tab, InspectorTab::Pages);
    assert_eq!(app.tab.doc.arrows, before.arrows);
    assert_eq!(app.tab.doc.graphics, before.graphics);
    assert_ne!(app.tab.doc.atoms, before.atoms);
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
    let (page_lo, page_hi) = app
        .tab
        .doc
        .page_layout
        .as_ref()
        .unwrap()
        .content_bounds(1)
        .unwrap();
    assert!(((lo.x + hi.x) - (page_lo.x + page_hi.x)).abs() < 0.01);
    assert!(((lo.y + hi.y) - (page_lo.y + page_hi.y)).abs() < 0.01);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.inspector_tab, InspectorTab::Pages);
    assert!(!app.tab.history.can_undo());
}
