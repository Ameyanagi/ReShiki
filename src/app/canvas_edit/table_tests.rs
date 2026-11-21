use crate::app::{App, Message};
use crate::canvas::{Edit, Tool};
use reshiki::{
    document::{Document, Point},
    graphics::{
        Graphic, GraphicKind,
        table::{Cell, CellRef, Membership, Table},
    },
};

fn app() -> (App, u64, CellRef) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let atom = app.tab.doc.add_atom("C", Point::new(60., 60.));
    let mut graphic = Graphic::dragged(
        100,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(300., 200.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.kind = GraphicKind::Table;
    graphic.table = Some(Box::new(Table {
        cells: vec![
            Cell {
                id: 1,
                lo: Point::default(),
                hi: Point::new(0.5, 1.),
                members: vec![atom],
                borders: vec![],
            },
            Cell {
                id: 2,
                lo: Point::new(0.5, 0.),
                hi: Point::new(1., 1.),
                members: vec![],
                borders: vec![],
            },
        ],
    }));
    app.tab.doc.graphics.push(graphic);
    app.tab.doc.validate().unwrap();
    (
        app,
        atom,
        CellRef {
            table: 100,
            cell: 1,
        },
    )
}
fn check_history(app: &mut App, before: Document, target: CellRef) {
    assert!(!app.error, "{}", app.status);
    app.tab.doc.validate().unwrap();
    let ownership = Membership::new(&app.tab.doc).unwrap();
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .all(|atom| ownership.cell_of(atom.id) == Some(target))
    );
    assert!(app.tab.doc.atoms.len() > before.atoms.len());
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}
#[test]
fn imported_cell_atoms_support_existing_click_drag_atom_chain_and_keyboard_tools() {
    for mode in 0..5 {
        let (mut app, atom, target) = app();
        let before = app.tab.doc.clone();
        app.tool = if mode == 2 { Tool::Atom } else { Tool::Bond(1) };
        match mode {
            0 => {
                let _ = app.update(Message::Canvas(Edit::Click(Point::new(60., 60.))));
            }
            1 | 2 => {
                let _ = app.update(Message::Canvas(Edit::Bond(
                    Point::new(60., 60.),
                    Point::new(100., 60.),
                    Some(atom),
                    None,
                )));
            }
            3 => {
                let _ = app.update(Message::Canvas(Edit::Chain {
                    points: vec![
                        Point::new(60., 60.),
                        Point::new(90., 60.),
                        Point::new(105., 86.),
                    ],
                    source: Some(atom),
                    source_text: None,
                    target: None,
                }));
            }
            _ => {
                app.tool = Tool::Select;
                let _ = app.context_key_at("1", reshiki::keyboard_drawing::Target::Atom(atom));
            }
        }
        check_history(&mut app, before, target);
    }
}
#[test]
fn dragging_between_existing_different_cell_atoms_rolls_back_without_an_undo_step() {
    let (mut app, atom, _) = app();
    let other = app.tab.doc.add_atom("O", Point::new(240., 60.));
    app.tab.doc.graphics[0].table.as_mut().unwrap().cells[1]
        .members
        .push(other);
    let before = app.tab.doc.clone();
    app.tool = Tool::Bond(1);
    let _ = app.update(Message::Canvas(Edit::Bond(
        Point::new(60., 60.),
        Point::new(240., 60.),
        Some(atom),
        Some(other),
    )));
    assert!(app.error);
    assert!(app.status.contains("same table cell"), "{}", app.status);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn cell_callout_drafts_cancel_and_finish_with_caption_leader_style_and_one_undo() {
    use crate::app::inline_text;
    for mode in 0..3 {
        let (mut app, a, cell) = app();
        let b = app.tab.doc.add_atom("C", Point::new(100., 60.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let shape = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            shape,
            GraphicKind::Rectangle,
            Point::new(20., 90.),
            Point::new(120., 150.),
            Default::default(),
            Default::default(),
            false,
        ));
        app.tab.doc.graphics[0].table.as_mut().unwrap().cells[0]
            .members
            .extend([b, shape]);
        let selected = match mode {
            0 => vec![a],
            1 => vec![a, b],
            _ => vec![shape],
        };
        app.tab.selected = selected.clone();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::AddCallout);
        let _ = app.update(Message::InlineText(inline_text::Action::ReplaceText(
            "Cell callout".into(),
        )));
        assert!(app.inline_candidate().is_ok());
        assert_eq!(app.tab.doc, before);
        assert!(app.finish_inline(false));
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        app.tab.selected = selected;
        let target = app.callout_target().unwrap();
        let _ = app.update(Message::AddCallout);
        let _ = app.update(Message::InlineText(inline_text::Action::ReplaceText(
            "Cell callout".into(),
        )));
        app.tab.caption_format.style.bold = true;
        let format = app.tab.caption_format.clone();
        let draft = app.inline_candidate().unwrap();
        let text = draft.annotations.last().unwrap().id;
        let line = draft
            .graphics
            .iter()
            .find(|g| g.leader.is_some())
            .unwrap()
            .id;
        let recovery = app.recovery_document();
        for doc in [&draft, &recovery] {
            let owner = Membership::new(doc).unwrap();
            assert_eq!(owner.cell_of(text), Some(cell));
            assert_eq!(owner.cell_of(line), Some(cell));
            doc.validate().unwrap();
        }
        assert!(app.finish_inline(true), "{}", app.status);
        assert!(!app.error, "{}", app.status);
        assert!(app.tab.inline_text.is_none());
        app.tab.doc.validate().unwrap();
        let made = app.tab.doc.graphics.iter().find(|g| g.id == line).unwrap();
        assert_eq!(made.leader.as_ref().unwrap().end, Some(target));
        assert_eq!(app.tab.doc.annotations.last().unwrap().format, format);
        assert_eq!(app.tab.history.undo_frames().len(), 1);
        let after = app.tab.doc.clone();
        assert_eq!(
            Document::from_native_file(&after.file_json().unwrap()).unwrap(),
            after.current()
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}

#[test]
fn cell_centroid_and_attachment_commands_keep_ownership_or_reject_without_history() {
    use crate::app::inspector::Action;
    for mode in 0..3 {
        for owner in [None, Some(1), Some(2)] {
            let (mut app, a, cell) = app();
            let b = app.tab.doc.add_atom(
                "O",
                Point::new(if owner == Some(1) { 100. } else { 240. }, 60.),
            );
            if let Some(owner) = owner {
                app.tab.doc.graphics[0]
                    .table
                    .as_mut()
                    .unwrap()
                    .cell_mut(owner)
                    .unwrap()
                    .members
                    .push(b);
            }
            app.tab.doc.validate().unwrap();
            app.tab.selected = vec![a, b];
            let before = app.tab.doc.clone();
            let action = match mode {
                0 => Action::Centroid,
                1 => Action::Attachment(reshiki::attachments::Kind::MultiCenter),
                _ => Action::Attachment(reshiki::attachments::Kind::Variable),
            };
            let _ = app.update(Message::InspectorAction(action));
            if owner == Some(1) {
                assert!(!app.error, "{}", app.status);
                let id = app.tab.selected[0];
                assert_eq!(
                    Membership::new(&app.tab.doc).unwrap().cell_of(id),
                    Some(cell)
                );
                assert_eq!(app.tab.doc.atom(id).unwrap().centroid, [a, b]);
                app.tab.doc.validate().unwrap();
                assert_eq!(app.tab.history.undo_frames().len(), 1);
                let after = app.tab.doc.clone();
                assert_eq!(
                    Document::from_native_file(&after.file_json().unwrap()).unwrap(),
                    after.current()
                );
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, before);
                let _ = app.update(Message::Redo);
                assert_eq!(app.tab.doc, after);
            } else {
                assert!(app.error);
                assert!(app.status.contains("same table cell"), "{}", app.status);
                assert_eq!(app.tab.doc, before);
                assert!(!app.tab.history.can_undo());
            }
        }
    }
}

#[test]
fn table_command_and_numeric_half_turns_keep_the_frame_center_with_off_center_captions() {
    use crate::app::numeric_transforms::{Action, Field};
    use reshiki::document::Annotation;
    use reshiki::editing::Transform;
    use reshiki::graphics::table::Frame;
    for numeric in [false, true] {
        for overflow in [false, true] {
            let (mut app, atom, _) = app();
            let table = 100;
            app.tab.doc.graphics[0].table.as_mut().unwrap().cells = (0..6)
                .map(|i| Cell {
                    id: i + 1,
                    lo: Point::new((i % 3) as f32 / 3., (i / 3) as f32 / 2.),
                    hi: Point::new((i % 3 + 1) as f32 / 3., (i / 3 + 1) as f32 / 2.),
                    members: if i == 0 { vec![atom] } else { vec![] },
                    borders: vec![],
                })
                .collect();
            for (cell, position) in [
                (0, Point::new(12., 20.)),
                (
                    1,
                    if overflow {
                        Point::new(5000., -3000.)
                    } else {
                        Point::new(120., 25.)
                    },
                ),
            ] {
                let id = app.tab.doc.next_id();
                app.tab.doc.annotations.push(Annotation {
                    id,
                    position,
                    text: "off-center caption".into(),
                    angle: 0.,
                    format: Default::default(),
                });
                app.tab.doc.graphics[0].table.as_mut().unwrap().cells[cell]
                    .members
                    .push(id);
            }
            app.tab.doc.validate().unwrap();
            app.tab.selected = vec![table];
            app.sync_numeric_transforms();
            let before = app.tab.doc.clone();
            let center = Frame::of(&before.graphics[0])
                .unwrap()
                .point(Point::new(0.5, 0.5));
            if numeric {
                let _ = app.update(Message::NumericTransform(Action::Input(
                    Field::Rotation,
                    "180".into(),
                )));
                let _ = app.update(Message::NumericTransform(Action::Apply(Field::Rotation)));
            } else {
                let _ = app.update(Message::Transform(Transform::Rotate(180.)));
            }
            assert!(!app.error, "{numeric}, {overflow}: {}", app.status);
            assert!(
                Frame::of(&app.tab.doc.graphics[0])
                    .unwrap()
                    .point(Point::new(0.5, 0.5))
                    .distance(center)
                    < 1e-4
            );
            assert_eq!(app.tab.doc.graphics[0].table, before.graphics[0].table);
            for (old, new) in before.annotations.iter().zip(&app.tab.doc.annotations) {
                assert!(
                    new.position.distance(Point::new(
                        2. * center.x - old.position.x,
                        2. * center.y - old.position.y
                    )) < 0.001
                );
            }
            assert_eq!(app.tab.history.undo_frames().len(), 1);
            let after = app.tab.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, after);
        }
    }
}

#[test]
fn two_click_mechanism_arrows_in_cells_keep_targets_and_one_undo_step() {
    use reshiki::arrow_anchors::Pick;
    for kind in 0..3 {
        let (mut app, a, cell) = app();
        app.tab.camera.zoom = 2.5;
        let b = app.tab.doc.add_atom("C", Point::new(120., 60.));
        let c = app.tab.doc.add_atom("N", Point::new(110., 140.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.graphics[0].table.as_mut().unwrap().cells[0]
            .members
            .extend([b, c]);
        reshiki::scientific::attach(
            app.tab.doc.atom_mut(a).unwrap(),
            reshiki::scientific::SymbolKind::LonePair,
            Point::new(0., -25.),
        )
        .unwrap();
        app.tab.doc.atom_mut(a).unwrap().marks[0].id = None;
        app.tab.doc.atom_mut(a).unwrap().mark_serial = 0;
        let _ = app.update(Message::ArrowStyle(reshiki::arrows::Preset::Curved));
        assert!(app.arrow_attach_mode());
        let before = app.tab.doc.clone();
        let source = match kind {
            0 => Point::new(60., 60.),
            1 => Point::new(90., 60.),
            _ => Point::new(60., 35.),
        };
        let frames = app.tab.history.undo_frames().len();
        let _ = app.update(Message::Canvas(Edit::ArrowTarget(source, false)));
        assert!(!app.error, "{}", app.status);
        assert!(matches!(
            (kind, app.tab.arrows.source()),
            (0, Some(Pick::Atom(_)))
                | (1, Some(Pick::Bond { .. }))
                | (2, Some(Pick::LonePair { .. }))
        ));
        assert_eq!(app.tab.doc, before, "the first click is UI state only");
        assert_eq!(app.tab.history.undo_frames().len(), frames);
        let _ = app.update(Message::Canvas(Edit::ArrowTarget(
            Point::new(110., 140.),
            false,
        )));
        assert!(!app.error, "kind {kind}: {}", app.status);
        assert!(app.tab.arrows.source().is_none());
        let arrow = &app.tab.doc.arrows[0];
        assert!(arrow.start_anchor.is_some() && arrow.end_anchor.is_some());
        assert_eq!(app.tab.selected, [arrow.id]);
        assert_eq!(
            Membership::new(&app.tab.doc).unwrap().cell_of(arrow.id),
            Some(cell)
        );
        app.tab.doc.validate().unwrap();
        assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
        let after = app.tab.doc.clone();
        let reopened = Document::from_native_file(&after.file_json().unwrap()).unwrap();
        assert_eq!(
            Membership::new(&reopened).unwrap().cell_of(arrow.id),
            Some(cell)
        );
        assert_eq!(
            reopened.arrows, after.arrows,
            "anchors and geometry stay exact"
        );
        // Saving raises the native feature version; the live Undo snapshot
        // keeps its original version until it is reopened.
        assert_eq!(reopened, after.current());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}

#[test]
fn two_click_mechanism_arrows_across_cells_are_atomic_and_leave_no_history() {
    for other_cell in [false, true] {
        let (mut app, a, _) = app();
        app.tab.camera.zoom = 2.5;
        let b = app.tab.doc.add_atom("O", Point::new(240., 60.));
        if other_cell {
            app.tab.doc.graphics[0].table.as_mut().unwrap().cells[1]
                .members
                .push(b);
        }
        reshiki::scientific::attach(
            app.tab.doc.atom_mut(a).unwrap(),
            reshiki::scientific::SymbolKind::LonePair,
            Point::new(0., -25.),
        )
        .unwrap();
        app.tab.doc.atom_mut(a).unwrap().marks[0].id = None;
        app.tab.doc.atom_mut(a).unwrap().mark_serial = 0;
        let _ = app.update(Message::ArrowStyle(reshiki::arrows::Preset::Curved));
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Canvas(Edit::ArrowTarget(
            Point::new(60., 35.),
            false,
        )));
        assert!(!app.error, "{}", app.status);
        let _ = app.update(Message::Canvas(Edit::ArrowTarget(
            Point::new(240., 60.),
            false,
        )));
        assert!(app.error);
        assert!(app.status.contains("same table cell"), "{}", app.status);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        assert!(app.tab.arrows.source().is_some());
        let _ = app.update(Message::Escape);
        assert!(app.tab.arrows.source().is_none());
        assert_eq!(app.tab.doc, before);
    }
}

#[test]
fn first_linked_formula_caption_on_imported_cell_molecule_has_one_undo() {
    let (mut app, atom, target) = app();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ToggleProperty(
        vec![atom],
        reshiki::linked_properties::Property::Formula,
    ));
    assert!(!app.error, "{}", app.status);
    app.tab.doc.validate().unwrap();
    let caption = app.tab.doc.property_captions[0].annotation;
    assert_eq!(
        Membership::new(&app.tab.doc).unwrap().cell_of(caption),
        Some(target)
    );
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn bonds_and_chains_from_cell_captions_keep_membership_in_one_undo_step() {
    for mode in 0..4 {
        let (mut app, atom, cell) = app();
        let text = app.tab.doc.next_id();
        app.tab.doc.annotations.push(reshiki::document::Annotation {
            id: text,
            position: Point::new(90., 80.),
            text: "N".into(),
            angle: 0.,
            format: Default::default(),
        });
        app.tab.doc.graphics[0].table.as_mut().unwrap().cells[0]
            .members
            .push(text);
        let before = app.tab.doc.clone();
        let start = reshiki::atom_text::text_atom_position(&before, text).unwrap();
        app.tool = Tool::Bond(1);
        let edit = if mode == 3 {
            Edit::Chain {
                points: vec![start, start.offset(30., 0.), start.offset(45., 26.)],
                source: None,
                source_text: Some(text),
                target: None,
            }
        } else {
            Edit::TextBond {
                source: None,
                source_text: Some(text),
                start,
                end: match mode {
                    0 => None,
                    1 => Some(start.offset(40., 0.)),
                    _ => Some(before.atom(atom).unwrap().position),
                },
                target: (mode == 2).then_some(atom),
                target_text: None,
            }
        };
        let frames = app.tab.history.undo_frames().len();
        let _ = app.update(Message::Canvas(edit));
        assert!(!app.error, "mode {mode}: {}", app.status);
        assert!(app.tab.doc.annotations.iter().all(|a| a.id != text));
        assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
        check_history(&mut app, before, cell);
    }
}

#[test]
fn normal_inline_shape_and_arrow_captions_inherit_the_holder_cell() {
    use crate::app::inline_text;
    for shape_text in [false, true] {
        let (mut app, _, cell) = app();
        let holder = app.tab.doc.next_id();
        if shape_text {
            app.tab.doc.graphics.push(Graphic::dragged(
                holder,
                GraphicKind::Rectangle,
                Point::new(20., 90.),
                Point::new(130., 160.),
                Default::default(),
                Default::default(),
                false,
            ));
        } else {
            app.tab.doc.arrows.push(reshiki::document::Arrow::new(
                holder,
                Point::new(20., 130.),
                Point::new(130., 130.),
                Default::default(),
                Default::default(),
            ));
        }
        app.tab.doc.graphics[0].table.as_mut().unwrap().cells[0]
            .members
            .push(holder);
        let before = app.tab.doc.clone();
        let frames = app.tab.history.undo_frames().len();
        if shape_text {
            let _ = app.update(Message::Canvas(Edit::BeginText(holder)));
        } else {
            let _ = app.update(Message::InlineText(inline_text::Action::Begin(
                None,
                Point::new(50., 95.),
            )));
        }
        let _ = app.update(Message::InlineText(inline_text::Action::ReplaceText(
            "heat".into(),
        )));
        assert!(
            app.inline_candidate().is_ok(),
            "the draft is valid before commit"
        );
        assert_eq!(
            app.tab.doc, before,
            "a caption draft does not edit the source"
        );
        let _ = app.update(Message::InlineText(inline_text::Action::Finish(true)));
        assert!(!app.error, "{}", app.status);
        app.tab.doc.validate().unwrap();
        let text = app.tab.doc.annotations.last().unwrap().id;
        assert_eq!(
            Membership::new(&app.tab.doc).unwrap().cell_of(text),
            Some(cell)
        );
        if shape_text {
            assert_eq!(
                app.tab
                    .doc
                    .graphics
                    .iter()
                    .find(|g| g.id == holder)
                    .unwrap()
                    .text,
                Some(text)
            );
        } else {
            assert!(
                app.tab
                    .doc
                    .arrow_labels
                    .iter()
                    .any(|l| l.arrow == holder && l.annotation == text)
            );
        }
        assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}
