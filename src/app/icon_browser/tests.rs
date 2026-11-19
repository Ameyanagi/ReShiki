use super::*;
use crate::canvas::Edit;

fn by_name(name: &str) -> Choice {
    Choice::Library(
        (library().entries.iter())
            .position(|e| e.name == name)
            .unwrap(),
    )
}

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app
}

#[test]
fn every_bundled_icon_reads_and_stays_on_its_grid() {
    let lib = library();
    assert_eq!(lib.entries.len(), 5203);
    assert!(lib.categories.iter().any(|c| c == "Health"));
    for e in &lib.entries {
        let icon = Icon::parse(&e.name, &e.paths).unwrap();
        icon.validate().unwrap();
    }
}

#[test]
fn search_ranks_names_first_and_reads_japanese_and_conditions() {
    let found = |query: &str| ranked(query, State::default().pool());
    assert_eq!(found("flask")[0], by_name("flask"));
    assert!(found("フラスコ").contains(&by_name("flask")));
    assert_eq!(found("heat")[0], Choice::Condition(SymbolKind::Heat));
    assert_eq!(found("加熱")[0], Choice::Condition(SymbolKind::Heat));
    assert!(found("chemistry flask").contains(&by_name("flask")));
    assert!(found("no such icon at all").is_empty());
}

#[test]
fn the_grid_lists_recent_then_conditions_then_categories() {
    let mut state = State::default();
    let headings = |state: &State| -> Vec<String> {
        (state.rows().into_iter())
            .filter_map(|r| match r {
                Row::Heading(h) => Some(h),
                Row::Tiles(_) => None,
            })
            .collect()
    };
    assert_eq!(headings(&state)[0], CONDITIONS);
    state.recent = vec![by_name("flask")];
    assert_eq!(headings(&state)[..2], ["Recent", CONDITIONS]);
    let tiles: usize = (state.rows().iter())
        .map(|r| match r {
            Row::Tiles(c) => c.len(),
            Row::Heading(_) => 0,
        })
        .sum();
    assert_eq!(
        tiles,
        1 + SymbolKind::CONDITIONS.len() + library().entries.len()
    );
    state.query = "flask".into();
    assert!(headings(&state).is_empty(), "a search lists matches only");
}

#[test]
fn a_click_places_one_icon_a_bond_long_in_one_undo_step() {
    let mut app = app();
    let _ = app.update(Message::Tool(Tool::Icons));
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Icons);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Icons(Action::Pick(by_name("flask"))));
    assert_eq!(app.tool, Tool::Icons);
    assert_eq!(app.tab.doc, before, "arming changes nothing");
    let at = Point::new(120., 80.);
    let _ = app.update(Message::Canvas(Edit::Icon(at)));
    assert!(!app.error, "{}", app.status);
    let [icon] = app.tab.doc.graphics.as_slice() else {
        panic!("one graphic");
    };
    assert_eq!(icon.kind, GraphicKind::Icon);
    assert_eq!(icon.icon.as_ref().unwrap().name, "flask");
    assert_eq!(icon.origin, at);
    let bond = app.tab.doc.drawing_style.bond_length_world;
    assert_eq!(icon.axis_x, Point::new(bond, 0.));
    assert_eq!(icon.style.width_pt, app.tab.doc.drawing_style.line_width_pt);
    assert_eq!(app.tab.selected, [icon.id]);
    assert_eq!(app.icons.recent, [by_name("flask")]);
    let placed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
    // The armed icon again: a copy in the middle of the view.
    let _ = app.update(Message::Icons(Action::Pick(by_name("flask"))));
    assert_eq!(app.tab.doc.graphics.len(), 2);
    assert_eq!(app.tab.doc.graphics[1].origin, app.tab.camera.center);
}

#[test]
fn a_condition_icon_keeps_its_drawing_and_paper_disc() {
    let mut app = app();
    let _ = app.update(Message::Icons(Action::Arm(Choice::Condition(
        SymbolKind::Light,
    ))));
    assert_eq!(app.inspector_tab, InspectorTab::Icons);
    let _ = app.update(Message::Canvas(Edit::Icon(Point::default())));
    let [g] = app.tab.doc.graphics.as_slice() else {
        panic!("one graphic");
    };
    assert_eq!(g.kind, GraphicKind::Symbol(SymbolKind::Light));
    assert_eq!(g.style.fill, Some(reshiki::palette::Color::Paper));
    assert!(g.layer > 0);
}

#[test]
fn escape_closes_the_browser_and_returns_to_select() {
    let mut app = app();
    let _ = app.update(Message::Icons(Action::Arm(by_name("flask"))));
    let _ = app.update(Message::Icons(Action::Search("fl".into())));
    let _ = app.update(Message::Escape);
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Icon(Point::default())));
    assert_eq!(app.tab.doc, before, "a closed browser places nothing");
}
