use super::*;
use crate::{
    document::Annotation,
    graphics::{
        Graphic, GraphicKind, GraphicStyle, PathCommand,
        table::{Border, Cell, Side, Table},
    },
    palette::Color,
};

fn table(cells: Vec<Cell>) -> Graphic {
    let mut g = Graphic::dragged(
        1000,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(200., 100.),
        Default::default(),
        Default::default(),
        false,
    );
    g.kind = GraphicKind::Table;
    g.table = Some(Box::new(Table { cells }));
    g
}
fn cell(id: u32, lo: f32, hi: f32, members: Vec<u64>) -> Cell {
    Cell {
        id,
        lo: Point::new(lo, 0.),
        hi: Point::new(hi, 1.),
        members,
        borders: vec![],
    }
}

#[test]
fn newly_placed_affine_text_uses_one_effective_layout_for_creation_scope_and_paint() {
    for (axis_x, axis_y) in [
        (Point::new(0., 600.), Point::new(-400., 0.)),
        (Point::new(-600., 0.), Point::new(0., 400.)),
        (Point::new(600., 0.), Point::new(5000., 1000.)),
    ] {
        let mut doc = Document::default();
        let mut g = table(vec![cell(37, 0., 1., vec![1])]);
        g.origin = Point::new(2000., 2000.);
        g.axis_x = axis_x;
        g.axis_y = axis_y;
        let frame = g.table.as_ref().unwrap().cells[0].frame(&g).unwrap();
        let a = crate::graphics::table::editor::new_text(
            1,
            frame,
            "Product".into(),
            Default::default(),
        )
        .unwrap();
        let layout = annotation_layout(&a, frame);
        doc.annotations.push(a.clone());
        doc.graphics.push(g);
        let scope = Layout::new(&doc);
        let effective = scope.annotation(&a);
        let scoped = crate::typography::cell_layout(&effective.text, &effective.format);
        assert_eq!((layout.width, layout.height), (scoped.width, scoped.height));
        let expected = layout
            .fragments
            .iter()
            .map(|f| {
                (
                    f.text.clone(),
                    a.position.offset(f.position.x, f.position.y),
                )
            })
            .collect::<Vec<_>>();
        let scene = super::super::primitives(&doc);
        let Primitive::Clip { primitives, .. } = &scene[0] else {
            panic!("Cell clip")
        };
        let painted = primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, position, .. } => Some((text.clone(), *position)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(painted, expected);
        assert_eq!(
            doc.annotations,
            [a],
            "Derived layout must not rewrite new cell text"
        );
    }
}

#[test]
fn picking_uses_affine_cell_clips_without_changing_raw_geometry() {
    for reflected in [false, true] {
        let mut doc = Document::default();
        let mut g = table(vec![cell(1, 0., 0.5, vec![1, 2])]);
        g.origin = Point::new(40., 30.);
        g.axis_x = Point::new(if reflected { -200. } else { 200. }, 60.);
        g.axis_y = Point::new(50., 150.);
        let frame = g.table.as_ref().unwrap().cells[0].frame(&g).unwrap();
        let a = doc.add_atom("C", frame.point(Point::new(0.2, 0.5)));
        let b = doc.add_atom("C", frame.point(Point::new(1.4, 0.5)));
        doc.add_bond(a, b, 1, "plain");
        doc.graphics.push(g);
        let before = doc.file_json().unwrap();
        let view = Layout::new(&doc);
        let outside = frame.point(Point::new(1.1, 0.5));
        assert!(!view.contains(a, outside));
        assert_eq!(doc.nearest(doc.atom(b).unwrap().position, 2.), None);
        let (_, end) = view
            .segment(
                a,
                doc.atom(a).unwrap().position,
                doc.atom(b).unwrap().position,
            )
            .unwrap();
        assert!(end.distance(frame.point(Point::new(1., 0.5))) < 0.001);
        assert_eq!(crate::editing::nearest_bond(&doc, outside, 2.), None);
        let enclosing = [
            Point::new(-1000., -1000.),
            Point::new(1000., -1000.),
            Point::new(1000., 1000.),
            Point::new(-1000., 1000.),
        ];
        assert!(!crate::selection_region::objects(&doc, &enclosing).contains(&b));
        assert_eq!(doc.file_json().unwrap(), before);
    }
}
fn pixels(drawing: &[Primitive], lo: Point, hi: Point) -> resvg::tiny_skia::Pixmap {
    let svg = super::super::svg_part(drawing, lo, hi);
    let options = resvg::usvg::Options {
        fontdb: crate::style::fontdb(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
    let mut pix =
        resvg::tiny_skia::Pixmap::new((hi.x - lo.x) as u32, (hi.y - lo.y) as u32).unwrap();
    resvg::render(&tree, Default::default(), &mut pix.as_mut());
    pix
}

#[test]
fn scene_keeps_owned_bonds_curves_and_full_wrapped_text_inside_one_clip() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(20., 70.));
    let b = doc.add_atom("C", Point::new(160., 70.));
    doc.add_bond(a, b, 1, "plain");
    let text = "complete cell text keeps every word after wrapping";
    doc.annotations.push(Annotation {
        id: 40,
        position: Point::new(5., 4.),
        text: text.into(),
        format: Default::default(),
        angle: 0.,
    });
    let mut curve = Graphic::pen_curve(
        50,
        Point::default(),
        Point::new(1., 1.),
        GraphicStyle {
            stroke: Color::Custom([220, 0, 0]),
            ..Default::default()
        },
    );
    curve.origin = Point::default();
    curve.axis_x = Point::new(1., 0.);
    curve.axis_y = Point::new(0., 1.);
    curve.path = vec![
        PathCommand::Move(Point::new(10., 80.)),
        PathCommand::Cubic(
            Point::new(50., -80.),
            Point::new(160., 180.),
            Point::new(180., 80.),
        ),
    ];
    let commands = curve.commands();
    doc.graphics.push(curve);
    doc.graphics.push(table(vec![
        cell(1, 0., 0.5, vec![a, b, 40, 50]),
        cell(2, 0.5, 1., vec![]),
    ]));
    doc.validate().unwrap();
    let before = doc.clone();
    let scene = super::super::primitives(&doc);
    assert_eq!(
        doc, before,
        "Derived cell layout must not edit authoritative text or geometry"
    );
    assert_eq!(
        scene.len(),
        2,
        "Owned objects must not also paint on the ordinary canvas"
    );
    let Primitive::Clip {
        corners,
        primitives,
    } = &scene[0]
    else {
        panic!("Missing cell clip")
    };
    assert_eq!(
        *corners,
        [
            Point::new(0., 0.),
            Point::new(100., 0.),
            Point::new(100., 100.),
            Point::new(0., 100.)
        ]
    );
    assert!(
        primitives
            .iter()
            .any(|p| matches!(p, Primitive::Line(_, end, _) if end.x > 100.)),
        "A crossing bond retains its full source geometry inside the clip"
    );
    assert!(
        primitives
            .iter()
            .any(|p| matches!(p, Primitive::Path { commands: got, .. } if *got == commands)),
        "The complete cubic remains a cubic in the common scene"
    );
    let fragments: Vec<_> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text { text, position, .. } => Some((text.as_str(), position.y)),
            _ => None,
        })
        .collect();
    assert_eq!(fragments.iter().map(|(s, _)| *s).collect::<String>(), text);
    assert!(
        fragments.windows(2).any(|w| w[0].1 != w[1].1),
        "Cell width must wrap the annotation"
    );
    let pix = pixels(&scene, Point::default(), Point::new(200., 100.));
    assert!(pix.pixels().iter().any(|p| p.alpha() > 0));
    for y in 0..100 {
        for x in 101..200 {
            assert_eq!(
                pix.pixel(x, y).unwrap().alpha(),
                0,
                "Cell ink leaked at {x},{y}"
            );
        }
    }
}

#[test]
fn affine_clip_rasterizes_the_actual_parallelogram_and_bounds_ignore_overflow() {
    let mut doc = Document::default();
    let curve = Graphic::dragged(
        5,
        GraphicKind::Rectangle,
        Point::new(-200., -200.),
        Point::new(500., 400.),
        GraphicStyle {
            fill: Some(Color::Custom([200, 20, 50])),
            ..Default::default()
        },
        Default::default(),
        false,
    );
    doc.graphics.push(curve);
    let mut g = table(vec![cell(1, 0., 1., vec![5])]);
    g.origin = Point::new(40., 20.);
    g.axis_x = Point::new(120., 30.);
    g.axis_y = Point::new(-20., 90.);
    let frame = g.table.as_ref().unwrap().cells[0].frame(&g).unwrap();
    doc.graphics.push(g);
    doc.validate().unwrap();
    let scene = super::super::primitives(&doc);
    let (lo, hi) = super::super::bounds(&scene);
    assert!(lo.x > 0. && lo.y > 0. && hi.x < 200. && hi.y < 180.);
    let pix = pixels(&scene, Point::default(), Point::new(220., 180.));
    let mut filled = 0;
    for y in 0..pix.height() {
        for x in 0..pix.width() {
            if pix.pixel(x, y).unwrap().alpha() < 32 {
                continue;
            }
            filled += 1;
            let p = frame
                .local(Point::new(x as f32 + 0.5, y as f32 + 0.5))
                .unwrap();
            assert!(
                (-0.01..=1.01).contains(&p.x) && (-0.01..=1.01).contains(&p.y),
                "Affine cell leaked at {x},{y}: {p:?}"
            );
        }
    }
    assert!(filled > 10_000);
}

#[test]
fn neighboring_borders_keep_native_cell_order_without_deduplicating_the_edge() {
    let mut left = cell(1, 0., 0.5, vec![]);
    left.borders.push(Border {
        side: Side::Right,
        width_pt: 4.,
        color: Color::Custom([255, 0, 0]),
        pattern: Default::default(),
    });
    let mut right = cell(2, 0.5, 1., vec![]);
    right.borders.push(Border {
        side: Side::Left,
        width_pt: 1.5,
        color: Color::Custom([0, 0, 255]),
        pattern: Default::default(),
    });
    let mut doc = Document::default();
    doc.graphics.push(table(vec![left, right]));
    doc.validate().unwrap();
    let scene = super::super::primitives(&doc);
    assert!(matches!(
        scene.as_slice(),
        [
            Primitive::Clip { .. },
            Primitive::Path { .. },
            Primitive::Clip { .. },
            Primitive::Path { .. }
        ]
    ));
    let pix = pixels(&scene, Point::default(), Point::new(200., 100.));
    assert_eq!(pix.pixel(100, 50).unwrap().demultiply().blue(), 255);
    assert_eq!(pix.pixel(104, 50).unwrap().demultiply().red(), 255);
    let svg = super::super::svg(&doc);
    assert_eq!(svg.matches("<clipPath ").count(), 2);
    assert!(svg.find("rgb(255,0,0)").unwrap() < svg.find("rgb(0,0,255)").unwrap());
}

#[test]
fn available_width_follows_cell_and_annotation_rotation_without_clamping_source() {
    let frame = Frame {
        origin: Point::new(60., 20.),
        axis_x: Point::new(0., 100.),
        axis_y: Point::new(-40., 0.),
    };
    let p = frame.point(Point::new(0.1, 0.2));
    assert!((available(frame, p, 90.).unwrap() - 90.).abs() < 0.001);
    assert_eq!(
        available(frame, frame.point(Point::new(1.2, 0.2)), 90.),
        None
    );
}

#[test]
fn narrow_native_cells_wrap_inside_a_word_without_losing_text_or_rich_spans() {
    use crate::typography::{TextFormat, TextSpan, TextStyle};
    let mut doc = Document::default();
    doc.annotations.push(Annotation {
        id: 1,
        position: Point::new(5., 5.),
        text: "Crosses".into(),
        angle: 0.,
        format: TextFormat {
            spans: vec![TextSpan {
                start: 0,
                end: 3,
                style: TextStyle {
                    color: Color::Custom([255, 0, 0]),
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
    });
    doc.graphics.push(table(vec![cell(1, 0., 0.2, vec![1])]));
    doc.validate().unwrap();
    let before = doc.clone();
    let scene = super::super::primitives(&doc);
    let Primitive::Clip { primitives, .. } = &scene[0] else {
        panic!("Missing cell clip")
    };
    let runs: Vec<_> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text {
                position,
                text,
                color,
                ..
            } => Some((position, text, color)),
            _ => None,
        })
        .collect();
    assert_eq!(
        runs.iter()
            .map(|(_, text, _)| text.as_str())
            .collect::<String>(),
        "Crosses"
    );
    assert!(
        runs.windows(2).filter(|w| w[0].0.y != w[1].0.y).count() >= 2,
        "A native narrow cell must split a word across multiple lines"
    );
    assert_eq!(
        runs.iter()
            .filter(|(_, _, color)| **color == [255, 0, 0])
            .map(|(_, text, _)| text.as_str())
            .collect::<String>(),
        "Cro"
    );
    assert_eq!(doc, before);
}

#[test]
fn transparent_cell_halo_knocks_out_previous_ink_only_inside_its_affine_clip() {
    let rect = |x0, y0, x1, y1| {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    };
    let drawing = vec![
        Primitive::Polygon(rect(90., 20., 110., 40.)),
        Primitive::Clip {
            corners: [
                Point::new(100., 0.),
                Point::new(200., 0.),
                Point::new(200., 100.),
                Point::new(100., 100.),
            ],
            primitives: vec![Primitive::Halo {
                radius: 0.,
                primitives: vec![Primitive::Polygon(rect(95., 25., 105., 35.))],
            }],
        },
    ];
    let pix = pixels(&drawing, Point::default(), Point::new(200., 100.));
    assert_eq!(
        pix.pixel(95, 30).unwrap().alpha(),
        255,
        "The halo must respect the cell's clipping boundary"
    );
    assert_eq!(
        pix.pixel(102, 30).unwrap().alpha(),
        0,
        "Transparent export must remove prior cells' ink under this cell's halo"
    );
    assert_eq!(pix.pixel(108, 30).unwrap().alpha(), 255);
}

#[test]
fn public_cell_annotation_layout_matches_native_hard_wrapped_scene_runs() {
    let mut doc = Document::default();
    doc.annotations.push(Annotation {
        id: 1,
        position: Point::new(5., 5.),
        text: "CompleteUnbrokenCaption".into(),
        angle: 0.,
        format: Default::default(),
    });
    let graphic = table(vec![cell(1, 0., 0.2, vec![1])]);
    let frame = graphic.table.as_ref().unwrap().cells[0]
        .frame(&graphic)
        .unwrap();
    let expected = super::super::table_annotation_layout(&doc.annotations[0], frame);
    doc.graphics.push(graphic);
    let before = doc.annotations.clone();
    let scene = super::super::primitives(&doc);
    let Primitive::Clip { primitives, .. } = &scene[0] else {
        panic!("Missing cell clip")
    };
    let actual: Vec<_> = primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text { position, text, .. } => Some((*position, text.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        actual,
        expected
            .fragments
            .iter()
            .map(|fragment| (
                doc.annotations[0]
                    .position
                    .offset(fragment.position.x, fragment.position.y),
                fragment.text.as_str(),
            ))
            .collect::<Vec<_>>()
    );
    assert!(
        expected
            .fragments
            .iter()
            .any(|fragment| fragment.position.y > 0.)
    );
    assert_eq!(doc.annotations, before);
}
