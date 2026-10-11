//! Real event routes must reject unresolved participating components atomically.
use super::*;
use crate::{app::shortcuts, canvas::Edit};
use iced::keyboard::{Key, Modifiers};
use reshiki::{atom_text, generic::JOIN_NOTICE};

#[derive(Debug, PartialEq)]
struct Snapshot {
    doc: Document,
    selected: Vec<u64>,
    revision: u64,
    epoch: u64,
    camera: (Point, f32),
    fit: bool,
    tool: Tool,
    undo: Vec<Document>,
    redo: Vec<Document>,
    inline: bool,
    caption: String,
    hue_edit: bool,
}
fn snapshot(app: &App) -> Snapshot {
    Snapshot {
        doc: app.tab.doc.clone(),
        selected: app.tab.selected.clone(),
        revision: app.tab.revision,
        epoch: app.tab.file_epoch,
        camera: (app.tab.camera.center, app.tab.camera.zoom),
        fit: app.tab.fit_to_view,
        tool: app.tool,
        undo: app.tab.history.undo_frames().cloned().collect(),
        redo: app.tab.history.redo_frames().cloned().collect(),
        inline: app.tab.inline_text.is_some(),
        caption: app.tab.caption.clone(),
        hue_edit: matches!(
            app.style_menu,
            Some(crate::app::color_popover::Menu::Color { edit: Some(_), .. })
        ),
    }
}
fn chain(doc: &mut Document, x: f32, y: f32) -> [u64; 3] {
    let a = doc.add_atom("C", Point::new(x, y));
    let b = doc.add_atom("C", Point::new(x + 42., y));
    let c = doc.add_atom("C", Point::new(x + 84., y));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    [a, b, c]
}
fn generic(doc: &mut Document, site: u64) {
    *doc = atom_text::apply(doc, site, "[C,N]", atom_text::Mode::Auto).unwrap();
    doc.validate().unwrap();
}
fn ready(doc: Document, selected: Vec<u64>) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.saved = app.tab.doc.clone();
    // Both stacks contain real edits, so rejection cannot clear pending Redo.
    for y in [600., 700.] {
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("O", Point::new(800., y));
        app.changed(before);
    }
    let _ = app.update(Message::Undo);
    assert!(app.tab.history.can_undo() && app.tab.history.can_redo());
    app.tab.selected = selected;
    app.tab.camera.center = Point::new(90., -40.);
    app.tab.camera.zoom = 1.6;
    app.tab.fit_to_view = false;
    app
}
fn primary_join() -> Message {
    let key = Key::Character("j".into());
    let message = shortcuts::key_message(&key, &key, Modifiers::COMMAND).unwrap();
    assert!(matches!(
        message,
        Message::Shortcut(shortcuts::Action::Join)
    ));
    message
}
fn rejected(app: &mut App, message: Message) {
    let before = snapshot(app);
    let _ = app.update(message);
    assert!(app.error);
    assert_eq!(app.status, JOIN_NOTICE);
    assert_eq!(snapshot(app), before);
}

#[test]
fn primary_j_and_explicit_merge_reject_typed_generic_components_without_history_change() {
    for merge in [false, true] {
        for wrapped in [false, true] {
            let mut doc = Document::default();
            let [a, remote, _] = chain(&mut doc, 0., 0.);
            let [b, _, _] = chain(&mut doc, 210., 0.);
            let c = doc.add_atom("C", Point::new(0., 180.));
            generic(&mut doc, remote);
            let mut app = ready(doc, if merge { vec![a, b, c] } else { vec![a, b] });
            let before_hues = app.tab.doc.clone();
            for action in [
                crate::app::color_popover::Action::Color,
                crate::app::color_popover::Action::EditHues,
                crate::app::color_popover::Action::Chip(-3),
            ] {
                let _ = app.update(Message::StyleMenu(action));
            }
            assert_ne!(app.tab.doc, before_hues, "actual uncommitted live hue edit");
            assert!(app.tab.history.can_redo());
            let message = if merge {
                Message::Shortcut(shortcuts::Action::MergeAtoms)
            } else {
                primary_join()
            };
            let message = if wrapped {
                Message::ContextMenu(crate::app::context_menu::Action::Run(Box::new(message)))
            } else {
                message
            };
            rejected(&mut app, message);
        }
    }

    for route in ["share", "fuse", "merge", "explicit"] {
        for host_side in [false, true] {
            let mut doc = Document::default();
            let [a, ar, _] = chain(&mut doc, 0., 0.);
            let [b, br, _] = chain(&mut doc, 210., 0.);
            let [c, _, _] = chain(&mut doc, 210., 160.);
            generic(&mut doc, if host_side { ar } else { br });
            let selected = match route {
                "share" => vec![a, b],
                "fuse" => vec![a, ar, b, br],
                _ => vec![a, b, c],
            };
            let mut app = ready(doc, selected);
            let message = if route == "explicit" {
                Message::Shortcut(shortcuts::Action::MergeAtoms)
            } else {
                primary_join()
            };
            rejected(&mut app, message);
            assert!(app.tab.joining.is_none());
        }
    }
    for metal_first in [false, true] {
        for host_side in [false, true] {
            let mut doc = Document::from_json(include_bytes!(
                "../../../tests/fixtures/coordination/co-en3-before.rsk"
            ))
            .unwrap();
            let site = if host_side {
                let [a, b, _] = chain(&mut doc, -210., 0.);
                doc.add_bond(1, a, 1, "plain");
                b
            } else {
                3
            };
            generic(&mut doc, site);
            let mut app = ready(doc, if metal_first { vec![1, 2] } else { vec![2, 1] });
            rejected(&mut app, primary_join());
        }
    }
    for route in ["coordinate", "merge"] {
        let mut doc = Document::from_json(include_bytes!(
            "../../../tests/fixtures/coordination/co-en3-before.rsk"
        ))
        .unwrap();
        let [_, site, _] = chain(&mut doc, 600., 300.);
        generic(&mut doc, site);
        let selected = if route == "coordinate" {
            vec![1, 2]
        } else {
            (0..3)
                .map(|n| doc.add_atom("C", Point::new(-300., n as f32 * 60.)))
                .collect()
        };
        let member = if route == "coordinate" {
            2
        } else {
            *selected.first().unwrap()
        };
        doc.group_selection(&[member, site]).unwrap();
        doc.groups.last_mut().unwrap().integral = true;
        doc.validate().unwrap();
        let mut app = ready(doc, selected);
        let before = snapshot(&app);
        let _ = app.update(primary_join());
        assert!(!app.error, "{}", app.status);
        assert_ne!(app.tab.doc, before.doc);
        assert_eq!(app.tab.doc.generic, before.doc.generic);
        assert_eq!(app.tab.history.undo_frames().len(), before.undo.len() + 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before.doc);
    }
}

#[test]
fn begin_move_or_coordinate_rejects_generic_source_without_preparing_or_changing_camera() {
    for route in ["begin", "coordinate", "share", "fuse"] {
        let mut doc = Document::default();
        // Four-endpoint Join moves the first bond in document order.
        let [source, source_end, _] = chain(&mut doc, 210., 0.);
        let [host, host_end, _] = chain(&mut doc, 0., 0.);
        let [_, site, _] = chain(&mut doc, 600., 300.);
        generic(&mut doc, site);
        doc.group_selection(&[source, site]).unwrap();
        doc.groups.last_mut().unwrap().integral = true;
        doc.validate().unwrap();
        let selected = match route {
            "share" => vec![source, host],
            "fuse" => vec![source, source_end, host, host_end],
            _ => vec![source],
        };
        let mut app = ready(doc, selected.clone());
        let _ = app.update(Message::InlineText(crate::app::inline_text::Action::Begin(
            None,
            Point::new(300., 300.),
        )));
        let _ = app.update(Message::InlineText(
            crate::app::inline_text::Action::ReplaceText("uncommitted caption".into()),
        ));
        app.tab.selected = selected;
        assert!(app.tab.inline_text.is_some() && app.tab.inline_changed());
        let message = match route {
            "begin" => Message::Join(Action::Begin),
            "coordinate" => Message::Join(Action::BeginCoordination),
            _ => primary_join(),
        };
        rejected(&mut app, message);
        assert!(app.tab.joining.is_none());
    }

    for coordinate in [false, true] {
        for unrelated in [false, true] {
            let mut doc = Document::default();
            let metal = doc.add_atom(if coordinate { "Co" } else { "C" }, Point::default());
            if coordinate {
                doc.atom_mut(metal).unwrap().charge = 3;
            }
            let [source, remote, _] = chain(&mut doc, 210., 0.);
            if coordinate {
                doc.atom_mut(source).unwrap().element = "N".into();
            }
            let site = if unrelated {
                let [_, site, _] = chain(&mut doc, 600., 300.);
                site
            } else {
                remote
            };
            generic(&mut doc, site);
            let mut app = ready(doc, vec![source]);
            let action = if coordinate {
                Action::BeginCoordination
            } else {
                Action::Begin
            };
            if unrelated {
                let before = snapshot(&app);
                let _ = app.update(Message::Join(action));
                assert!(!app.error, "{}", app.status);
                assert!(app.tab.joining.is_some());
                assert_eq!(app.tab.doc, before.doc);
                assert_eq!(
                    app.tab.history.undo_frames().cloned().collect::<Vec<_>>(),
                    before.undo
                );
                assert_eq!(
                    app.tab.history.redo_frames().cloned().collect::<Vec<_>>(),
                    before.redo
                );
                assert_eq!((app.tab.camera.center, app.tab.camera.zoom), before.camera);
            } else {
                rejected(&mut app, Message::Join(action));
                assert!(app.tab.joining.is_none());
            }
        }
    }
}

#[test]
fn generic_target_rejection_keeps_prepared_preview_and_history_for_retry() {
    for coordinate in [false, true] {
        for draft in ["none", "caption", "hue"] {
            let mut doc = Document::default();
            let [host, remote, _] = chain(&mut doc, 0., 0.);
            let [source, _, _] = chain(&mut doc, 210., 0.);
            let target = doc.add_atom(if coordinate { "Co" } else { "C" }, Point::new(500., 0.));
            if coordinate {
                for id in [host, target] {
                    let atom = doc.atom_mut(id).unwrap();
                    atom.element = "Co".into();
                    atom.charge = 3;
                }
                doc.atom_mut(source).unwrap().element = "N".into();
            }
            generic(&mut doc, remote);
            let mut app = ready(doc, vec![source]);
            let _ = app.update(Message::Join(if coordinate {
                Action::BeginCoordination
            } else {
                Action::Begin
            }));
            assert!(app.tab.joining.is_some());
            let preview = |app: &App| {
                let state = app.tab.joining.as_ref().unwrap();
                (
                    state.prepared.original.clone(),
                    state.prepared.fragment.clone(),
                    state.prepared.base.clone(),
                    state.prepared.moving.clone(),
                    state.anchor,
                    state.mode,
                    state.revision,
                    state.epoch,
                )
            };
            if draft == "caption" {
                let _ = app.update(Message::InlineText(crate::app::inline_text::Action::Begin(
                    None,
                    Point::new(300., 300.),
                )));
                let _ = app.update(Message::InlineText(
                    crate::app::inline_text::Action::ReplaceText("pending caption".into()),
                ));
                assert!(app.tab.inline_changed());
            } else if draft == "hue" {
                let before_hues = app.tab.doc.clone();
                for action in [
                    crate::app::color_popover::Action::Color,
                    crate::app::color_popover::Action::EditHues,
                    crate::app::color_popover::Action::Chip(-3),
                ] {
                    let _ = app.update(Message::StyleMenu(action));
                }
                assert_ne!(app.tab.doc, before_hues);
            }
            let prepared = preview(&app);
            rejected(
                &mut app,
                Message::Canvas(Edit::Template(Point::default(), None)),
            );
            assert_eq!(preview(&app), prepared);
            if draft == "caption" {
                let _ = app.update(Message::InlineText(
                    crate::app::inline_text::Action::Finish(false),
                ));
            } else if draft == "hue" {
                for action in [
                    crate::app::color_popover::Action::Cancel,
                    crate::app::color_popover::Action::Close,
                ] {
                    let _ = app.update(Message::StyleMenu(action));
                }
            }
            let before = snapshot(&app);
            let _ = app.update(Message::Canvas(Edit::Template(Point::new(500., 0.), None)));
            assert!(!app.error, "{}", app.status);
            assert!(app.tab.joining.is_none());
            assert_eq!(app.tab.history.undo_frames().len(), before.undo.len() + 1);
            assert!(!app.tab.history.can_redo());
            assert_eq!(app.tab.doc.generic, before.doc.generic);
            let after = app.tab.doc.clone();
            assert_ne!(after, before.doc);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before.doc);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, after);
        }
    }
}
