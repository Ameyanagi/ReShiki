use super::*;
use crate::canvas::Edit;

fn draw(app: &mut App) -> u64 {
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Table)));
    let _ = app.update(Message::Canvas(Edit::Graphic(
        Point::new(30., 20.),
        Point::new(330., 180.),
        false,
    )));
    assert!(!app.error, "{}", app.status);
    app.tab
        .doc
        .graphics
        .iter()
        .find(|g| g.table.is_some())
        .unwrap()
        .id
}

#[test]
fn table_tool_commits_the_preview_grid_and_undo_restores_the_drawing() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Table)));
    assert!(app.table_card().is_some());
    let _ = app.update(Message::Table(Action::Rows("3".into())));
    let _ = app.update(Message::Table(Action::Columns("4".into())));
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Table(Action::ApplyGrid));
    let id = draw(&mut app);
    let graphic = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    let preview = table::editor::dragged(
        id,
        Point::new(30., 20.),
        Point::new(330., 180.),
        [3, 4],
        app.tab.doc.drawing_style.line_width_pt,
    )
    .unwrap();
    assert_eq!(graphic.table, preview.table);
    assert_eq!(graphic.origin, preview.origin);
    assert_eq!(graphic.axis_x, preview.axis_x);
    assert_eq!(app.tab.selected, [id]);
    assert!(app.table_card().is_some());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn active_cell_text_and_border_edits_are_native_and_each_one_undo() {
    let (mut app, _) = App::new();
    let id = draw(&mut app);
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Table(Action::Cell(2)));
    let _ = app.update(Message::Table(Action::Text(
        "Full editable cell text".into(),
    )));
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Table(Action::AddText));
    assert!(!app.error, "{}", app.status);
    let caption = app.tab.doc.annotations.last().unwrap();
    assert_eq!(caption.text, "Full editable cell text");
    assert_eq!(
        Membership::new(&app.tab.doc).unwrap().cell_of(caption.id),
        Some(CellRef { table: id, cell: 2 })
    );
    let with_text = app.tab.doc.clone();
    let _ = app.update(Message::Table(Action::Width("0".into())));
    let _ = app.update(Message::Table(Action::ApplyWidth));
    assert!(!app.error, "{}", app.status);
    let cell = app
        .tab
        .doc
        .graphics
        .iter()
        .find(|g| g.id == id)
        .unwrap()
        .table
        .as_ref()
        .unwrap()
        .cell(2)
        .unwrap();
    assert_eq!(cell.borders.len(), 4);
    assert!(cell.borders.iter().all(|b| b.width_pt == 0.));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, with_text);
    assert_eq!(app.tab.table_inputs.width.parse::<f32>().unwrap(), 0.6);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.table_inputs.width, "0");
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn assigning_and_releasing_a_molecule_keeps_geometry_and_refuses_partial_graphs() {
    let (mut app, _) = App::new();
    let id = draw(&mut app);
    let a = app.tab.doc.add_atom("C", Point::new(500., 100.));
    let b = app.tab.doc.add_atom("O", Point::new(540., 100.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let original = app.tab.doc.clone();
    app.set_selection(vec![a]);
    let _ = app.update(Message::Table(Action::AssignSelection));
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    app.set_selection(vec![a, b]);
    let _ = app.update(Message::Table(Action::AssignSelection));
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .map(|a| a.position)
            .collect::<Vec<_>>(),
        original
            .atoms
            .iter()
            .map(|a| a.position)
            .collect::<Vec<_>>()
    );
    let membership = Membership::new(&app.tab.doc).unwrap();
    assert_eq!(membership.cell_of(a), Some(CellRef { table: id, cell: 1 }));
    assert_eq!(membership.cell_of(b), membership.cell_of(a));
    let assigned = app.tab.doc.clone();
    let _ = app.update(Message::Table(Action::Cell(2)));
    let _ = app.update(Message::Table(Action::AssignSelection));
    assert_eq!(
        Membership::new(&app.tab.doc).unwrap().cell_of(a),
        Some(CellRef { table: id, cell: 2 })
    );
    assert_eq!(app.tab.doc.atoms, assigned.atoms);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, assigned);
    let _ = app.update(Message::Table(Action::ReleaseSelection));
    assert_eq!(Membership::new(&app.tab.doc).unwrap().cell_of(a), None);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, assigned);
}

#[test]
fn explicit_cell_choice_wins_over_the_previously_selected_caption() {
    let (mut app, _) = App::new();
    let id = draw(&mut app);
    for (cell, text) in [(1, "First"), (2, "Second")] {
        let _ = app.update(Message::Table(Action::Cell(cell)));
        let _ = app.update(Message::Table(Action::Text(text.into())));
        let _ = app.update(Message::Table(Action::AddText));
        let caption = app.tab.doc.annotations.last().unwrap();
        assert_eq!(caption.text, text);
        assert_eq!(
            Membership::new(&app.tab.doc).unwrap().cell_of(caption.id),
            Some(CellRef { table: id, cell })
        );
        assert_eq!(app.tab.selected, [caption.id]);
    }
}

#[tokio::test]
#[ignore = "Capture with isolated RESHIKI_DATA_DIR and RESHIKI_TABLE_EDITOR_QA_DIR"]
async fn manual_table_editor_renderer_evidence() -> anyhow::Result<()> {
    use iced::advanced::{renderer::Headless, widget::Tree};
    let output = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_TABLE_EDITOR_QA_DIR").expect("Capture folder"),
    );
    std::env::var_os("RESHIKI_DATA_DIR").expect("Isolated application data folder");
    std::fs::create_dir_all(&output)?;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Renderer");
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.inspector_open = true;
    app.tab.fit_to_view = false;
    let capture = |app: &mut App, renderer: &mut iced::Renderer, file: &str, size| {
        let mut tree = Tree::empty();
        for _ in 0..3 {
            let messages =
                crate::app::workspace::layout_snapshots::pass(app, renderer, &mut tree, size, None);
            if messages.is_empty() {
                break;
            }
            for message in messages {
                let _ = app.update(message);
            }
        }
        let _ = crate::app::workspace::layout_snapshots::pass(
            app,
            renderer,
            &mut tree,
            size,
            Some(&output.join(file)),
        );
    };
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Table)));
    capture(
        &mut app,
        &mut renderer,
        "table-tool.png",
        iced::Size::new(1280., 820.),
    );
    let table = draw(&mut app);
    let _ = app.update(Message::Table(Action::Text(
        "Original editable text wraps within its owning cell".into(),
    )));
    let _ = app.update(Message::Table(Action::AddText));
    let mut ring = Vec::new();
    for i in 0..6 {
        let angle = i as f32 * std::f32::consts::TAU / 6.;
        ring.push(app.tab.doc.add_atom(
            "C",
            Point::new(223. + 32. * angle.cos(), 65. + 32. * angle.sin()),
        ));
    }
    for i in 0..6 {
        app.tab.doc.add_bond(
            ring[i],
            ring[(i + 1) % 6],
            if i % 2 == 0 { 2 } else { 1 },
            "plain",
        );
    }
    app.set_selection(ring);
    let _ = app.update(Message::Table(Action::Cell(2)));
    let _ = app.update(Message::Table(Action::AssignSelection));
    app.tab.doc.validate().map_err(anyhow::Error::msg)?;
    app.tab.camera.center = Point::new(180., 100.);
    app.tab.camera.zoom = 1.6;
    app.set_selection(vec![table]);
    capture(
        &mut app,
        &mut renderer,
        "table-cell.png",
        iced::Size::new(1280., 820.),
    );
    capture(
        &mut app,
        &mut renderer,
        "table-cell-minimum.png",
        iced::Size::new(1040., 680.),
    );
    std::fs::write(
        output.join("table-editor.rsk"),
        app.tab.doc.file_json().map_err(anyhow::Error::msg)?,
    )?;
    Ok(())
}
