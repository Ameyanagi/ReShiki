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
    assert_eq!(found("flask-2")[0], by_name("flask-2"));
    assert_eq!(found("Flask 2")[0], by_name("flask-2"));
    assert!(found("no such icon at all").is_empty());
}

#[test]
fn the_grid_lists_recent_then_conditions_then_categories() {
    let mut state = State::default();
    let headings = |state: &State| -> Vec<String> {
        (state.rows().into_iter())
            .filter_map(|r| match r {
                Row::Heading(h) => Some(h),
                Row::Tiles(..) => None,
            })
            .collect()
    };
    assert_eq!(headings(&state)[0], CONDITIONS);
    state.recent = vec![by_name("flask")];
    assert_eq!(headings(&state)[..2], ["Recent", CONDITIONS]);
    let tiles: usize = (state.rows().iter())
        .map(|r| match r {
            Row::Tiles(c, _) => c.len(),
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
    app.tab.camera.center = Point::new(500., -300.);
    let _ = app.update(Message::Icons(Action::Pick(by_name("flask"))));
    assert_eq!(app.tab.doc.graphics.len(), 2);
    assert_eq!(app.tab.doc.graphics[1].origin, Point::new(500., -300.));
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

#[test]
fn the_grid_survives_scrolling_searching_and_a_new_recent_row() {
    use iced::advanced::widget::Tree;
    let mut app = app();
    let _ = app.update(Message::Tool(Tool::Icons));
    let mut tree = Tree::new(&app.icons_panel());
    let scroll = |offset| Action::Scrolled {
        offset,
        height: 700.,
    };
    let steps = [
        scroll(3000.),
        scroll(12000.),
        scroll(400.),
        scroll(60000.),
        scroll(0.),
        Action::Search("flask".into()),
        Action::Pick(by_name("flask")),
        Action::Search(String::new()),
        Action::Pick(Choice::Condition(SymbolKind::Light)),
        scroll(5000.),
    ];
    for action in steps {
        let _ = app.update(Message::Icons(action));
        reconcile_grid(&app, &mut tree);
        // A placement adds a Recent row above everything else.
        let _ = app.update(Message::Canvas(Edit::Icon(Point::default())));
        reconcile_grid(&app, &mut tree);
    }
}

/// Diffing may finish before a stale paragraph state panics during layout.
/// Compare tags as well, so this remains a portable regression without a GPU.
fn reconcile_grid(app: &App, tree: &mut iced::advanced::widget::Tree) {
    use iced::advanced::widget::Tree;
    fn same_shape(actual: &Tree, fresh: &Tree) {
        assert_eq!(actual.tag, fresh.tag, "a row keeps only compatible state");
        assert_eq!(actual.children.len(), fresh.children.len());
        for (actual, fresh) in actual.children.iter().zip(&fresh.children) {
            same_shape(actual, fresh);
        }
    }
    let panel = app.icons_panel();
    tree.diff(&panel);
    same_shape(tree, &Tree::new(&panel));
}

#[test]
fn a_keyed_row_replaces_its_state_when_it_becomes_a_heading() {
    use iced::advanced::widget::Tree;
    let tiles: Element<'_, Message> =
        iced::widget::keyed_column([(3, row![text("Light")].into())]).into();
    let mut tree = Tree::new(&tiles);
    let heading: Element<'_, Message> =
        iced::widget::keyed_column([(3, text("Reaction conditions").into())]).into();
    tree.diff(&heading);
    let fresh = Tree::new(&heading);
    assert_eq!(tree.children[0].tag, fresh.children[0].tag);
    assert!(matches!(
        tree.children[0].state,
        iced::advanced::widget::tree::State::Some(_)
    ));
}

#[test]
fn unfiltered_placement_and_search_to_recent_reconcile_each_row() {
    use iced::advanced::widget::Tree;
    let mut app = app();
    let _ = app.update(Message::Tool(Tool::Icons));
    let mut tree = Tree::new(&app.icons_panel());
    for action in [
        Action::Pick(Choice::Condition(SymbolKind::Light)),
        Action::Search("flask".into()),
        Action::Pick(by_name("flask")),
        Action::Search(String::new()),
        Action::Search("フラスコ".into()),
        Action::Search("no such icon".into()),
        Action::Search(String::new()),
        Action::Filter(Filter::Conditions),
        Action::Filter(Filter::All),
    ] {
        let _ = app.update(Message::Icons(action));
        reconcile_grid(&app, &mut tree);
        let _ = app.update(Message::Canvas(Edit::Icon(Point::default())));
        reconcile_grid(&app, &mut tree);
    }
    assert!(
        app.icons
            .recent
            .contains(&Choice::Condition(SymbolKind::Light))
    );
    assert!(app.icons.recent.contains(&by_name("flask")));
}

#[tokio::test]
#[ignore = "Opt-in headless catalog layout check"]
async fn the_grid_lays_out_after_placement_search_and_scroll_jumps() {
    use iced::advanced::{layout, renderer::Headless, widget::Tree};
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let mut app = app();
    let _ = app.update(Message::Tool(Tool::Icons));
    let mut tree = Tree::new(&app.icons_panel());
    let limits = layout::Limits::new(iced::Size::ZERO, iced::Size::new(340., 900.));
    let mut layout = |app: &App| {
        let mut panel = app.icons_panel();
        tree.diff(&panel);
        let node = panel.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert!(node.size().width.is_finite());
        assert!(node.size().height.is_finite());
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(
            iced::Size::new(340., 900.),
        ));
        panel.as_widget_mut().operate(
            &mut tree,
            iced::advanced::Layout::new(&node),
            &renderer,
            &mut iced::advanced::widget::operation::black_box(&mut collect),
        );
        assert!(
            collect.snapshot().duplicate_ids.is_empty(),
            "{:?}",
            collect.snapshot().duplicate_ids
        );
    };
    layout(&app);
    for action in [
        Action::Pick(Choice::Condition(SymbolKind::Light)),
        Action::Search("flask".into()),
        Action::Pick(by_name("flask")),
        Action::Search(String::new()),
        Action::Search("search".into()),
        Action::Search(String::new()),
    ] {
        let _ = app.update(Message::Icons(action));
        layout(&app);
        let _ = app.update(Message::Canvas(Edit::Icon(Point::default())));
        layout(&app);
    }
    // Both wheel updates and scrollbar drags report this same viewport action.
    for offset in [46., 92., 460., 3000., 12000., 30000., 0., 20000., 0.] {
        let _ = app.update(Message::Icons(Action::Scrolled {
            offset,
            height: 700.,
        }));
        layout(&app);
    }
}
