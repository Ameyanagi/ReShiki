use super::*;
use crate::{
    graphics::{
        Graphic, GraphicKind, GraphicStyle, LinePattern,
        table::{Cell, Table},
    },
    palette::Color,
};

fn rectangle(lo: Point, hi: Point, pattern: LinePattern) -> Graphic {
    Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        lo,
        hi,
        GraphicStyle {
            pattern,
            ..Default::default()
        },
        Default::default(),
        false,
    )
}

fn path(commands: Vec<PathCommand>, filled: bool, width: f32) -> Graphic {
    let mut g = Graphic::pen_curve(
        1,
        Point::default(),
        Point::new(1., 1.),
        GraphicStyle {
            fill: filled.then_some(Color::Ink),
            width_pt: width,
            ..Default::default()
        },
    );
    g.reset_frame();
    g.path = commands;
    g.node_types.clear();
    g
}

fn scoped(g: Graphic, frame: Frame) -> Document {
    let mut table = rectangle(Point::default(), Point::new(200., 100.), LinePattern::Solid);
    table.id = 1000;
    table.kind = GraphicKind::Table;
    table.table = Some(Box::new(Table {
        cells: [vec![1], vec![]]
            .into_iter()
            .enumerate()
            .map(|(i, members)| Cell {
                id: i as u32 + 1,
                lo: Point::new(i as f32 * 0.5, 0.),
                hi: Point::new((i + 1) as f32 * 0.5, 1.),
                members,
                borders: vec![],
            })
            .collect(),
    }));
    let mut doc = Document {
        graphics: vec![g, table],
        ..Default::default()
    };
    for g in &mut doc.graphics {
        g.map_positions(|p| frame.point(Point::new(p.x / 100., p.y / 100.)));
    }
    doc.validate().unwrap();
    doc
}

fn check(g: Graphic, selected: bool) {
    for (origin, axis_x, axis_y) in [
        (Point::default(), Point::new(100., 0.), Point::new(0., 100.)),
        (
            Point::new(40., 30.),
            Point::new(100., 30.),
            Point::new(35., 150.),
        ),
        (
            Point::new(40., 30.),
            Point::new(-100., -30.),
            Point::new(35., 150.),
        ),
    ] {
        let frame = Frame {
            origin,
            axis_x,
            axis_y,
        };
        let doc = scoped(g.clone(), frame);
        let before = doc.clone();
        let ids = objects(&doc, &frame.corners());
        assert_eq!(ids.contains(&1), selected, "{origin:?}/{axis_x:?}: {g:?}");
        assert!(
            !ids.contains(&1000),
            "One cell must not select the whole table"
        );
        let whole = [
            Point::new(-30., -30.),
            Point::new(230., -30.),
            Point::new(230., 130.),
            Point::new(-30., 130.),
        ]
        .map(|p| frame.point(Point::new(p.x / 100., p.y / 100.)));
        assert!(objects(&doc, &whole).contains(&1000));
        assert_eq!(doc, before);
    }
}

#[test]
fn unfilled_closed_outlines_do_not_create_visible_cell_footprints() {
    for pattern in [LinePattern::Solid, LinePattern::Dashed, LinePattern::Wavy] {
        check(
            rectangle(Point::new(-20., -20.), Point::new(120., 120.), pattern),
            false,
        );
        check(
            rectangle(Point::new(20., 20.), Point::new(80., 80.), pattern),
            true,
        );
        check(
            rectangle(Point::new(80., 20.), Point::new(120., 80.), pattern),
            true,
        );
    }
    let mut filled = rectangle(
        Point::new(-20., -20.),
        Point::new(120., 120.),
        LinePattern::Solid,
    );
    filled.style.fill = Some(Color::Ink);
    check(filled, true);
}

#[test]
fn open_paths_select_only_visible_segments_and_stroke_width() {
    use PathCommand::{Line, Move};
    check(
        path(
            vec![Move(Point::new(120., 20.)), Line(Point::new(120., 80.))],
            false,
            0.6,
        ),
        false,
    );
    check(
        path(
            vec![Move(Point::new(20., 40.)), Line(Point::new(120., 40.))],
            false,
            0.6,
        ),
        true,
    );
    // The centreline is outside; its physical stroke width still paints a
    // narrow strip inside the cell. This is legitimate visible content.
    check(
        path(
            vec![Move(Point::new(100.1, 20.)), Line(Point::new(100.1, 80.))],
            false,
            0.6,
        ),
        true,
    );
}

#[test]
fn filled_compound_holes_and_retraced_bridges_do_not_invent_cell_ink() {
    use PathCommand::{Close, Line, Move};
    let mut hole = vec![
        Move(Point::new(-20., -20.)),
        Line(Point::new(120., -20.)),
        Line(Point::new(120., 120.)),
        Line(Point::new(-20., 120.)),
        Close,
        Move(Point::new(-10., -10.)),
        Line(Point::new(-10., 110.)),
        Line(Point::new(110., 110.)),
        Line(Point::new(110., -10.)),
        Close,
    ];
    check(path(hole.clone(), true, 0.), false);
    check(path(hole.clone(), true, 0.6), false);
    hole.truncate(5);
    check(path(hole, true, 0.), true);
    let bridge = vec![
        Move(Point::new(-40., 30.)),
        Line(Point::new(-20., 30.)),
        Line(Point::new(-20., 70.)),
        Line(Point::new(-40., 70.)),
        Line(Point::new(-40., 30.)),
        Line(Point::new(120., 30.)),
        Line(Point::new(140., 30.)),
        Line(Point::new(140., 70.)),
        Line(Point::new(120., 70.)),
        Line(Point::new(120., 30.)),
        Line(Point::new(-40., 30.)),
        Close,
    ];
    check(path(bridge.clone(), true, 0.), false);
    check(path(bridge, true, 0.6), true);
}

#[test]
fn ordinary_outline_enclosure_keeps_its_existing_region_behavior() {
    let doc = Document {
        graphics: vec![rectangle(
            Point::new(-20., -20.),
            Point::new(120., 120.),
            LinePattern::Solid,
        )],
        ..Default::default()
    };
    let frame = Frame {
        origin: Point::default(),
        axis_x: Point::new(100., 0.),
        axis_y: Point::new(0., 100.),
    };
    assert!(objects(&doc, &frame.corners()).is_empty());
    let whole = [
        Point::new(-30., -30.),
        Point::new(130., -30.),
        Point::new(130., 130.),
        Point::new(-30., 130.),
    ];
    assert_eq!(objects(&doc, &whole), vec![1]);
}
