use super::*;
use crate::canvas::Edit;
use reshiki::document::Point;

fn edit_cell(app: &mut App, action: CellAction) {
    app.sync_tables();
    let target = app.table_target().unwrap();
    let _ = app.update(Message::Table(Action::Edit(target, action)));
}

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
    edit_cell(&mut app, CellAction::Cell(2));
    edit_cell(&mut app, CellAction::Text("Full editable cell text".into()));
    assert_eq!(app.tab.doc, original);
    edit_cell(&mut app, CellAction::AddText);
    assert!(!app.error, "{}", app.status);
    let caption = app.tab.doc.annotations.last().unwrap();
    assert_eq!(caption.text, "Full editable cell text");
    assert_eq!(
        Membership::new(&app.tab.doc).unwrap().cell_of(caption.id),
        Some(CellRef { table: id, cell: 2 })
    );
    let with_text = app.tab.doc.clone();
    edit_cell(&mut app, CellAction::Width("0".into()));
    edit_cell(&mut app, CellAction::ApplyWidth);
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
    edit_cell(&mut app, CellAction::AssignSelection);
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    app.set_selection(vec![a, b]);
    edit_cell(&mut app, CellAction::AssignSelection);
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
    edit_cell(&mut app, CellAction::Cell(2));
    edit_cell(&mut app, CellAction::AssignSelection);
    assert_eq!(
        Membership::new(&app.tab.doc).unwrap().cell_of(a),
        Some(CellRef { table: id, cell: 2 })
    );
    assert_eq!(app.tab.doc.atoms, assigned.atoms);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, assigned);
    edit_cell(&mut app, CellAction::ReleaseSelection);
    assert_eq!(Membership::new(&app.tab.doc).unwrap().cell_of(a), None);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, assigned);
}

#[test]
fn explicit_cell_choice_wins_over_the_previously_selected_caption() {
    let (mut app, _) = App::new();
    let id = draw(&mut app);
    for (cell, text) in [(1, "First"), (2, "Second")] {
        edit_cell(&mut app, CellAction::Cell(cell));
        edit_cell(&mut app, CellAction::Text(text.into()));
        edit_cell(&mut app, CellAction::AddText);
        let caption = app.tab.doc.annotations.last().unwrap();
        assert_eq!(caption.text, text);
        assert_eq!(
            Membership::new(&app.tab.doc).unwrap().cell_of(caption.id),
            Some(CellRef { table: id, cell })
        );
        assert_eq!(app.tab.selected, [caption.id]);
    }
}

#[test]
fn mixed_table_and_rectangle_properties_style_only_the_rectangle_in_one_undo() {
    use reshiki::graphics::{BracketSides, Graphic, GraphicChange, GraphicStyle};
    let (mut app, _) = App::new();
    let table = draw(&mut app);
    app.tab.doc.graphics.push(Graphic::dragged(
        2,
        GraphicKind::Rectangle,
        Point::new(400., 20.),
        Point::new(500., 120.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    app.set_selection(vec![table, 2]);
    assert!(app.graphic_panel_visible());
    let before = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    let color = Color::Custom([17, 126, 108]);
    let _ = app.update(Message::Graphics(super::super::graphics::Action::Style(
        GraphicChange::Fill(Some(color)),
    )));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.graphics[0], before.graphics[0]);
    assert_eq!(app.tab.doc.graphics[1].style.fill, Some(color));
    assert_eq!(app.tab.history.frames(), frames + 1);
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn stale_cell_actions_cannot_cross_cells_selection_history_epoch_or_tabs() {
    let (mut app, _) = App::new();
    draw(&mut app);
    edit_cell(&mut app, CellAction::Text("Current draft".into()));
    let first = app.table_target().unwrap();
    edit_cell(&mut app, CellAction::Cell(2));
    let before = app.tab.doc.clone();
    for action in [
        CellAction::Text("Old draft".into()),
        CellAction::Width("3".into()),
        CellAction::AddText,
        CellAction::ApplyWidth,
        CellAction::AssignSelection,
        CellAction::Done,
    ] {
        let _ = app.update(Message::Table(Action::Edit(first, action)));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.table_inputs.text, "Current draft");
        assert_eq!(app.active_table_cell().unwrap().cell, 2);
    }
    edit_cell(&mut app, CellAction::Cell(1));
    assert_ne!(
        app.table_target(),
        Some(first),
        "Returning to a cell starts a new control context"
    );
    let _ = app.update(Message::Table(Action::Edit(first, CellAction::AddText)));
    assert_eq!(app.tab.doc, before);
    let selected = app.table_target().unwrap();
    app.set_selection(vec![]);
    let _ = app.update(Message::Table(Action::Edit(selected, CellAction::AddText)));
    assert_eq!(app.tab.doc, before);
    let previous = app.table_target().unwrap();
    edit_cell(&mut app, CellAction::Width("2".into()));
    edit_cell(&mut app, CellAction::ApplyWidth);
    let _ = app.update(Message::Undo);
    assert!(app.tab.history.can_redo());
    let restored = app.tab.doc.clone();
    let _ = app.update(Message::Table(Action::Edit(previous, CellAction::AddText)));
    assert_eq!(app.tab.doc, restored);
    assert!(app.tab.history.can_redo());
    let previous = app.table_target().unwrap();
    app.tab.file_epoch += 1;
    let _ = app.update(Message::Table(Action::Edit(previous, CellAction::AddText)));
    assert_eq!(app.tab.doc, restored);
    let previous = app.table_target().unwrap();
    app.tab.id = super::super::document_tab::TabId(app.tab.id.0 + 1);
    let _ = app.update(Message::Table(Action::Edit(previous, CellAction::AddText)));
    assert_eq!(app.tab.doc, restored);
}

#[test]
fn transformed_merged_cells_add_upright_visible_pickable_text_and_one_undo() {
    use reshiki::{graphics::table::Frame, typography::TextFormat};
    for (x, y) in [
        (Point::new(600., 0.), Point::new(0., 400.)),
        (Point::new(0., 600.), Point::new(-400., 0.)),
        (Point::new(-600., 0.), Point::new(0., -400.)),
        (Point::new(-600., 0.), Point::new(0., 400.)),
        (Point::new(600., 120.), Point::new(1500., 700.)),
        (Point::new(600., 0.), Point::new(5000., 1000.)),
    ] {
        let (mut app, _) = App::new();
        let mut g =
            table::editor::dragged(1, Point::default(), Point::new(600., 400.), [1, 3], 0.6)
                .unwrap();
        g.origin = Point::new(2000., 2000.);
        g.axis_x = x;
        g.axis_y = y;
        let cells = &mut g.table.as_mut().unwrap().cells;
        cells.truncate(1);
        cells[0].id = 37;
        cells[0].hi = Point::new(1., 1.); // One merged cell spans all three columns.
        let frame = Frame::of(&g).unwrap();
        app.tab.doc.graphics = vec![g];
        app.set_selection(vec![1]);
        let before = app.tab.doc.clone();
        edit_cell(&mut app, CellAction::Text("Product".into()));
        edit_cell(&mut app, CellAction::AddText);
        assert!(!app.error, "{}", app.status);
        let a = app.tab.doc.annotations.last().unwrap();
        assert_eq!(a.angle, 0.);
        assert_eq!(
            Membership::new(&app.tab.doc).unwrap().cell_of(a.id),
            Some(CellRef { table: 1, cell: 37 })
        );
        let layout = reshiki::scene::table_annotation_layout(a, frame);
        let visible = layout
            .fragments
            .iter()
            .flat_map(|fragment| {
                let at = a.position.offset(fragment.position.x, fragment.position.y);
                reshiki::style::text_ink_boxes(
                    &fragment.text,
                    fragment.style.size(),
                    &fragment.style,
                )
                .into_iter()
                .map(move |(lo, hi)| (at.offset(lo.x, lo.y), at.offset(hi.x, hi.y)))
            })
            .find(|(lo, hi)| {
                [*lo, Point::new(hi.x, lo.y), *hi, Point::new(lo.x, hi.y)]
                    .into_iter()
                    .all(|p| frame.contains(p))
            })
            .expect("At least one complete glyph must remain visible");
        assert_eq!(
            crate::canvas::hit_object(&app.tab.doc, visible.0.lerp(visible.1, 0.5), 0.),
            Some(a.id)
        );
        if x.y == 0. && x.x > 0. && y.x == 0. && y.y > 0. {
            assert_eq!(a.position, frame.point(Point::new(0.05, 0.08)));
            assert_eq!(a.format.width_pt, TextFormat::default().width_pt);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }
}

#[test]
fn unreadably_thin_affine_cell_refuses_text_without_an_edit_or_lost_draft() {
    let (mut app, _) = App::new();
    let mut g =
        table::editor::dragged(1, Point::default(), Point::new(600., 400.), [1, 1], 0.6).unwrap();
    g.axis_y = Point::new(600., 0.002);
    g.validate().unwrap();
    app.tab.doc.graphics = vec![g];
    app.set_selection(vec![1]);
    let before = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    edit_cell(&mut app, CellAction::Text("Product".into()));
    edit_cell(&mut app, CellAction::AddText);
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.frames(), frames);
    assert_eq!(app.tab.table_inputs.text, "Product");
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
    edit_cell(
        &mut app,
        CellAction::Text("Original editable text wraps within its owning cell".into()),
    );
    edit_cell(&mut app, CellAction::AddText);
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
    edit_cell(&mut app, CellAction::Cell(2));
    edit_cell(&mut app, CellAction::AssignSelection);
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
