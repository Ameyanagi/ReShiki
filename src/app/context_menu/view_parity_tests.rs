//! Opt-in before/after parity of the context menu entries.
//!
//! Capture the unchanged entries once, then compare a candidate, as
//! `app::workspace::view_parity_tests` does for the views. For each scenario
//! the test dumps every page, in source order, to `context-entries.txt`:
//! `Item|{label}|{action:?}|{enabled}|{shortcut:?}`, `Separator` and
//! `Hint|{text}`. Lines are never sorted or deduplicated.
//!
//! Environment:
//! - `RESHIKI_VIEW_PARITY` (required): the artifact directory.
//! - `RESHIKI_VIEW_PARITY_CAPTURE=1`: write the baseline; otherwise compare.
//! - `RESHIKI_DATA_DIR` (required): recreate it empty before every run.
use super::*;
use reshiki::document::{Arrow, Document, Point as World};
use std::fmt::Write as _;

fn benzene() -> Document {
    reshiki::rings::Preset::Benzene.document(42., false)
}

fn open(app: &mut App, doc: Document) {
    app.tab.doc = doc;
    app.tab.saved = app.tab.doc.clone();
}

fn molecule(app: &mut App) {
    open(app, benzene());
    app.tab.selected = app.tab.doc.all_ids();
}

/// Benzene and a copy of it 240 pt to the right; returns the copy's ids.
fn two_molecules(app: &mut App) -> Vec<u64> {
    let mut doc = benzene();
    let source = doc.clone();
    let copy = reshiki::editing::append(&mut doc, &source, World::new(240., 0.));
    open(app, doc);
    copy
}

/// O, a forward arrow and O; returns the reactant, arrow and product.
fn reaction(app: &mut App) -> (u64, u64, u64) {
    let mut doc = Document::default();
    let reactant = doc.add_atom("O", World::default());
    let product = doc.add_atom("O", World::new(240., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        World::new(90., 0.),
        World::new(150., 0.),
        Default::default(),
        Default::default(),
    ));
    open(app, doc);
    (reactant, arrow, product)
}

/// The reaction recorded explicitly, as `copy_reaction` defines it.
fn explicit_reaction(app: &mut App) -> (u64, u64, u64) {
    let ids = reaction(app);
    let doc = app.tab.doc.clone();
    app.tab.doc.reactions = vec![
        reshiki::reactions::copy_reaction(&doc, &doc)
            .unwrap()
            .unwrap(),
    ];
    ids
}

type Setup = fn(&mut App);

const SCENARIOS: [(&str, Setup); 16] = [
    ("empty", |_| {}),
    ("benzene", |app| open(app, benzene())),
    ("benzene-selected", molecule),
    ("atom", |app| {
        open(app, benzene());
        app.tab.selected = vec![app.tab.doc.atoms[0].id];
    }),
    ("two-molecules", |app| {
        let copy = two_molecules(app);
        app.tab.selected = vec![app.tab.doc.atoms[0].id, copy[0]];
    }),
    ("arrow", |app| {
        let mut doc = benzene();
        let arrow = doc.next_id();
        doc.arrows.push(Arrow::new(
            arrow,
            World::new(70., 0.),
            World::new(160., 0.),
            Default::default(),
            Default::default(),
        ));
        open(app, doc);
        app.tab.selected = vec![arrow];
    }),
    ("ring-fill", |app| {
        use reshiki::palette::{Color as Paint, Hue, Row};
        let mut doc = benzene();
        let ids = doc.all_ids();
        let fill = Some(Paint::Palette(Hue::Blue, Row::Strong));
        assert_eq!(reshiki::ring_fills::apply(&mut doc, &ids, fill), 1);
        open(app, doc);
        app.tab.selected = ids;
    }),
    ("grouped", |app| {
        two_molecules(app);
        app.tab.selected = app.tab.doc.all_ids();
        let _ = app.update(Message::Group);
        assert!(!app.tab.doc.groups.is_empty());
    }),
    ("attachment", |app| {
        molecule(app);
        let _ = app.update(Message::InspectorAction(inspector::Action::Attachment(
            reshiki::attachments::Kind::MultiCenter,
        )));
        let point = app
            .tab
            .selected
            .first()
            .and_then(|id| app.tab.doc.atom(*id));
        assert!(point.is_some_and(|atom| atom.attachment.is_some()));
    }),
    ("clipboard-busy", |app| {
        molecule(app);
        app.tab.clipboard_busy = true;
    }),
    ("copy-as-busy", |app| {
        molecule(app);
        app.copy_as_busy = true;
    }),
    ("undo-redo", |app| {
        molecule(app);
        for _ in 0..2 {
            let _ = app.update(Message::Transform(Transform::Rotate(30.)));
        }
        let _ = app.update(Message::Undo);
        assert!(app.tab.history.can_undo() && app.tab.history.can_redo());
        app.tab.selected.clear();
    }),
    ("tilt", |app| {
        two_molecules(app);
        app.tab.selected = app.tab.doc.atoms[..6].iter().map(|a| a.id).collect();
        assert!(crate::canvas::tilt::available(
            &app.tab.doc,
            &app.tab.selected
        ));
    }),
    ("reaction-inferred", |app| {
        reaction(app);
        app.tab.selected = app.tab.doc.all_ids();
    }),
    ("reaction-explicit-partial", |app| {
        let (reactant, arrow, _) = explicit_reaction(app);
        app.tab.selected = vec![reactant, arrow];
    }),
    ("reaction-participants", |app| {
        let (reactant, _, product) = explicit_reaction(app);
        app.tab.selected = vec![reactant, product];
    }),
];

/// Every page of the menu for the app's current selection.
fn dump(app: &App, out: &mut String) {
    let mut pages = vec![
        Page::Main,
        Page::Align,
        Page::Bonds,
        Page::Tilt,
        Page::Attachments,
        Page::CopyAs,
        Page::AlignObjects,
        Page::Distribute,
        Page::Order,
        Page::Arrange,
    ];
    pages.extend((0..=app.context_commands().len()).map(Page::More));
    for page in pages {
        let _ = writeln!(out, "PAGE|{page:?}");
        for entry in app.context_entries(page) {
            let _ = match entry {
                Entry::Item {
                    label,
                    action,
                    enabled,
                } => writeln!(
                    out,
                    "Item|{label}|{action:?}|{enabled}|{:?}",
                    shortcut(&action)
                ),
                Entry::Separator => writeln!(out, "Separator"),
                Entry::Hint(text) => writeln!(out, "Hint|{text}"),
            };
        }
    }
}

#[test]
#[ignore = "Baseline/candidate context entries; requires RESHIKI_VIEW_PARITY and RESHIKI_DATA_DIR"]
fn entries_match_captured_baseline() {
    let directory = std::path::PathBuf::from(
        std::env::var("RESHIKI_VIEW_PARITY")
            .expect("Set RESHIKI_VIEW_PARITY to the baseline artifact directory"),
    );
    let capture = std::env::var("RESHIKI_VIEW_PARITY_CAPTURE").as_deref() == Ok("1");
    std::env::var_os("RESHIKI_DATA_DIR")
        .expect("Set RESHIKI_DATA_DIR to a directory recreated empty before every run");
    let mut actual = String::new();
    for (name, setup) in SCENARIOS {
        let (mut app, _) = App::new();
        setup(&mut app);
        let _ = writeln!(actual, "SCENARIO|{name}");
        dump(&app, &mut actual);
    }
    let path = directory.join("context-entries.txt");
    if capture {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&path, &actual).unwrap();
        println!("ENTRIES,captured");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("Read baseline {}: {error}", path.display()));
    let (mut expected_lines, mut actual_lines) = (expected.lines(), actual.lines());
    for line in 1.. {
        match (expected_lines.next(), actual_lines.next()) {
            (None, None) => break,
            (Some(expected), Some(actual)) if expected == actual => {}
            (expected, actual) => {
                panic!(
                    "MISMATCH,context-entries.txt,line {line}\n  expected: {expected:?}\n  actual:   {actual:?}"
                );
            }
        }
    }
    assert_eq!(expected, actual, "context-entries.txt differs");
    println!("ENTRIES,matched");
}
