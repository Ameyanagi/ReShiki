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
#[tokio::test]
async fn native_wide_plate_properties_keep_calibrated_geometry_and_editable_exchange() {
    let engine = reshiki::engine::LocalEngine::in_process();
    for (format, input) in [
        (
            "cdx",
            include_bytes!("../../../crates/io/tests/fixtures/tlc/chemdraw26-reaction-check.cdx")
                .as_slice(),
        ),
        (
            "cdxml",
            include_bytes!("../../../crates/io/tests/fixtures/tlc/chemdraw26-reaction-check.cdxml")
                .as_slice(),
        ),
    ] {
        let (doc, warnings) = reshiki::formats::read_with_warnings(input.to_vec(), format, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{format}: {warnings:?}");
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.selected = vec![doc.graphics[0].id];
        app.tab.saved = doc.clone();
        app.tab.doc = doc;
        app.sync_graphics();
        let k = key(&app);
        let _ = app.update(Message::Tlc(Action::Lane(k, 2)));
        let before = app.tab.doc.clone();
        let g = &before.graphics[0];
        let t = g.tlc.as_deref().unwrap();
        let original = &t.lanes[2].spots[0];
        let label = original.label.as_ref().unwrap();
        assert_eq!(label.text, "Rf = 0.32");
        assert_eq!(label.format.style.size_pt, 9.);
        assert!(
            label
                .format
                .spans
                .iter()
                .any(|s| s.style.script == Script::Subscript)
        );
        edit(&mut app, Field::Rf, "0.48");
        let g = &app.tab.doc.graphics[0];
        let t = g.tlc.as_deref().unwrap();
        let spot = &t.lanes[2].spots[0];
        assert_eq!(spot.rf, 0.48);
        assert_eq!(spot.width_pt, original.width_pt);
        assert_eq!(spot.height_pt, original.height_pt);
        assert_eq!(spot.label.as_ref().unwrap().text, "Rf = 0.48");
        assert_eq!(spot.label.as_ref().unwrap().format, label.format);
        assert_eq!(spot.label.as_ref().unwrap().offset, label.offset);
        assert_eq!(
            t.lanes[0],
            before.graphics[0].tlc.as_deref().unwrap().lanes[0]
        );
        assert_eq!(
            t.lanes[1],
            before.graphics[0].tlc.as_deref().unwrap().lanes[1]
        );
        let center = t.spot_center(g, 2, spot.rf);
        let world = |pt| reshiki::style::DEFAULT.world(pt);
        // The fixture's 240 pt bottom span has lane centres at 43⅓, 120,
        // and 196⅔ pt. Rf 0.48 lies 103.2 pt below the top edge.
        assert!(
            center.distance(g.origin.offset(world(196.66667), world(103.2))) < 0.002,
            "{format}: {center:?}"
        );
        assert_eq!(app.tab.history.frames(), 1);
        one_undo(&mut app, &before);
        for target in ["rsk", "cdx", "cdxml"] {
            let output = reshiki::formats::write_with_engine(app.tab.doc.clone(), target, &engine)
                .await
                .unwrap();
            let (back, warnings) = reshiki::formats::read_with_warnings(output, target, &engine)
                .await
                .unwrap();
            assert!(warnings.is_empty(), "{format} to {target}: {warnings:?}");
            let t = back.graphics[0].tlc.as_deref().unwrap();
            assert!(t.show_borders && t.show_origin && t.show_solvent_front);
            assert_eq!(
                t.lanes.iter().map(|l| l.spots.len()).collect::<Vec<_>>(),
                [1, 2, 1]
            );
            let spot = &t.lanes[2].spots[0];
            assert_eq!(spot.rf, 0.48);
            assert_eq!(spot.width_pt, original.width_pt);
            assert_eq!(spot.height_pt, original.height_pt);
            assert!(spot.show_rf);
            let label = spot.label.as_ref().unwrap();
            assert_eq!(label.text, "Rf = 0.48");
            assert_eq!(label.format.style.size_pt, 9.);
            assert!(
                label
                    .format
                    .spans
                    .iter()
                    .any(|s| s.style.script == Script::Subscript)
            );
        }
    }
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

#[test]
fn mixed_graphic_selection_keeps_generic_properties_and_edits_only_the_rectangle() {
    let mut app = ready(plate());
    let rectangle = Graphic::dragged(
        2,
        GraphicKind::Rectangle,
        Point::new(200., 0.),
        Point::new(300., 100.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    app.tab.doc.graphics.push(rectangle);
    app.tab.selected.push(2);
    app.sync_graphics();
    assert!(app.tlc_panel_visible());
    assert!(app.graphic_panel_visible());
    assert!(app.tlc_target().is_none());
    let before = app.tab.doc.clone();
    let color = Color::Custom([17, 126, 108]);
    let _ = app.update(Message::Graphics(super::super::graphics::Action::Style(
        reshiki::graphics::GraphicChange::Fill(Some(color)),
    )));
    assert_eq!(app.tab.doc.graphics[0], before.graphics[0]);
    assert_eq!(app.tab.doc.graphics[1].style.fill, Some(color));
    one_undo(&mut app, &before);
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::TlcPlate)));
    assert!(app.tlc_panel_visible());
    assert!(!app.graphic_panel_visible());
}

#[test]
fn mixed_graphic_selection_syncs_generic_fields_from_the_first_ordinary_graphic() {
    use super::super::graphics::Action as Graphics;
    let mut source = plate();
    source.style.width_pt = 0.8;
    source.style.stroke = Color::Custom([0xb1, 0x23, 0x45]);
    let mut app = ready(source);
    let target = key(&app);
    let _ = app.update(Message::Tlc(Action::Lane(target, 1)));
    let target = key(&app);
    let _ = app.update(Message::Tlc(Action::Details(target, true)));
    let fields = app.tab.tlc.fields.clone();
    app.sync_graphics();
    assert_eq!(key(&app), target);
    assert_eq!(app.tab.tlc.fields, fields);
    assert!(app.tab.tlc.plate_settings);
    assert_eq!(app.tab.tlc.input(Field::Line), "0.8");
    assert_eq!(app.tab.tlc.input(Field::PlateColor), "#B12345");
    assert_eq!(app.tab.graphic_style, app.tab.doc.graphics[0].style);

    let mut rectangle = Graphic::dragged(
        2,
        GraphicKind::Rectangle,
        Point::new(200., 0.),
        Point::new(300., 100.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    rectangle.style.width_pt = 2.5;
    rectangle.style.stroke = Color::Custom([0x11, 0x7e, 0x6c]);
    rectangle.style.fill = Some(Color::Custom([0xdc, 0xef, 0xe9]));
    let style = rectangle.style.clone();
    // The protected plate leads both document and selection order.
    app.tab.doc.graphics.push(rectangle);
    app.tab.selected.push(2);
    app.sync_graphics();
    assert!(app.tlc_panel_visible() && app.graphic_panel_visible());
    assert!(app.tlc_target().is_none());
    assert_eq!(app.tab.graphic_style, style);
    assert_eq!(app.tab.graphic_width_input, "2.5");
    assert_eq!(app.tab.graphic_stroke_input, "#117E6C");
    assert_eq!(app.tab.graphic_fill_input, "#DCEFE9");

    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::Graphics(Graphics::ApplyWidth));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.tab.doc, before,
        "Unchanged Apply keeps the rectangle's width"
    );
    assert_eq!(app.tab.revision, revision);
    for (draft, apply) in [
        (Graphics::Width("3.25".into()), Graphics::ApplyWidth),
        (Graphics::Stroke("#224466".into()), Graphics::ApplyStroke),
    ] {
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Graphics(draft));
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Graphics(apply));
        assert_eq!(app.tab.doc.graphics[0], before.graphics[0]);
        assert_eq!(app.tab.graphic_style, app.tab.doc.graphics[1].style);
        assert_eq!(app.tab.graphic_width_input, "3.25");
        one_undo(&mut app, &before);
    }
    assert_eq!(app.tab.doc.graphics[1].style.width_pt, 3.25);
    assert_eq!(app.tab.graphic_stroke_input, "#224466");
    assert_eq!(app.tab.graphic_fill_input, "#DCEFE9");
    app.tab.selected = vec![1];
    app.sync_graphics();
    assert!(app.tlc_panel_visible() && !app.graphic_panel_visible());
    assert_eq!(key(&app).graphic, 1);
    assert_eq!(app.tab.tlc.input(Field::Line), "0.8");
    assert_eq!(app.tab.tlc.input(Field::PlateColor), "#B12345");
    assert_eq!(app.tab.graphic_style, app.tab.doc.graphics[0].style);
}

#[tokio::test]
#[ignore = "Opt-in real TLC widget metadata and native action regression"]
async fn native_lane_spot_and_toggle_controls_expose_values_and_edit_the_chosen_part() {
    use iced::advanced::{
        renderer::Headless,
        widget::{Operation, operation},
    };
    use iced::{Rectangle, Size};
    use iced_runtime::{UserInterface, user_interface::Cache};
    use reshiki::accessibility::{
        Activate, Collect, FocusControl, Role, SetValue, Snapshot, tree::NativeTree,
    };

    fn operate<T>(app: &App, renderer: &mut iced::Renderer, operation: &mut dyn Operation<T>) {
        let mut ui = UserInterface::build(
            app.tlc_panel(),
            Size::new(340., 1200.),
            Cache::new(),
            renderer,
        );
        ui.operate(renderer, &mut operation::black_box(operation));
    }
    fn snapshot(app: &App, renderer: &mut iced::Renderer) -> Snapshot {
        let viewport = Rectangle::with_size(Size::new(340., 1200.));
        let mut collect = Collect::new(viewport);
        operate(app, renderer, &mut collect);
        let snapshot = collect.snapshot().clone();
        assert!(snapshot.duplicate_ids.is_empty());
        NativeTree::default()
            .update(&snapshot, "ReShiki", viewport, 2.)
            .unwrap();
        snapshot
    }
    fn choose(app: &mut App, renderer: &mut iced::Renderer, id: &str, value: &str) {
        let mut select = SetValue::<Message>::new(id, value.into());
        operate(app, renderer, &mut select);
        let operation::Outcome::Some(message) = select.finish() else {
            panic!("native selection {id}")
        };
        let _ = app.update(message);
    }
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let mut app = ready(plate());
    let initial = snapshot(&app, &mut renderer);
    for (id, name, value) in [
        ("tlc-lane", "Lane", "Lane 1 · 1 spot"),
        ("tlc-spot", "Spot", "Spot 1 · Rf 0.15"),
    ] {
        let node = initial.nodes.iter().find(|node| node.id == id).expect(id);
        assert_eq!(node.name, name);
        assert_eq!(node.role, Role::ComboBox);
        assert_eq!(node.value.as_deref(), Some(value));
        assert!(node.enabled);
    }
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::Details(k, true)));
    let settings = snapshot(&app, &mut renderer);
    for id in ["tlc-borders", "tlc-show-origin", "tlc-show-front"] {
        let node = settings.nodes.iter().find(|node| node.id == id).expect(id);
        assert_eq!(node.role, Role::ToggleButton);
        assert_eq!(node.checked, Some(true));
    }
    let before_selection = app.tab.doc.clone();
    choose(&mut app, &mut renderer, "tlc-lane", "Lane 2 · 1 spot");
    assert_eq!(key(&app).lane, 1);
    assert_eq!(app.tab.doc, before_selection);
    let k = key(&app);
    let _ = app.update(Message::Tlc(Action::AddSpot(k)));
    assert_eq!(key(&app).spot, Some(1));
    let before_selection = app.tab.doc.clone();
    choose(&mut app, &mut renderer, "tlc-spot", "Spot 1 · Rf 0.15");
    assert_eq!(key(&app).spot, Some(0));
    assert_eq!(app.tab.doc, before_selection);
    for (id, name, checked) in [
        ("tlc-filled", "Filled", true),
        ("tlc-show-rf", "Show Rf", false),
    ] {
        let tree = snapshot(&app, &mut renderer);
        let node = tree.nodes.iter().find(|node| node.id == id).expect(id);
        assert_eq!(node.name, name);
        assert_eq!(node.role, Role::ToggleButton);
        assert_eq!(node.checked, Some(checked));
        let mut activate = Activate::<Message>::new(id);
        operate(&app, &mut renderer, &mut activate);
        let operation::Outcome::Some(message) = activate.finish() else {
            panic!("native toggle {id}")
        };
        let before = app.tab.doc.clone();
        let _ = app.update(message);
        let tree = snapshot(&app, &mut renderer);
        assert_eq!(
            tree.nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .checked,
            Some(!checked)
        );
        assert_eq!(
            app.tab.doc.graphics[0].tlc.as_deref().unwrap().lanes[0],
            before.graphics[0].tlc.as_deref().unwrap().lanes[0]
        );
        one_undo(&mut app, &before);
    }
    // Selection wrappers participate in the ordinary focus operation.
    let mut ui = UserInterface::build(
        app.tlc_panel(),
        Size::new(340., 1200.),
        Cache::new(),
        &mut renderer,
    );
    let mut focus: Box<dyn Operation> = Box::new(FocusControl::new("tlc-lane"));
    loop {
        ui.operate(&renderer, focus.as_mut());
        match focus.finish() {
            operation::Outcome::Chain(next) => focus = next,
            _ => break,
        }
    }
    let mut collect = Collect::new(Rectangle::with_size(Size::new(340., 1200.)));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    assert!(
        collect
            .snapshot()
            .nodes
            .iter()
            .any(|node| node.id == "tlc-lane" && node.focused)
    );
}
