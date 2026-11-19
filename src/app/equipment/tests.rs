use super::*;
use crate::canvas::Edit;
use reshiki::equipment::builtin;
use reshiki::graphics::GraphicKind;

/// The key that arms a built-in piece.
fn k(id: &str) -> String {
    placed::key(&builtin::find(id).unwrap())
}

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
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-15"))));
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
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/liebig-15"))));
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
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::default())));
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tool, Tool::Select);
}

/// A flask placed at `at`, and where a piece's joint seats on its neck.
fn flask_neck(app: &mut App, at: Point) -> Point {
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(at)));
    let neck = solve::ports(&app.tab.doc.graphics[0]).remove(0);
    Point::new(neck.point[0] as f32, neck.point[1] as f32)
}

#[test]
fn a_stopper_placed_by_the_neck_seats_and_undo_takes_it_off_in_one_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let neck = flask_neck(&mut app, Point::new(200., 300.));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let before = app.tab.doc.clone();
    // A click centres the stopper: aim its shoulder a few points off the neck.
    let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
    let probe = placed::graphic(
        0,
        builtin::find("reshiki/stopper-15").unwrap(),
        Point::default(),
        scale,
    );
    let shoulder = solve::ports(&probe)[0].point;
    let click = Point::new(
        neck.x - shoulder[0] as f32 + 3.,
        neck.y - shoulder[1] as f32 - 4.,
    );
    let _ = app.update(Message::Canvas(Edit::Piece(click)));
    assert_eq!(app.tab.doc.seats.len(), 1, "{}", app.status);
    assert_eq!(app.status, "Seated the stopper bottom on the flask neck");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn keyboard_connect_offers_joints_in_turn_and_return_seats_the_shown_one() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    flask_neck(&mut app, Point::new(200., 300.));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-29"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(600., 300.))));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(260., 100.))));
    let stopper = app.tab.doc.graphics[2].id;
    let _ = app.update(Message::Tool(Tool::Select));
    app.set_selection(vec![stopper]);
    let _ = app.update(Message::ContextKey("j".into()));
    assert!(
        app.status.starts_with("Connect to the flask neck (1 of 2)"),
        "{}",
        app.status
    );
    let _ = app.update(Message::Shortcut(crate::app::shortcuts::Action::Nudge(
        0., 1.,
    )));
    assert!(app.status.contains("(2 of 2)"), "{}", app.status);
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(
        app.status.starts_with("Inserted the reducing adapter"),
        "{}",
        app.status
    );
    assert_eq!(app.tab.doc.seats.len(), 2);
    assert!(app.tab.connect.is_none());
    // Detach from Properties pulls the adapter and stopper off the neck.
    let neck = app.tab.doc.seats[0].a.clone();
    let _ = app.update(Message::Equipment(Action::Detach(neck)));
    assert_eq!(app.tab.doc.seats.len(), 1);
}

/// A lab's copy of the 100 mL flask, at `revision`, with an extra keyword so
/// two copies can differ under one ID.
fn lab_flask(revision: u32, keyword: &str) -> Arc<Definition> {
    let mut d = (*builtin::find("reshiki/rbf-100-15").unwrap()).clone();
    d.id = "lab/flask".into();
    d.revision = revision;
    d.keywords.push(keyword.into());
    Arc::new(d)
}

#[test]
fn library_pieces_join_the_browser_once_each_and_name_their_library_on_a_clash() {
    let (mut app, _) = App::new();
    let mine = lab_flask(1, "mine");
    let theirs = lab_flask(1, "theirs");
    let libraries = &mut app.equipment.libraries;
    libraries.mine.pieces = vec![mine.clone(), mine.clone()];
    libraries.labs = vec![(
        PathBuf::from("/labs/Tanaka lab"),
        reshiki::piece_library::Folder {
            pieces: vec![theirs.clone(), mine.clone()],
            problems: vec![],
        },
    )];
    libraries.index();
    let entries = &libraries.entries;
    let lab: Vec<_> = (entries.iter())
        .filter(|e| e.definition.id == "lab/flask")
        .map(|e| e.source)
        .collect();
    assert_eq!(lab, [Source::Mine, Source::Lab(0)]);
    // The lab's own list still shows the piece My pieces also holds.
    let in_lab = (entries.iter())
        .filter(|e| e.definition.id == "lab/flask" && e.in_library(Source::Lab(0)))
        .count();
    assert_eq!(in_lab, 2);
    assert_eq!(libraries.name(Source::Lab(0)), "Tanaka lab");
    assert_eq!(libraries.newer(&lab_flask(0, "old")).unwrap().0, mine);
    assert!(libraries.newer(&mine).is_none());
    // Arming by key places that exact piece.
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(placed::key(&theirs))));
    assert_eq!(app.equipment.armed.as_deref(), Some(&*theirs));
}

#[test]
fn updating_to_a_newer_revision_is_one_undo_step_and_keeps_the_seat() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let old = lab_flask(1, "old");
    let new = lab_flask(2, "new");
    app.equipment.libraries.mine.pieces = vec![old.clone(), new.clone()];
    app.equipment.libraries.index();
    let _ = app.update(Message::Equipment(Action::Arm(placed::key(&old))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(200., 300.))));
    let flask = app.tab.selected[0];
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(600., 120.))));
    let stopper = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let joint = |piece, joint: &str| JointRef {
        piece,
        joint: joint.into(),
    };
    solve::connect(
        &mut app.tab.doc,
        &[stopper],
        &joint(flask, "neck"),
        &joint(stopper, "bottom"),
    )
    .unwrap();
    app.changed(before);
    assert_eq!(app.tab.doc.seats.len(), 1, "{}", app.status);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Equipment(Action::Update {
        ids: vec![flask],
        key: placed::key(&new),
    }));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Updated to revision 2");
    let g = app.tab.doc.graphics.iter().find(|g| g.id == flask).unwrap();
    assert_eq!(g.piece.as_ref().unwrap().definition, placed::key(&new));
    assert_eq!(app.tab.doc.seats.len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}
