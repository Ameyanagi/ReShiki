use super::{ArcGeometry, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand, PathHeads};
use crate::{
    arrows::{ArrowStyle, Head, HeadShape, Preset},
    document::{Arrow, Document, Point},
    palette::Color,
};

fn path(commands: Vec<PathCommand>) -> Graphic {
    let mut graphic = Graphic::pen_curve(
        1,
        Point::default(),
        Point::new(120., 0.),
        Default::default(),
    );
    graphic.path = commands;
    graphic.node_types.clear();
    graphic
}

fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.0001, "{a:?} != {b:?}");
}

fn arrow(start: Point, command: &PathCommand, heads: PathHeads, style: &GraphicStyle) -> Arrow {
    let mut arrow = Arrow::new(
        1,
        start,
        command.iter_points().last().unwrap(),
        Preset::Forward,
        ArrowStyle {
            head: heads.head,
            tail: heads.tail,
            shape: heads.shape,
            head_length_pt: heads.head_length_pt,
            head_width_pt: heads.head_width_pt,
            head_notch: heads.head_notch,
            color: style.stroke,
            width_pt: style.width_pt,
            pattern: style.pattern,
            ..ArrowStyle::DEFAULT
        },
    );
    if let PathCommand::Cubic(a, b, _) = command {
        arrow.cubic = Some([*a, *b]);
    }
    arrow
}

#[test]
fn path_heads_defaults_and_native_wire_preserve_old_graphics() {
    let heads = PathHeads::default();
    assert_eq!((heads.head, heads.tail), (Head::None, Head::None));
    assert_eq!(heads.shape, ArrowStyle::DEFAULT.shape);
    assert_eq!(heads.head_length_pt, ArrowStyle::DEFAULT.head_length_pt);
    assert_eq!(heads.head_width_pt, ArrowStyle::DEFAULT.head_width_pt);
    assert_eq!(heads.head_notch, ArrowStyle::DEFAULT.head_notch);
    assert_eq!(serde_json::from_str::<PathHeads>("{}").unwrap(), heads);
    let mut graphic = path(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(120., 0.)),
    ]);
    let old = serde_json::to_value(&graphic).unwrap();
    assert!(old.get("heads").is_none());
    assert_eq!(serde_json::from_value::<Graphic>(old).unwrap(), graphic);
    graphic.heads = Some(PathHeads {
        head: Head::Left,
        tail: Head::Right,
        shape: HeadShape::Hollow,
        ..heads
    });
    let value = serde_json::to_value(&graphic).unwrap();
    assert_eq!(value["heads"]["head"], "left");
    assert_eq!(value["heads"]["tail"], "right");
    assert_eq!(value["heads"]["shape"], "hollow");
    assert_eq!(serde_json::from_value::<Graphic>(value).unwrap(), graphic);
    let doc = Document {
        graphics: vec![graphic],
        ..Default::default()
    };
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );

    let arc = Graphic::dragged(
        2,
        GraphicKind::Arc,
        Point::default(),
        Point::new(80., 60.),
        Default::default(),
        Default::default(),
        false,
    )
    .with_arc(ArcGeometry::default());
    assert!(serde_json::to_value(&arc).unwrap().get("heads").is_none());
    assert_eq!(
        serde_json::from_value::<Graphic>(serde_json::to_value(&arc).unwrap()).unwrap(),
        arc
    );
}

#[test]
fn path_heads_validate_arrow_limits_and_supported_kinds() {
    let mut graphic = path(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(120., 0.)),
    ]);
    for heads in [
        PathHeads {
            head_length_pt: 0.49,
            ..Default::default()
        },
        PathHeads {
            head_length_pt: 24.01,
            ..Default::default()
        },
        PathHeads {
            head_width_pt: 0.24,
            ..Default::default()
        },
        PathHeads {
            head_width_pt: 16.01,
            ..Default::default()
        },
        PathHeads {
            head_notch: -0.01,
            ..Default::default()
        },
        PathHeads {
            head_notch: 0.91,
            ..Default::default()
        },
        PathHeads {
            head_length_pt: f32::NAN,
            ..Default::default()
        },
        PathHeads {
            head_width_pt: f32::INFINITY,
            ..Default::default()
        },
        PathHeads {
            head_notch: f32::NEG_INFINITY,
            ..Default::default()
        },
    ] {
        assert!(heads.validate().is_err());
        graphic.heads = Some(heads);
        assert!(graphic.validate().is_err());
    }
    for (length, width, notch) in [(0.5, 0.25, 0.), (24., 16., 0.9)] {
        graphic.heads = Some(PathHeads {
            head_length_pt: length,
            head_width_pt: width,
            head_notch: notch,
            ..Default::default()
        });
        graphic.validate().unwrap();
    }
    graphic.heads = Some(Default::default());
    for kind in GraphicKind::DRAWABLE {
        graphic.kind = kind;
        assert_eq!(
            graphic.validate().is_ok(),
            matches!(kind, GraphicKind::Curve | GraphicKind::Path),
            "{kind:?}"
        );
    }
}

#[test]
fn straight_and_cubic_path_heads_share_arrow_geometry_and_trim_both_ends() {
    let start = Point::new(0., 0.);
    let end = Point::new(120., 0.);
    for command in [
        PathCommand::Line(end),
        PathCommand::Cubic(Point::new(40., -4.), Point::new(80., -4.), end),
        PathCommand::Cubic(Point::new(0., -80.), Point::new(120., 80.), end),
        PathCommand::Cubic(start, end, end),
    ] {
        for shape in HeadShape::ALL {
            for kind in Head::ALL {
                let mut graphic = path(vec![PathCommand::Move(start), command.clone()]);
                graphic.style.stroke = Color::Custom([20, 80, 170]);
                graphic.style.pattern = LinePattern::Dashed;
                let heads = PathHeads {
                    head: *kind,
                    tail: *kind,
                    shape: *shape,
                    ..Default::default()
                };
                graphic.heads = Some(heads);
                let expected = arrow(start, &command, heads, &graphic.style).paths();
                let parts = graphic.parts();
                assert_eq!(parts.len(), expected.len());
                for (part, expected) in parts.iter().zip(&expected) {
                    assert_eq!(part.commands, expected.commands);
                    assert_eq!(part.style, expected.style);
                    assert_eq!(part.filled, expected.filled);
                }
                assert_eq!(
                    graphic.commands(),
                    vec![PathCommand::Move(start), command.clone()]
                );
                if *kind != Head::None && *shape != HeadShape::Open {
                    assert!(
                        parts[0].commands[0]
                            .iter_points()
                            .last()
                            .unwrap()
                            .distance(start)
                            > 0.01
                    );
                    assert!(
                        parts[0].commands[1]
                            .iter_points()
                            .last()
                            .unwrap()
                            .distance(end)
                            > 0.01
                    );
                }
                if *kind != Head::None && *shape == HeadShape::Solid {
                    let shaft = &parts[0];
                    let caps = [
                        shaft.commands[1].iter_points().last().unwrap(),
                        shaft.commands[0].iter_points().last().unwrap(),
                    ];
                    for (cap, head) in caps.into_iter().zip(&parts[1..]) {
                        let polygon = &super::flattened(&head.commands)[0];
                        assert!(crate::selection_region::contains(polygon, cap));
                        for i in 0..64 {
                            let (sin, cos) = (i as f32 * std::f32::consts::TAU / 64.).sin_cos();
                            let radius = shaft.style.width() * 0.5;
                            assert!(crate::selection_region::contains(
                                polygon,
                                cap.offset(cos * radius, sin * radius)
                            ));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn open_heads_follow_straight_and_cubic_endpoint_tangents() {
    let start = Point::new(10., 20.);
    let end = Point::new(130., 20.);
    let middle = Point::new(70., 60.);
    for (commands, departure, arrival) in [
        (
            vec![PathCommand::Move(start), PathCommand::Line(end)],
            end,
            start,
        ),
        (
            vec![
                PathCommand::Move(start),
                PathCommand::Line(middle),
                PathCommand::Line(end),
            ],
            middle,
            middle,
        ),
        (
            vec![
                PathCommand::Move(start),
                PathCommand::Cubic(Point::new(50., 16.), Point::new(90., 16.), end),
            ],
            Point::new(50., 16.),
            Point::new(90., 16.),
        ),
        (
            vec![
                PathCommand::Move(start),
                PathCommand::Cubic(Point::new(10., -60.), Point::new(130., 100.), end),
            ],
            Point::new(10., -60.),
            Point::new(130., 100.),
        ),
    ] {
        let mut graphic = path(commands);
        let heads = PathHeads {
            head: Head::Full,
            tail: Head::Full,
            shape: HeadShape::Open,
            ..Default::default()
        };
        graphic.heads = Some(heads);
        let parts = graphic.parts();
        assert_eq!(parts.len(), 3);
        for head in &parts[1..] {
            let [
                PathCommand::Move(left),
                PathCommand::Line(actual_tip),
                PathCommand::Line(right),
            ] = head.commands.as_slice()
            else {
                panic!("Open full head");
            };
            let (tip, direction) = if actual_tip.distance(end) < 0.0001 {
                (end, Point::new(end.x - arrival.x, end.y - arrival.y))
            } else {
                (
                    start,
                    Point::new(start.x - departure.x, start.y - departure.y),
                )
            };
            near(*actual_tip, tip);
            let length = crate::style::DEFAULT.world(heads.head_length_pt);
            let magnitude = direction.distance(Point::default());
            near(
                left.lerp(*right, 0.5),
                tip.offset(
                    -direction.x / magnitude * length,
                    -direction.y / magnitude * length,
                ),
            );
        }
    }
}

#[test]
fn parametric_curve_heads_use_its_world_frame() {
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Curve,
        Point::new(17., -9.),
        Point::new(100., 80.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.axis_y = Point::new(-40., 20.);
    let heads = PathHeads {
        head: Head::Full,
        tail: Head::Left,
        ..Default::default()
    };
    graphic.heads = Some(heads);
    let commands = graphic.commands();
    let start = commands[0].iter_points().last().unwrap();
    let expected = arrow(start, &commands[1], heads, &graphic.style).paths();
    let parts = graphic.parts();
    assert_eq!(parts.len(), expected.len());
    for (part, expected) in parts.iter().zip(expected) {
        assert_eq!(part.commands, expected.commands);
    }
}

#[test]
fn multiple_segments_keep_one_shaft_and_only_trim_terminal_segments() {
    let start = Point::default();
    let first = PathCommand::Cubic(
        Point::new(15., -30.),
        Point::new(45., 25.),
        Point::new(60., 0.),
    );
    let middle = PathCommand::Line(Point::new(100., 40.));
    let last = PathCommand::Cubic(
        Point::new(120., 80.),
        Point::new(150., -40.),
        Point::new(180., 20.),
    );
    let mut graphic = path(vec![
        PathCommand::Move(start),
        first.clone(),
        middle.clone(),
        last.clone(),
    ]);
    let heads = PathHeads {
        head: Head::Left,
        tail: Head::Right,
        ..Default::default()
    };
    graphic.heads = Some(heads);
    let first_paths = arrow(
        start,
        &first,
        PathHeads {
            head: Head::None,
            ..heads
        },
        &graphic.style,
    )
    .paths();
    let last_paths = arrow(
        Point::new(100., 40.),
        &last,
        PathHeads {
            tail: Head::None,
            ..heads
        },
        &graphic.style,
    )
    .paths();
    let parts = graphic.parts();
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0].commands,
        vec![
            first_paths[0].commands[0].clone(),
            first_paths[0].commands[1].clone(),
            middle,
            last_paths[0].commands[1].clone()
        ]
    );
    assert_eq!(parts[1].commands, first_paths[1].commands);
    assert_eq!(parts[2].commands, last_paths[1].commands);
    assert_eq!(
        parts[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::Move(_)))
            .count(),
        1
    );
}

#[test]
fn closed_and_compound_paths_keep_heads_without_rendering_them() {
    for commands in [
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(120., 0.)),
            PathCommand::Line(Point::new(60., 80.)),
            PathCommand::Close,
        ],
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(120., 0.)),
            PathCommand::Move(Point::new(20., 60.)),
            PathCommand::Line(Point::new(80., 60.)),
        ],
    ] {
        let mut graphic = path(commands.clone());
        graphic.heads = Some(PathHeads {
            head: Head::Full,
            tail: Head::Full,
            ..Default::default()
        });
        graphic.validate().unwrap();
        let parts = graphic.parts();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].commands, commands);
        assert_eq!(
            serde_json::from_value::<Graphic>(serde_json::to_value(&graphic).unwrap()).unwrap(),
            graphic
        );
    }
}

#[test]
fn head_parts_expand_bounds_hit_testing_and_scene_primitives() {
    let mut graphic = path(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(120., 0.)),
    ]);
    let original = graphic.bounds();
    graphic.heads = Some(PathHeads {
        head: Head::Full,
        tail: Head::Full,
        head_width_pt: 12.,
        ..Default::default()
    });
    let parts = graphic.parts();
    let (lo, hi) = graphic.bounds();
    assert!(lo.y < original.0.y && hi.y > original.1.y);
    for point in parts
        .iter()
        .flat_map(|p| &p.commands)
        .flat_map(PathCommand::iter_points)
    {
        assert!(point.x >= lo.x && point.x <= hi.x && point.y >= lo.y && point.y <= hi.y);
    }
    let tip = parts[1].commands[0].iter_points().last().unwrap();
    let wing = parts[1].commands[1].iter_points().last().unwrap();
    assert!(graphic.hit(tip.lerp(wing, 0.7), 0.01));
    let doc = Document {
        graphics: vec![graphic],
        ..Default::default()
    };
    let drawing = crate::scene::primitives(&doc);
    assert_eq!(drawing.len(), parts.len());
    for (primitive, part) in drawing.iter().zip(parts) {
        let crate::scene::Primitive::Path {
            commands,
            style,
            filled,
        } = primitive
        else {
            panic!("Graphic path primitive");
        };
        assert_eq!(commands, &part.commands);
        assert_eq!(style, &part.style);
        assert_eq!(*filled, part.filled);
    }
}
