use super::*;
use crate::canvas::{Edit, Tool};
use reshiki::{
    document::Document,
    graphics::{BracketSides, GraphicKind, GraphicStyle},
};
fn plate() -> Graphic {
    crate::canvas::tlc::dragged(
        1,
        GraphicKind::TlcPlate,
        Point::new(0., 0.),
        Point::new(140., 600.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    )
}
fn ready(g: Graphic) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.graphics = vec![g];
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = vec![1];
    app.sync_graphics();
    app
}
fn key(app: &App) -> Target {
    app.tab.tlc.target.unwrap()
}
fn edit(app: &mut App, field: Field, text: &str) {
    let k = key(app);
    let _ = app.update(Message::Tlc(Action::Draft(k, field, text.into())));
    let _ = app.update(Message::Tlc(Action::Apply(k, field)));
}
fn one_undo(app: &mut App, before: &Document) {
    assert!(!app.error, "{}", app.status);
    let after = app.tab.doc.clone();
    assert_ne!(&after, before);
    let _ = app.update(Message::Undo);
    assert_eq!(&app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}
#[test]
fn tool_arms_without_editing_and_normal_release_commits_the_preview_as_one_undo() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.saved = app.tab.doc.clone();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::TlcPlate)));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.frames(), 0);
    let (a, b) = (Point::new(20., 30.), Point::new(160., 630.));
    let preview = crate::canvas::tlc::dragged(
        1,
        GraphicKind::TlcPlate,
        a,
        b,
        app.tab.graphic_style.clone(),
        app.tab.bracket_sides,
        false,
    );
    let _ = app.update(Message::Canvas(Edit::Graphic(a, b, false)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.graphics.as_slice(), [preview]);
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.selected, [1]);
    assert!(app.tab.tlc.target.is_some());
    assert_eq!(app.tab.history.frames(), 1);
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}
#[test]
fn native_rf_edit_updates_standard_label_and_preserves_dimensions_in_one_undo() {
    let mut app = ready(plate());
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Toggle(k, Switch::Rf, true)));
    let before = app.tab.doc.clone();
    let dim = before.graphics[0].tlc.as_deref().unwrap().lanes[0].spots[0].width_pt;
    edit(&mut app, Field::Rf, "0.28");
    let s = &app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes[0].spots[0];
    assert_eq!(s.rf, 0.28);
    assert_eq!(s.width_pt, dim);
    assert_eq!(s.height_pt, dim);
    assert_eq!(s.label.as_ref().unwrap().text, "Rf = 0.28");
    one_undo(&mut app, &before);
}
#[test]
fn unchanged_display_keeps_exact_native_dimensions_fractions_and_redo() {
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().origin_fraction = 0.0007;
    let mut app = ready(g);
    let original = app.tab.doc.clone();
    for f in [
        Field::Origin,
        Field::Front,
        Field::Line,
        Field::Rf,
        Field::Width,
        Field::Height,
        Field::PlateColor,
        Field::SpotColor,
    ] {
        let k = key(&app);
        let _ = app.update(Message::Tlc(Action::Apply(k, f)));
        assert_eq!(app.tab.doc, original, "{f:?}");
        assert_eq!(app.tab.history.frames(), 0);
    }
    edit(&mut app, Field::Rf, "0.5");
    let _ = app.update(Message::Undo);
    assert!(app.tab.history.can_redo());
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Apply(k, Field::Width)));
    assert!(app.tab.history.can_redo());
    assert_eq!(app.tab.doc, original);
    let s = &app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes[0].spots[0];
    assert_eq!(s.width_pt.to_bits(), (325095f64 / 65536.).to_bits());
}
#[test]
fn invalid_drafts_preserve_document_input_selection_and_redo() {
    let mut app = ready(plate());
    edit(&mut app, Field::Rf, "0.5");
    let _ = app.update(Message::Undo);
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    for (field, value) in [
        (Field::Rf, "NaN"),
        (Field::Rf, "1.1"),
        (Field::Width, "0"),
        (Field::Height, "32768"),
        (Field::Origin, "95"),
        (Field::PlateColor, "bad"),
    ] {
        edit(&mut app, field, value);
        assert!(app.error, "{field:?}");
        assert_eq!(app.tab.tlc.input(field), value);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [1]);
        assert!(app.tab.history.can_redo());
    }
}
#[test]
fn stale_part_revision_and_file_targets_never_retarget_an_edit() {
    let mut app = ready(plate());
    let old = key(&app);
    let _ = app.update(Message::Tlc(Action::Draft(old, Field::Rf, "0.9".into())));
    let _ = app.update(Message::Tlc(Action::Lane(old, 1)));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tlc(Action::Apply(old, Field::Rf)));
    assert_eq!(app.tab.doc, before);
    assert_eq!(key(&app).lane, 1);
    let _ = app.update(Message::Tlc(Action::Draft(old, Field::Rf, "0.8".into())));
    assert_eq!(app.tab.tlc.input(Field::Rf), "0.15");
    let current = key(&app);
    edit(&mut app, Field::Rf, "0.7");
    let _ = app.update(Message::Undo);
    let restored = app.tab.doc.clone();
    let _ = app.update(Message::Tlc(Action::Toggle(current, Switch::Filled, false)));
    assert_eq!(app.tab.doc, restored);
    assert!(app.tab.history.can_redo());
    let old = key(&app);
    app.tab.file_epoch += 1;
    app.sync_graphics();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tlc(Action::DeleteLane(old)));
    assert_eq!(app.tab.doc, before);
}
#[test]
fn lane_and_spot_edits_keep_order_metadata_and_one_action_undo() {
    let mut app = ready(plate());
    let before = app.tab.doc.clone();
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::AddLane(k)));
    assert_eq!(key(&app).lane, 3);
    assert_eq!(key(&app).spot, None);
    assert_eq!(
        app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes.len(),
        4
    );
    one_undo(&mut app, &before);
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Lane(k, 3)));
    let before = app.tab.doc.clone();
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::AddSpot(k)));
    let s = &app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes[3].spots[0];
    assert_eq!(s.rf, 0.5);
    assert!(s.style.fill.is_some());
    one_undo(&mut app, &before);
    let before = app.tab.doc.clone();
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::DeleteSpot(k)));
    assert!(
        app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes[3]
            .spots
            .is_empty()
    );
    one_undo(&mut app, &before);
}
#[test]
fn spot_paint_and_outline_keep_ownership_and_have_independent_undo() {
    let mut app = ready(plate());
    let before = app.tab.doc.clone();
    let color = Color::Custom([17, 126, 108]);
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Paint(k, PaintScope::Spot, color)));
    let g = &app.tab.doc.graphics[0];
    let s = &g.tlc.as_deref().unwrap().lanes[0].spots[0];
    assert_eq!(s.style.stroke, color);
    assert_eq!(s.style.fill, Some(color));
    assert_eq!(g.style.stroke, Color::Ink);
    one_undo(&mut app, &before);
    let before = app.tab.doc.clone();
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Toggle(k, Switch::Filled, false)));
    assert!(
        app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes[0].spots[0]
            .style
            .fill
            .is_none()
    );
    one_undo(&mut app, &before);
}
#[test]
fn themed_hex_noop_keeps_semantic_colors_and_canvas_change_refreshes_drafts() {
    let mut g = plate();
    g.style.stroke = Color::Palette(reshiki::palette::Hue::Blue, reshiki::palette::Row::Strong);
    let spot = &mut g.tlc.as_deref_mut().unwrap().lanes[0].spots[0];
    spot.style.stroke = Color::Palette(reshiki::palette::Hue::Amber, reshiki::palette::Row::Strong);
    spot.style.fill = Some(spot.style.stroke);
    let mut app = ready(g);
    app.tab.doc.color_theme = reshiki::canvas_theme::ColorTheme::Pastel;
    app.tab.tlc.target = None;
    app.sync_graphics();
    let palette = reshiki::palette::Palette::of(&app.tab.doc);
    let before = app.tab.doc.clone();
    assert_eq!(
        app.tab.tlc.input(Field::PlateColor),
        reshiki::palette::hex(palette.rgb(app.tab.doc.graphics[0].style.stroke))
    );
    for field in [Field::PlateColor, Field::SpotColor] {
        let k = key(&app);
        let _ = app.update(Message::Tlc(Action::Apply(k, field)));
        assert_eq!(app.tab.doc, before);
    }
    app.tab.doc.graphics[0].style.stroke = Color::Ink;
    app.tab.tlc.target = None;
    app.sync_graphics();
    let _ = app.update(Message::CanvasTheme(
        reshiki::canvas_theme::CanvasTheme::Dark,
    ));
    assert_eq!(app.tab.tlc.input(Field::PlateColor), "#FFFFFF");
    let before = app.tab.doc.clone();
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Apply(k, Field::PlateColor)));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.doc.graphics[0].style.stroke, Color::Ink);
    assert_eq!(app.tab.history.frames(), 0);
}

#[test]
fn generated_rf_baseline_respects_a_skewed_reflected_frame() {
    let mut g = plate();
    g.axis_x = Point::new(-140., 20.);
    g.axis_y = Point::new(40., 600.);
    let mut app = ready(g);
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Toggle(k, Switch::Rf, true)));
    assert!(!app.error, "{}", app.status);
    let g = &app.tab.doc.graphics[0];
    let t = g.tlc.as_deref().unwrap();
    let s = &t.lanes[0].spots[0];
    let a = &t.labels(g)[0];
    let l = reshiki::typography::layout(&a.text, &a.format);
    let first = &l.fragments[0];
    let actual = a.position.offset(
        first.position.x,
        first.position.y + reshiki::style::text_ascent(&first.style),
    );
    let (x, y) = t.basis(g);
    let r = reshiki::style::DEFAULT.world((s.width_pt + f64::from(s.style.width_pt)) as f32) * 0.5;
    let expected = t
        .spot_center(g, 0, s.rf)
        .offset(x.x * r - y.x * r, x.y * r - y.y * r);
    assert!(actual.distance(expected) < 0.001);
}
#[test]
fn dedicated_properties_and_generic_style_guard_keep_plate_data_valid() {
    let mut app = ready(plate());
    assert!(app.tlc_panel_visible());
    assert!(!app.graphic_panel_visible());
    let _ = app.tlc_panel();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(super::super::graphics::Action::Style(
        reshiki::graphics::GraphicChange::Fill(Some(Color::Ink)),
    )));
    assert_eq!(app.tab.doc, before);
    app.tab.doc.validate().unwrap();
}
