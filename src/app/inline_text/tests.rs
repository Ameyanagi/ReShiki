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
fn popup_stays_in_view_at_document_edges_with_long_labels() {
    let viewport = iced::Size::new(420., 360.);
    for anchor in [
        iced::Point::new(-100., -100.),
        iced::Point::new(410., 350.),
        iced::Point::new(900., 800.),
    ] {
        for extra in [40., 120.] {
            let bounds = editor_bounds(viewport, anchor, 4000., 1800., extra);
            assert!(bounds.x >= 0. && bounds.y >= 0.);
            assert!(bounds.x + bounds.width <= viewport.width);
            assert!(bounds.y + bounds.height <= viewport.height);
            assert!(bounds.height > extra);
        }
    }
}
