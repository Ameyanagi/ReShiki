use super::*;
use crate::canvas::hit::{bond_target_with, bond_text_at, hit_object, hit_selection};
use reshiki::{
    arrows::{ArrowStyle, Preset},
    chains::BondDrawing,
    document::Arrow,
    graphics::{
        Graphic, GraphicKind, GraphicStyle,
        table::{Border, Side, Table},
    },
    palette::Color,
};

fn table(members: Vec<u64>) -> Graphic {
    let mut graphic = Graphic::dragged(
        1000,
        GraphicKind::Rectangle,
        World::default(),
        World::new(200., 200.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.kind = GraphicKind::Table;
    graphic.table = Some(Box::new(Table {
        cells: vec![Cell {
            id: 1,
            lo: World::default(),
            hi: World::new(0.5, 1.),
            members,
            borders: vec![],
        }],
    }));
    graphic
}

fn caption(id: u64, position: World, text: &str) -> Annotation {
    Annotation {
        id,
        position,
        text: text.into(),
        format: Default::default(),
        angle: 0.,
    }
}

fn rectangle(id: u64, p: World, layer: i32) -> Graphic {
    let mut graphic = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        p.offset(-15., -15.),
        p.offset(15., 15.),
        GraphicStyle {
            fill: Some(Color::Ink),
            ..Default::default()
        },
        Default::default(),
        false,
    );
    graphic.layer = layer;
    graphic
}

#[test]
fn affine_and_reflected_clips_filter_labels_before_the_winner_is_chosen() {
    for (axis_x, axis_y, local) in [
        (
            World::new(200., 0.),
            World::new(80., 200.),
            World::new(0.6, 0.05),
        ),
        (
            World::new(-200., 0.),
            World::new(80., 200.),
            World::new(0.6, 0.95),
        ),
    ] {
        let mut doc = Document::default();
        let mut graphic = table(vec![]);
        graphic.origin = World::new(240., 20.);
        graphic.axis_x = axis_x;
        graphic.axis_y = axis_y;
        let frame = Frame::of(&graphic).unwrap();
        let point = frame.point(local);
        let visible = doc.add_atom("N", point);
        let hidden = doc.add_atom("N", point);
        graphic.table.as_mut().unwrap().cells[0].members = vec![hidden];
        let cell_frame = graphic.table.as_ref().unwrap().cells[0]
            .frame(&graphic)
            .unwrap();
        assert!(!cell_frame.contains(point));
        let corners = cell_frame.corners();
        assert!(corners.iter().any(|p| p.x < point.x));
        assert!(corners.iter().any(|p| p.x > point.x));
        assert!(corners.iter().any(|p| p.y < point.y));
        assert!(corners.iter().any(|p| p.y > point.y));
        // Equal-distance labels retain the existing reverse atom order.
        assert_eq!(
            reshiki::scene::atom_label_hit(&doc, point, 0.),
            Some(hidden)
        );
        doc.graphics.push(graphic);
        assert_eq!(hit_object(&doc, point, 8.), Some(visible));
        assert_eq!(hit_selection(&doc, point, 8.), vec![visible]);
    }
}

#[test]
fn hidden_graphic_and_arrow_candidates_do_not_mask_visible_lower_objects() {
    let mut doc = Document::default();
    let point = World::new(140., 50.);
    doc.graphics.push(rectangle(1, point, 1));
    doc.graphics.push(rectangle(2, point, 2));
    doc.graphics.push(table(vec![2, 4]));
    doc.arrows.push(Arrow::new(
        3,
        point.offset(-25., 0.),
        point.offset(25., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    doc.arrows.push(Arrow::new(
        4,
        point.offset(-25., 0.),
        point.offset(25., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    assert_eq!(hit_object(&doc, point, 3.), Some(1));
    doc.graphics.retain(|g| g.id != 1);
    assert_eq!(hit_object(&doc, point, 3.), Some(3));
}

#[test]
fn a_clipped_caption_does_not_mask_visible_caption_ink_below_it() {
    let mut doc = Document::default();
    let lower = caption(1, World::new(130., 30.), "N");
    let point = reshiki::atom_text::text_first_glyph_center(&lower).unwrap();
    doc.annotations.push(lower);
    let upper = caption(2, World::new(90., 30.), "NNNNNNNNNNNNNNNN");
    assert!(upper.contains(point));
    doc.annotations.push(upper);
    doc.graphics.push(table(vec![2]));
    let before = doc.annotations.clone();
    assert_eq!(hit_object(&doc, point, 5.), Some(1));
    assert_eq!(bond_text_at(&doc, point, 5.), Some(1));
    assert_eq!(doc.annotations, before);
}

#[test]
fn wrapped_caption_rows_use_the_same_turned_layout_as_the_scene() {
    let mut doc = Document::default();
    let mut graphic = table(vec![1]);
    graphic.axis_x = World::new(100., 40.);
    graphic.axis_y = World::new(-40., 300.);
    let parent = Frame::of(&graphic).unwrap();
    let frame = graphic.table.as_ref().unwrap().cells[0]
        .frame(&graphic)
        .unwrap();
    let mut annotation = caption(
        1,
        parent.point(World::new(0.08, 0.05)),
        "Supercalifragilisticexpialidocious",
    );
    annotation.angle = graphic.axis_x.y.atan2(graphic.axis_x.x).to_degrees();
    let layout = reshiki::scene::table_annotation_layout(&annotation, frame);
    let pivot = annotation
        .position
        .offset(layout.width / 2., layout.height / 2.);
    let point = layout
        .fragments
        .iter()
        .filter(|fragment| fragment.position.y > annotation.size().1)
        .find_map(|fragment| {
            reshiki::style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)
                .into_iter()
                .map(|(lo, hi)| lo.lerp(hi, 0.5))
                .map(|ink| {
                    annotation
                        .position
                        .offset(fragment.position.x + ink.x, fragment.position.y + ink.y)
                        .turned(pivot, annotation.angle)
                })
                .find(|point| frame.contains(*point) && !annotation.contains(*point))
        })
        .expect("A narrow cell should expose wrapped ink below the canonical box");
    assert!(frame.contains(point));
    assert!(!annotation.contains(point));
    doc.annotations.push(annotation);
    doc.graphics.push(graphic);
    let before = doc.annotations.clone();
    assert_eq!(hit_object(&doc, point, 0.), Some(1));
    assert_eq!(bond_text_at(&doc, point, 0.), Some(1));
    assert_eq!(doc.annotations, before);
}

#[test]
fn hidden_nearer_bond_does_not_mask_a_visible_bond_or_background_graphic() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(110., 50.));
    let b = doc.add_atom("C", World::new(170., 50.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", World::new(110., 52.));
    let d = doc.add_atom("C", World::new(170., 52.));
    doc.add_bond(c, d, 1, "plain");
    let point = World::new(140., 50.);
    doc.graphics.push(rectangle(10, point, -1));
    doc.graphics.push(table(vec![a, b]));
    assert_eq!(reshiki::editing::nearest_bond(&doc, point, 7.), Some(0));
    assert_eq!(hit_selection(&doc, point, 10.), vec![c, d]);
    doc.bonds.pop();
    assert_eq!(hit_object(&doc, point, 10.), Some(10));
    assert_eq!(hit_selection(&doc, point, 10.), vec![10]);
}

#[test]
fn bond_attachment_filters_hidden_atoms_before_distance_ranking() {
    let mut doc = Document::default();
    let point = World::new(140., 50.);
    let hidden = doc.add_atom("C", point);
    let visible = doc.add_atom("C", point.offset(2., 0.));
    doc.graphics.push(table(vec![hidden]));
    let drawing = BondDrawing {
        fixed_length: false,
        ..Default::default()
    };
    assert_eq!(
        bond_target_with(&doc, point.offset(-40., 0.), point, None, 8., drawing),
        (point.offset(2., 0.), Some(visible))
    );
    doc.atoms.retain(|a| a.id != visible);
    assert_eq!(
        bond_target_with(&doc, point.offset(-40., 0.), point, None, 8., drawing),
        (point, None)
    );
}

#[test]
fn a_hidden_atom_at_the_snapped_endpoint_cannot_capture_a_distant_pointer() {
    let mut doc = Document::default();
    let start = World::new(100., 50.);
    let snapped = World::new(140., 50.);
    let hidden = doc.add_atom("C", snapped);
    let visible = doc.add_atom("C", snapped.offset(2., 0.));
    doc.graphics.push(table(vec![hidden]));
    let drawing = BondDrawing {
        length: 40.,
        fixed_angles: false,
        ..Default::default()
    };
    assert_eq!(
        bond_target_with(&doc, start, World::new(200., 50.), None, 8., drawing),
        (snapped.offset(2., 0.), Some(visible))
    );
    doc.atoms.retain(|atom| atom.id != visible);
    assert_eq!(
        bond_target_with(&doc, start, World::new(200., 50.), None, 8., drawing),
        (snapped, None)
    );
}

#[test]
fn indicator_overflow_does_not_suppress_visible_underlying_objects() {
    let mut doc = Document::default();
    let atom = doc.add_atom("C", World::new(80., 50.));
    doc.atom_mut(atom).unwrap().display.number = Some(reshiki::atom_labels::Number {
        text: "1".into(),
        offset: Some(World::new(60., 0.)),
        style: reshiki::atom_labels::number_style(),
    });
    let point = reshiki::atom_labels::indicators(&doc)[0].center;
    assert!(point.x > 100.);
    doc.graphics.push(rectangle(10, point, -1));
    doc.graphics.push(table(vec![atom]));
    assert_eq!(hit_selection(&doc, point, 2.), vec![10]);
}

#[test]
fn owned_marks_are_clipped_before_background_graphics_are_considered() {
    let mut doc = Document::default();
    let atom = doc.add_atom("C", World::new(80., 50.));
    doc.atom_mut(atom)
        .unwrap()
        .marks
        .push(reshiki::scientific::AtomMark {
            id: Some(1),
            kind: reshiki::scientific::MarkKind::LonePair,
            offset: World::new(60., 0.),
            angle: 0.,
            size_pt: None,
        });
    let parts = reshiki::scientific::document_mark_parts(doc.atom(atom).unwrap(), &doc);
    let paths = reshiki::graphics::flattened(&parts[0].commands);
    let point = paths[0][0].lerp(paths[0][1], 0.5);
    assert!(point.x > 100.);
    doc.graphics.push(rectangle(10, point, -1));
    doc.graphics.push(table(vec![atom]));
    assert_eq!(hit_object(&doc, point, 2.), Some(10));
}

#[test]
fn table_border_ink_remains_hittable_outside_its_content_clip() {
    let mut doc = Document::default();
    let mut graphic = table(vec![]);
    graphic.table.as_mut().unwrap().cells[0]
        .borders
        .push(Border {
            side: Side::Right,
            width_pt: 2.,
            color: Color::Ink,
            pattern: Default::default(),
        });
    let point = World::new(100.2, 50.);
    assert!(
        !graphic.table.as_ref().unwrap().cells[0]
            .frame(&graphic)
            .unwrap()
            .contains(point)
    );
    doc.graphics.push(graphic);
    assert_eq!(hit_object(&doc, point, 1.), Some(1000));
}

#[test]
fn no_table_and_distant_candidate_checks_leave_the_ownership_index_unbuilt() {
    let doc = Document::default();
    let clips = CellClips::new(&doc);
    assert!(clips.allows(1, World::default()));
    assert!(clips.owners.get().is_none());

    let mut doc = Document::default();
    let atom = doc.add_atom("N", World::new(20., 20.));
    doc.graphics.push(table(vec![atom]));
    let clips = CellClips::new(&doc);
    assert_eq!(
        crate::canvas::hit::hit_object_in_cells(
            &doc,
            World::new(10_000., 10_000.),
            8.,
            false,
            &clips,
        ),
        None
    );
    assert!(clips.owners.get().is_none());
}

#[test]
fn ordinary_atom_bond_and_chain_presses_use_the_visible_source_candidate() {
    use crate::canvas::{Gesture, State, Tool};
    use iced::{Event, Point, Rectangle, Size, mouse, widget::canvas::Program};
    let mut doc = Document::default();
    let point = World::new(140., 50.);
    let hidden = doc.add_atom("C", point);
    let visible = doc.add_atom("C", point.offset(2., 0.));
    doc.graphics.push(table(vec![hidden]));
    let bounds = Rectangle::with_size(Size::new(500., 400.));
    for tool in [
        Tool::Atom,
        Tool::Bond(1),
        Tool::Chain(reshiki::chains::ChainMode::Straight),
    ] {
        let mut canvas =
            crate::canvas::tests::chain_canvas(&doc, reshiki::chains::ChainMode::Straight);
        canvas.tool = tool;
        let mut state = State::default();
        let screen = canvas.camera.screen(point, bounds);
        let _ = canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(Point::new(screen.x, screen.y)),
        );
        match state.gesture {
            Some(Gesture::Draw { id, .. }) => assert_eq!(id, Some(visible)),
            Some(Gesture::Chain { source, .. }) => assert_eq!(source, Some(visible)),
            other => panic!("Expected a drawing gesture, got {other:?}"),
        }
    }
}

#[test]
fn a_five_unit_pick_radius_does_not_admit_ink_one_unit_outside_the_cell() {
    use crate::canvas::hit::{nearest_atom, nearest_bond};
    let mut doc = Document::default();
    doc.drawing_style.line_width_pt = 0.1;
    let hidden = doc.add_atom("C", World::new(101., 50.));
    let end = doc.add_atom("C", World::new(101., 80.));
    doc.add_bond(hidden, end, 1, "plain");
    let visible = doc.add_atom("C", World::new(97., 50.));
    doc.graphics.push(table(vec![hidden, end]));
    let p = World::new(99., 50.);
    assert_eq!(nearest_atom(&doc, p, 5.), Some(visible));
    assert_eq!(nearest_bond(&doc, World::new(99., 65.), 5.), None);
    let lower_a = doc.add_atom("C", World::new(97., 20.));
    let lower_b = doc.add_atom("C", World::new(97., 80.));
    doc.add_bond(lower_a, lower_b, 1, "plain");
    assert_eq!(nearest_bond(&doc, World::new(99., 65.), 5.), Some(1));
    let mut graphic = rectangle(10, World::new(110., 50.), 2);
    graphic.origin = World::new(101., 40.);
    graphic.axis_x = World::new(9., 0.);
    graphic.axis_y = World::new(0., 20.);
    graphic.style.width_pt = 0.1;
    doc.graphics.push(graphic);
    doc.graphics[0].table.as_mut().unwrap().cells[0]
        .members
        .push(10);
    doc.graphics.push(rectangle(11, World::new(99., 65.), 1));
    assert_eq!(hit_object(&doc, World::new(99., 65.), 5.), Some(11));
}

#[test]
fn a_thick_affine_border_is_selectable_through_its_filled_band() {
    let mut doc = Document::default();
    let mut graphic = table(vec![]);
    graphic.axis_x = World::new(200., 80.);
    graphic.axis_y = World::new(30., 200.);
    graphic.table.as_mut().unwrap().cells[0]
        .borders
        .push(Border {
            side: Side::Top,
            width_pt: 12.,
            color: Color::Ink,
            pattern: Default::default(),
        });
    let point = Frame::of(&graphic).unwrap().point(World::new(0.25, 0.));
    let id = graphic.id;
    doc.graphics.push(graphic);
    assert_eq!(hit_object(&doc, point, 1.), Some(id));
}
