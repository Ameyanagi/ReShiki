use super::*;
use crate::canvas::{Edit as CanvasEdit, Tool};
use iced::widget::text_editor::{Action as Input, Edit, Motion};

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app
}
fn label(app: &mut App, text: &str) -> u64 {
    let id = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id,
        position: Point::new(20., 30.),
        text: text.into(),
        format: Default::default(),
    });
    id
}
fn type_text(app: &mut App, value: &str) {
    let _ = app.update(Message::CaptionAction(Input::Edit(Edit::Paste(
        value.to_owned().into(),
    ))));
}
fn begin(app: &mut App, id: Option<u64>) {
    let _ = app.update(Message::InlineText(Action::Begin(id, Point::new(60., 70.))));
}

#[test]
fn unicode_subscript_caption_commits_ascii_and_reopens_with_explicit_formatting() {
    let mut app = app();
    begin(&mut app, None);
    type_text(&mut app, "H₂O");
    assert_eq!(
        app.tab.caption, "H₂O",
        "the draft stays literal until Apply"
    );
    let before = app.tab.doc.clone();
    assert!(app.finish_inline(true));
    let caption = &app.tab.doc.annotations[0];
    assert_eq!(caption.text, "H2O");
    assert_eq!(caption.format.spans.len(), 1);
    assert_eq!(
        (caption.format.spans[0].start, caption.format.spans[0].end),
        (1, 2)
    );
    assert_eq!(caption.format.at(1).script, Script::Subscript);
    let id = caption.id;
    let committed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, committed);
    begin(&mut app, Some(id));
    assert_eq!(app.tab.caption, "H2O");
    assert_eq!(app.tab.caption_format.at(1).script, Script::Subscript);
    assert!(app.finish_inline(true));
    assert_eq!(app.tab.doc, committed);
}

#[test]
fn pasted_captions_normalize_subscripts_before_native_document_insertion() {
    let mut source = Document::default();
    source.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "K₂CO₃, DMF".into(),
        format: Default::default(),
    });
    let oxygen = source.add_atom("O", Point::new(100., 0.));
    reshiki::molecule_names::show(
        &mut source,
        &[oxygen],
        "H₂O".into(),
        "O".into(),
        TextFormat::default(),
    )
    .unwrap();
    let mut app = app();
    let text = format!(
        "{}{}",
        reshiki::editing::CLIPBOARD_PREFIX,
        serde_json::to_string(&source.current()).unwrap()
    );
    let _ = app.update(Message::Pasted(Some(text)));
    let caption = &app.tab.doc.annotations[0];
    assert_eq!(caption.text, "K2CO3, DMF");
    for offset in [1, 4] {
        assert_eq!(caption.format.at(offset).script, Script::Subscript);
    }
    let name = &app.tab.doc.molecule_names[0];
    assert_eq!(name.text, "H2O");
    let named_caption = app
        .tab
        .doc
        .annotations
        .iter()
        .find(|caption| caption.id == name.annotation)
        .unwrap();
    assert_eq!(named_caption.text, name.text);
    assert_eq!(named_caption.format.at(1).script, Script::Subscript);
    assert!(named_caption.format.name_breaks);
    app.tab.doc.validate().unwrap();
}

#[test]
fn new_formula_captions_format_automatically_and_manual_controls_win() {
    let mut app = app();
    begin(&mut app, None);
    type_text(&mut app, "C2H2");
    assert!(app.tab.caption_format.style.formula);
    let preview = reshiki::typography::layout(&app.tab.caption, &app.tab.caption_format);
    assert!(
        preview
            .fragments
            .iter()
            .any(|f| f.text == "2" && f.style.script == reshiki::typography::Script::Subscript)
    );
    assert!(app.finish_inline(true));
    let original = app.tab.doc.clone();
    assert!(
        app.tab.doc.atoms.is_empty(),
        "Caption formatting must not create molecular atoms"
    );
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.annotations.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, original);
    begin(&mut app, None);
    type_text(&mut app, "Figure 2");
    assert!(!app.tab.caption_format.style.formula);
    let _ = app.update(Message::InlineText(Action::Finish(false)));
    begin(&mut app, None);
    type_text(&mut app, "H2O");
    app.apply_text_style(StyleChange::Formula(false));
    type_text(&mut app, "2");
    assert!(!app.tab.caption_format.style.formula);
    let _ = app.update(Message::InlineText(Action::Finish(false)));
    assert_eq!(app.tab.doc, original);
}

#[test]
fn text_tool_typing_and_formatting_commit_as_one_undo_step() {
    let mut app = app();
    app.inspector_open = false;
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::Text));
    let _ = app.update(Message::Canvas(CanvasEdit::Click(Point::new(60., 70.))));
    assert!(app.tab.inline_text.is_some());
    assert!(!app.inspector_open);
    type_text(&mut app, "加熱 H2O");
    let _ = app.update(Message::TextStyle(StyleChange::Formula(true)));
    let _ = app.update(Message::TextStyle(StyleChange::Color(
        reshiki::palette::Color::Custom([30, 90, 70]),
    )));
    assert_eq!(app.tab.doc, before);
    assert!(app.dirty());
    let _ = app.update(Message::InlineText(Action::Finish(true)));
    let finished = app.tab.doc.clone();
    assert_eq!(finished.annotations[0].text, "加熱 H2O");
    assert_eq!(finished.annotations[0].position, Point::new(60., 70.));
    assert_eq!(
        finished.annotations[0].format.style.color,
        reshiki::palette::Color::Custom([30, 90, 70])
    );
    assert!(finished.annotations[0].format.style.formula);
    assert_eq!(app.tab.selected, vec![finished.annotations[0].id]);
    assert!(!app.inspector_open);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, finished);
}

#[test]
fn draft_undo_and_cancel_preserve_independent_canvas_changes() {
    let mut app = app();
    let id = label(&mut app, "酸触媒");
    begin(&mut app, Some(id));
    type_text(&mut app, "、加熱");
    let _ = app.update(Message::TextStyle(StyleChange::Bold(true)));
    let _ = app.update(Message::Undo);
    assert!(!app.tab.caption_format.style.bold);
    assert_eq!(app.tab.caption, "酸触媒、加熱");
    let _ = app.update(Message::Redo);
    assert!(app.tab.caption_format.style.bold);
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::new(200., 20.));
    app.changed(before.clone());
    let concurrent = app.tab.doc.clone();
    app.sync_typography();
    assert_eq!(app.tab.caption, "酸触媒、加熱");
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, concurrent);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn text_commit_preserves_other_edits_and_rejects_conflicting_targets() {
    let mut app = app();
    let id = label(&mut app, "A");
    begin(&mut app, Some(id));
    type_text(&mut app, "B");
    app.tab.doc.add_atom("C", Point::default());
    let concurrent = app.tab.doc.clone();
    assert!(app.finish_inline(true));
    assert_eq!(app.tab.doc.annotations[0].text, "AB");
    assert_eq!(app.tab.doc.atoms, concurrent.atoms);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, concurrent);
    begin(&mut app, Some(id));
    type_text(&mut app, " draft");
    app.tab.doc.annotations[0].text = "changed externally".into();
    let external = app.tab.doc.clone();
    assert!(!app.finish_inline(true));
    assert_eq!(app.tab.doc, external);
    assert!(app.tab.inline_text.is_some());
    assert!(app.finish_inline(false));
    assert_eq!(app.tab.doc, external);
    begin(&mut app, Some(id));
    type_text(&mut app, " epoch");
    app.tab.file_epoch += 1;
    assert!(!app.finish_inline(true));
    assert_eq!(app.tab.doc, external);
}

#[test]
fn untouched_labels_keep_order_and_do_not_add_history() {
    let mut app = app();
    let id = label(&mut app, "First");
    label(&mut app, "Second");
    let before = app.tab.doc.clone();
    begin(&mut app, Some(id));
    assert!(app.finish_inline(true));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.undo(&mut app.tab.doc));
}

#[test]
fn deleting_a_label_prunes_groups_and_undo_restores_them() {
    let mut app = app();
    let id = label(&mut app, "Caption");
    let atom = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![id, atom];
    let _ = app.update(Message::Group);
    let before = app.tab.doc.clone();
    assert!(!before.groups.is_empty());
    begin(&mut app, Some(id));
    let _ = app.update(Message::CaptionAction(Input::SelectAll));
    type_text(&mut app, "");
    assert!(app.finish_inline(true));
    assert!(app.tab.doc.annotations.is_empty());
    assert_eq!(app.tab.doc.atoms, before.atoms);
    app.tab.doc.validate().unwrap();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn unicode_ranges_keep_formatting_during_edits() {
    let mut app = app();
    let id = label(&mut app, "酸触媒 H2O");
    let range = "酸触媒 ".len().."酸触媒 H2O".len();
    app.tab.doc.annotations[0]
        .format
        .apply("酸触媒 H2O", Some(range), &StyleChange::Bold(true));
    begin(&mut app, Some(id));
    let _ = app.update(Message::CaptionAction(Input::Move(Motion::DocumentStart)));
    type_text(&mut app, "濃 ");
    assert!(app.finish_inline(true));
    let label = &app.tab.doc.annotations[0];
    assert_eq!(label.text, "濃 酸触媒 H2O");
    assert!(label.format.at("濃 酸触媒 ".len()).bold);
    assert!(!label.format.at(0).bold);
    label.format.validate(&label.text).unwrap();
}

#[test]
fn shared_editor_bookkeeping_tracks_repeated_utf8_glyph_deletion_and_paste() {
    let old = "触触触\nH2O";
    let mut editor = text_editor::Content::with_text(old);
    editor.perform(Input::Move(Motion::DocumentStart));
    editor.perform(Input::Move(Motion::Right));
    let (updated, replaced) =
        perform_editor_action(&mut editor, old, Input::Edit(Edit::Delete)).unwrap();
    assert_eq!(updated, "触触\nH2O");
    assert_eq!(replaced, "触".len().."触触".len());
    editor.perform(Input::Move(Motion::DocumentEnd));
    editor.perform(Input::Select(Motion::Left));
    assert_eq!(
        editor_range(&editor, &updated),
        Some(updated.len() - 1..updated.len())
    );
    let (pasted, replaced) = perform_editor_action(
        &mut editor,
        &updated,
        Input::Edit(Edit::Paste("温度".to_owned().into())),
    )
    .unwrap();
    assert_eq!(pasted, "触触\nH2温度");
    assert_eq!(replaced, updated.len() - 1..updated.len());
}

#[test]
fn recovery_includes_drafts_and_cancel_removes_them() {
    let mut app = app();
    let dir = tempfile::tempdir().unwrap();
    app.tab.recovery = Some(reshiki::recovery::Recovery::in_directory(dir.path()).unwrap());
    begin(&mut app, None);
    type_text(&mut app, "Unsaved label");
    let _ = app.update(Message::Tick);
    super::super::autosave::tests::finish_pending(&mut app);
    let path = app.tab.recovery.as_ref().unwrap().session.clone();
    let snapshot: reshiki::recovery::Snapshot =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(snapshot.document.annotations[0].text, "Unsaved label");
    assert!(app.tab.doc.annotations.is_empty());
    type_text(&mut app, " updated");
    let _ = app.update(Message::Tick);
    super::super::autosave::tests::finish_pending(&mut app);
    let snapshot: reshiki::recovery::Snapshot =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        snapshot.document.annotations[0].text,
        "Unsaved label updated"
    );
    let _ = app.update(Message::Escape);
    let _ = app.update(Message::Tick);
    super::super::autosave::tests::finish_pending(&mut app);
    assert!(!path.exists());
}

#[test]
fn file_and_tool_commands_finish_drafts_before_continuing() {
    let mut app = app();
    begin(&mut app, None);
    type_text(&mut app, "Keep me");
    let _ = app.update(Message::New);
    assert!(app.pending.is_none(), "New opens a tab without asking");
    assert!(app.tab.doc.annotations.is_empty());
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Cycle(false)));
    assert!(app.tab.inline_text.is_none());
    assert_eq!(app.tab.doc.annotations[0].text, "Keep me");
    begin(&mut app, Some(1));
    type_text(&mut app, " too");
    let _ = app.update(Message::Tool(Tool::Bond(1)));
    assert!(app.tab.inline_text.is_none());
    assert_eq!(app.tool, Tool::Bond(1));
    assert_eq!(app.tab.doc.annotations[0].text, "Keep me too");
}

#[test]
fn highlighter_uses_utf8_offsets_across_lines() {
    use iced::advanced::text::Highlighter;
    let text = "触媒\nH2O";
    let mut format = TextFormat::default();
    format.apply(
        text,
        Some("触媒\n".len()..text.len()),
        &StyleChange::Bold(true),
    );
    let mut highlighter = CaptionHighlighter::new(&(text.into(), format));
    let first: Vec<_> = highlighter.highlight_line("触媒").collect();
    assert_eq!(first[0].0, 0..3);
    assert_eq!(first[1].0, 3..6);
    assert!(first.iter().all(|(_, style)| !style.bold));
    let second: Vec<_> = highlighter.highlight_line("H2O").collect();
    assert!(second.iter().all(|(_, style)| style.bold));
    highlighter.change_line(0);
    assert!(
        highlighter
            .highlight_line("触媒")
            .all(|(_, style)| !style.bold)
    );
}

#[test]
fn finished_labels_are_revealed_without_changing_zoom() {
    let mut app = app();
    app.tab.camera.zoom = 1.;
    app.tab.camera.center = Point::default();
    app.viewport = iced::Size::new(420., 360.);
    let _ = app.inline_action(Action::Begin(None, Point::new(190., 160.)));
    type_text(&mut app, "Conditions\nTime");
    assert!(app.finish_inline(true));
    let label = &app.tab.doc.annotations[0];
    let (width, height) = label.size();
    let position = app
        .tab
        .camera
        .screen(label.position, iced::Rectangle::with_size(app.viewport));
    assert!(position.x >= 20. && position.x + width <= app.viewport.width - 19.);
    assert!(position.y >= 20. && position.y + height <= app.viewport.height - 19.);
    assert_eq!(app.tab.camera.zoom, 1.);
}

#[test]
fn full_range_styles_are_reflected_in_the_toolbar_after_commit() {
    let mut app = app();
    let id = label(&mut app, "酸触媒");
    begin(&mut app, Some(id));
    let _ = app.update(Message::CaptionAction(Input::SelectAll));
    app.apply_text_style(StyleChange::Bold(true));
    app.apply_selection_color(reshiki::palette::Color::Custom([20, 70, 130]));
    assert!(app.finish_inline(true));
    assert!(app.current_text_style().bold);
    assert_eq!(
        app.current_selection_color(),
        Some(reshiki::palette::Color::Custom([20, 70, 130]))
    );
}

#[test]
fn empty_editor_is_wide_enough_for_the_zoomed_placeholder() {
    use iced::advanced::{graphics::text::Paragraph, text::Paragraph as _};
    let format = TextFormat::default();
    let natural = reshiki::typography::layout(PLACEHOLDER, &format);
    for zoom in [1.5, 3.] {
        let size = (format.style.size() * zoom).clamp(12., 56.);
        let bounds = editor_bounds(
            iced::Size::new(1600., 1000.),
            iced::Rectangle::new(iced::Point::new(60., 70.), iced::Size::ZERO),
            natural.width * zoom + 36.,
            natural.height * zoom + 20.,
            40.,
            None,
        );
        let placeholder = Paragraph::with_text(iced::advanced::Text {
            content: PLACEHOLDER,
            bounds: iced::Size::INFINITE,
            size: iced::Pixels(size),
            line_height: format.line_spacing.into(),
            font: iced::Font::with_name(reshiki::style::font_name(&format.style.family)),
            align_x: Default::default(),
            align_y: iced::alignment::Vertical::Top,
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::Word,
        });
        // The editor and popup pad the text by 7 and 2 px on each side.
        assert!(
            bounds.width - 18. >= placeholder.min_width(),
            "{zoom}: {bounds:?} {}",
            placeholder.min_width()
        );
    }
}

#[test]
fn popup_stays_in_view_at_document_edges_with_long_labels() {
    let viewport = iced::Size::new(420., 360.);
    for anchor in [
        iced::Point::new(-100., -100.),
        iced::Point::new(410., 350.),
        iced::Point::new(900., 800.),
    ] {
        for extra in [40., 120.] {
            for arrow in [None, Some(iced::Point::new(210., 180.))] {
                let label = iced::Rectangle::new(anchor, iced::Size::new(80., 35.));
                let bounds = editor_bounds(viewport, label, 4000., 1800., extra, arrow);
                assert!(bounds.x >= 0. && bounds.y >= 0.);
                assert!(bounds.x + bounds.width <= viewport.width);
                assert!(bounds.y + bounds.height <= viewport.height);
                assert!(bounds.height > extra);
            }
        }
    }
}

#[test]
fn condition_popups_stay_outside_the_label_and_arrow_when_space_permits() {
    let viewport = iced::Size::new(1200., 900.);
    let arrow = iced::Point::new(600., 450.);
    let corners = |r: iced::Rectangle| {
        [
            r.position(),
            iced::Point::new(r.x + r.width, r.y),
            iced::Point::new(r.x, r.y + r.height),
            iced::Point::new(r.x + r.width, r.y + r.height),
        ]
    };
    // Both sides of horizontal, vertical and diagonal arrows at the repro zoom.
    for (x, y) in [(0., 1.), (1., 0.), (0.6, 0.8), (0.8, -0.6)] {
        for side in [-1., 1.] {
            let normal = iced::Vector::new(x * side, y * side);
            let center = arrow + normal * 100.;
            let label = iced::Rectangle::new(
                center - iced::Vector::new(30., 17.5) * 1.09,
                iced::Size::new(60. * 1.09, 35. * 1.09),
            );
            for extra in [40., 124.] {
                let bounds = editor_bounds(viewport, label, 100., 60., extra, Some(arrow));
                let projection = |p: iced::Point| {
                    let offset = p - arrow;
                    offset.x * normal.x + offset.y * normal.y
                };
                let label_edge = corners(label)
                    .map(projection)
                    .into_iter()
                    .reduce(f32::max)
                    .unwrap();
                for corner in corners(bounds) {
                    assert!(projection(corner) >= label_edge + 7.99, "{bounds:?}");
                }
            }
            let ordinary = editor_bounds(viewport, label, 100., 60., 40., None);
            assert_eq!(
                ordinary.position(),
                label.position() - iced::Vector::new(8., 8.)
            );
            assert_eq!(ordinary.size(), iced::Size::new(240., 100.));
        }
    }
}

#[test]
fn text_tool_conditions_attach_to_an_arrow_and_sideways_drag_detaches() {
    use reshiki::{
        arrow_labels::Side,
        arrows::{ArrowStyle, Preset},
        document::Arrow,
    };
    let mut app = app();
    app.tab.doc.arrows.push(Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(80., 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::Text));
    // The text tool puts the top-left corner at the click.
    let _ = app.update(Message::Canvas(CanvasEdit::Click(Point::new(10., -60.))));
    type_text(&mut app, "Pd/C, H2, EtOAc, rt");
    let _ = app.update(Message::InlineText(Action::Finish(true)));
    let doc = &app.tab.doc;
    let id = doc.annotations[0].id;
    assert_eq!(doc.arrow_labels.len(), 1);
    assert_eq!(doc.arrow_labels[0].side, Side::Above);
    let width = doc.annotations[0].size().0;
    let arrow = &doc.arrows[0];
    assert!(arrow.start.distance(arrow.end) > width);
    let mid = arrow.start.lerp(arrow.end, 0.5);
    assert!((doc.annotations[0].position.x + width / 2. - mid.x).abs() < 1e-3);
    let attached = app.tab.doc.clone();

    app.viewport = iced::Size::new(1000., 700.);
    app.tab.camera.zoom = 1.09;
    let camera = app.tab.camera;
    begin(&mut app, Some(id));
    type_text(&mut app, ", overnight");
    let _ = app.update(Message::TextStyle(StyleChange::Underline(true)));
    let _ = app.with_inline_text(Space::new().into());
    assert_eq!(app.tab.doc, attached, "editing does not move the drawing");
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, attached);
    begin(&mut app, Some(id));
    type_text(&mut app, ", overnight");
    assert!(app.finish_inline(true));
    assert_eq!(app.tab.doc.arrow_labels, attached.arrow_labels);
    assert!(
        app.tab.doc.arrows[0]
            .start
            .distance(app.tab.doc.arrows[0].end)
            > attached.arrows[0].start.distance(attached.arrows[0].end)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, attached);

    let _ = app.update(Message::Canvas(CanvasEdit::Move(vec![id], 1000., 0.)));
    assert!(app.tab.doc.arrow_labels.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, attached);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}
