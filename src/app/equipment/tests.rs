use super::*;
use crate::canvas::Edit;
use reshiki::graphics::GraphicKind;

#[test]
fn search_finds_pieces_by_any_name_keyword_or_size() {
    let found = |query: &str| -> Vec<&str> {
        (builtin::catalogue().iter())
            .filter(|d| matches(d, query))
            .map(|d| d.id.as_str())
            .collect()
    };
    for query in ["丸底", "round", "ナス", "Round-bottom 100"] {
        assert!(found(query).contains(&"reshiki/rbf-100-15"), "{query}");
    }
    assert_eq!(found("liebig"), ["reshiki/liebig-15", "reshiki/liebig-29"]);
    assert_eq!(found("septum 29/42"), ["reshiki/septum-29"]);
    assert!(found("burette").is_empty());
    assert_eq!(found("").len(), builtin::catalogue().len());
}

#[test]
fn names_sizes_and_the_click_hint_come_from_the_definition() {
    let flask = builtin::find("reshiki/rbf-100-15").unwrap();
    assert_eq!(title(&flask), "Round-bottom flask");
    assert_eq!(size(&flask), "100 mL · 15/25");
    assert_eq!(noun(&flask), "flask");
    let adapter = builtin::find("reshiki/red-29-15").unwrap();
    assert_eq!(size(&adapter), "29/42 · 15/25");
    assert_eq!(noun(&adapter), "adapter");
}

#[test]
fn arming_and_placing_a_piece_adds_one_graphic_in_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Tool(Tool::Equipment));
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Equipment);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Equipment(Action::Arm("reshiki/rbf-100-15".into())));
    assert_eq!(app.tool, Tool::Equipment);
    assert_eq!(app.tab.doc, before, "arming changes nothing");
    let at = Point::new(120., 80.);
    let _ = app.update(Message::Canvas(Edit::Piece(at)));
    assert!(!app.error, "{}", app.status);
    let [piece] = app.tab.doc.graphics.as_slice() else {
        panic!("one graphic: {:?}", app.tab.doc.graphics.len());
    };
    assert_eq!(piece.kind, GraphicKind::Piece);
    assert_eq!(app.tab.selected, [piece.id]);
    let (lo, hi) = piece.shape_bounds();
    let mid = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert!(mid.distance(at) < 0.01, "{mid:?}");
    assert_eq!(app.tab.doc.piece_definitions.len(), 1);
    // Keep placing is on: the tool stays armed for the next copy.
    assert_eq!(app.tool, Tool::Equipment);
    let placed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
}

#[test]
fn escape_disarms_and_a_click_then_places_nothing() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm("reshiki/liebig-15".into())));
    assert_eq!(app.tool, Tool::Equipment);
    let _ = app.update(Message::Escape);
    assert_eq!(app.tool, Tool::Select);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Piece(Point::default())));
    assert_eq!(app.tab.doc, before);
}

#[test]
fn without_keep_placing_one_click_returns_to_select() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Repeat(false)));
    let _ = app.update(Message::Equipment(Action::Arm("reshiki/stopper-15".into())));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::default())));
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tool, Tool::Select);
}
