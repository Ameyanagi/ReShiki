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
    // A size applies at once and keeps the editor open.
    let _ = app.update(Message::Pages(Action::Preset(Preset::Letter)));
    let letter = app.tab.doc.clone();
    assert_eq!(letter.page_layout.as_ref().unwrap().width_pt, 612.);
    assert!(app.tab.pages.editor.is_some());
    let _ = app.update(Message::Pages(Action::Input(Field::Columns, "2".into())));
    assert_eq!(app.tab.doc, letter, "typing alone changes nothing");
    let _ = app.update(Message::Pages(Action::Apply));
    assert!(app.tab.pages.editor.is_none());
    let expected = app.tab.doc.clone();
    assert!(app.dirty());
    assert_eq!(app.recovery_document().page_layout, expected.page_layout);
    assert_eq!(expected.page_layout.as_ref().unwrap().count(), 2);
    assert_eq!(expected.atoms, before.atoms);
    assert_eq!(expected.bonds, before.bonds);
    assert_eq!(expected.page_layout.as_ref().unwrap().width_pt, 612.);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, letter);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
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

#[test]
fn page_setup_closes_the_view_strip_and_landscape_applies_at_once() {
    let mut app = ready();
    app.view_open = true;
    let _ = app.update(Message::Pages(Action::Show));
    assert!(!app.view_open, "the strip's Done would be a second way out");
    let _ = app.update(Message::Pages(Action::Preset(Preset::A4)));
    let _ = app.update(Message::Pages(Action::Landscape(false)));
    let portrait = app.tab.doc.clone();
    let _ = app.update(Message::Pages(Action::Landscape(true)));
    let layout = app.tab.doc.page_layout.as_ref().unwrap();
    assert!(layout.width_pt > layout.height_pt);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, portrait);
}

#[test]
fn a_size_choice_keeps_typed_margins_and_grid_typed() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::A4)));
    let _ = app.update(Message::Pages(Action::Input(Field::Columns, "2".into())));
    let _ = app.update(Message::Pages(Action::Input(Field::Rows, "x".into())));
    let _ = app.update(Message::Pages(Action::Landscape(true)));
    let layout = app.tab.doc.page_layout.clone().unwrap();
    assert!(layout.width_pt > layout.height_pt);
    assert_eq!(
        (layout.columns, layout.rows),
        (1, 1),
        "the grid is not applied"
    );
    let editor = app.tab.pages.editor.as_ref().unwrap();
    assert_eq!((editor.columns.as_str(), editor.rows.as_str()), ("2", "x"));
    assert!(!app.error, "{}", app.status);
}

#[test]
fn margins_too_wide_for_a_smaller_size_fall_back_to_the_defaults() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::A4)));
    let _ = app.update(Message::Pages(Action::Input(Field::Left, "80".into())));
    let _ = app.update(Message::Pages(Action::Input(Field::Right, "80".into())));
    let _ = app.update(Message::Pages(Action::Apply));
    let _ = app.update(Message::Pages(Action::Show));
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::A5)));
    assert!(!app.error, "{}", app.status);
    let layout = app.tab.doc.page_layout.as_ref().unwrap();
    assert_eq!(layout.preset(), Preset::A5);
    assert_eq!(layout.margins, reshiki::pages::Margins::default());
}

#[test]
fn a_toc_preset_is_one_sheet_framing_page_one() {
    let mut app = ready();
    app.tab.doc.add_atom("C", Point::new(0., 0.));
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Input(Field::Columns, "2".into())));
    let _ = app.update(Message::Pages(Action::Apply));
    let _ = app.update(Message::Pages(Action::Navigate(1)));
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::AcsToc)));
    let layout = app.tab.doc.page_layout.as_ref().unwrap();
    assert_eq!(layout.count(), 1);
    assert_eq!(app.tab.pages.active, 0);
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc.page_layout.as_ref().unwrap().count(),
        2,
        "one Undo step"
    );
}

#[test]
fn page_setup_on_a_paged_drawing_also_closes_the_view_strip() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Apply));
    assert!(app.tab.doc.page_layout.is_some());
    app.view_open = true;
    let _ = app.update(Message::Pages(Action::Show));
    assert!(!app.view_open);
}

#[test]
fn landscape_turns_a_typed_custom_size() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::A4)));
    let _ = app.update(Message::Pages(Action::Input(Field::Width, "100".into())));
    let _ = app.update(Message::Pages(Action::Input(Field::Height, "200".into())));
    let _ = app.update(Message::Pages(Action::Landscape(true)));
    let layout = app.tab.doc.page_layout.as_ref().unwrap();
    let mm = |pt: f32| (pt * 25.4 / 72.).round();
    assert_eq!((mm(layout.width_pt), mm(layout.height_pt)), (200., 100.));
}

#[test]
fn landscape_with_a_mistyped_width_turns_the_applied_size() {
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Preset(Preset::A4)));
    let mm = |pt: f32| (pt * 25.4 / 72.).round();
    // Not a number, or out of the supported 12.7–1016 mm range.
    for (width, turned) in [("x", true), ("2000", false), ("inf", true)] {
        let _ = app.update(Message::Pages(Action::Input(Field::Width, width.into())));
        let _ = app.update(Message::Pages(Action::Input(Field::Height, "100".into())));
        let _ = app.update(Message::Pages(Action::Landscape(turned)));
        let layout = app.tab.doc.page_layout.as_ref().unwrap();
        let expected = if turned { (297., 210.) } else { (210., 297.) };
        assert_eq!(
            (mm(layout.width_pt), mm(layout.height_pt)),
            expected,
            "{width}"
        );
    }
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn printing_uses_the_applied_pages_and_keeps_typed_page_settings() {
    use crate::app::{InspectorTab, printing::Action as Print};
    use reshiki::printing::Scope;
    let mut app = ready();
    let _ = app.update(Message::Pages(Action::Open));
    let _ = app.update(Message::Pages(Action::Input(
        Field::Width,
        "invalid".into(),
    )));
    let _ = app.update(Message::Pages(Action::Input(Field::Columns, "3".into())));
    let before = app.tab.doc.clone();
    // A mistyped, unapplied width neither blocks nor changes the print.
    let _ = app.print_action(Print::Start(Scope::Document));
    assert!(app.printing.active.is_some(), "{}", app.status);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.tab.pages.editor.as_ref().unwrap().columns, "3");

    // An unpaged drawing wider than A4 brings Page setup back, as typed.
    app.printing.active = None;
    app.tab.doc.add_atom("N", Point::new(5000., 0.));
    app.inspector_tab = InspectorTab::Properties;
    let _ = app.print_action(Print::Start(Scope::Document));
    assert!(app.printing.active.is_none());
    assert!(app.error, "{}", app.status);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Pages);
    let editor = app.tab.pages.editor.as_ref().unwrap();
    assert_eq!(
        (editor.width.as_str(), editor.columns.as_str()),
        ("invalid", "3")
    );
}
