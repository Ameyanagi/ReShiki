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
