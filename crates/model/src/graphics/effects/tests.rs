use super::*;
use crate::graphics::{GraphicStyle, LinePattern, PathHeads};

fn shape(kind: GraphicKind, effects: Effects) -> Graphic {
    Graphic::dragged(
        1,
        kind,
        Point::new(0., 0.),
        Point::new(40., 20.),
        GraphicStyle {
            fill: Some(crate::palette::Color::Ink),
            effects,
            ..Default::default()
        },
        Default::default(),
        false,
    )
}

#[test]
fn a_wavy_line_is_half_circles_that_end_on_the_line() {
    let commands = vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Line(Point::new(20., 0.)),
    ];
    let waved = wavy(&commands, 2.5, [0.; 2]);
    // Four half-waves of two quarter arcs each.
    assert_eq!(waved.len(), 1 + 4 * 2);
    let PathCommand::Cubic(_, _, end) = waved[waved.len() - 1] else {
        panic!("the wave ends with a curve");
    };
    assert_eq!(end, Point::new(20., 0.));
    let tops: Vec<f32> = waved
        .iter()
        .skip(1)
        .step_by(2)
        .map(|c| match c {
            PathCommand::Cubic(_, _, top) => top.y,
            _ => panic!("quarter arcs only"),
        })
        .collect();
    assert_eq!(tops, [2.5, -2.5, 2.5, -2.5]);
}

#[test]
fn a_closed_wave_keeps_an_even_count_and_closes() {
    let square = shape(GraphicKind::Rectangle, Effects::default()).commands();
    let waved = wavy(&square, 7., [0.; 2]);
    assert_eq!(waved.last(), Some(&PathCommand::Close));
    let arcs = waved
        .iter()
        .filter(|c| matches!(c, PathCommand::Cubic(..)))
        .count();
    assert_eq!(arcs % 4, 0, "{arcs} quarter arcs");
}

#[test]
fn wavy_lines_wave_their_shaft_and_keep_straight_heads() {
    let mut line = shape(GraphicKind::Line, Effects::default());
    line.axis_x = Point::new(200., 0.);
    line.style.fill = None;
    line.style.pattern = LinePattern::Wavy;
    line.heads = Some(PathHeads {
        head: crate::arrows::Head::Full,
        ..Default::default()
    });
    line.validate().expect("lines take arrowheads");
    let parts = line.parts();
    assert_eq!(parts.len(), 2, "a shaft and one head");
    assert!(parts[0].commands.len() > 4, "the shaft waves");
    assert!(parts[1].filled, "the head stays a solid triangle");
    // The wave runs straight into the head instead of curling under it.
    let shaft = &parts[0].commands;
    assert!(matches!(shaft[shaft.len() - 1], PathCommand::Line(_)));
    assert!(
        matches!(shaft[1], PathCommand::Cubic(..)),
        "the tail end waves"
    );
}

#[test]
fn shading_runs_from_the_color_at_the_edge_to_white_at_the_highlight() {
    assert_eq!(shade([0, 0, 255], 0.), [255, 255, 255]);
    assert_eq!(shade([0, 0, 255], 1.), [0, 0, 255]);
    assert_eq!(shade([0; 3], 0.5), [210; 3]);
}

#[test]
fn rectangle_bands_shrink_into_the_top_left_corner() {
    let rectangle = shape(
        GraphicKind::Rectangle,
        Effects {
            shaded: true,
            shadow_pt: None,
        },
    );
    let bands = rectangle.shading_bands();
    let n = bands.len();
    // 40 × 20 drawing units is about 14 pt across: three bands per point.
    assert_eq!(n, 42);
    let extent = |band: &[Point]| {
        band.iter().fold(Point::new(0., 0.), |m, p| {
            Point::new(m.x.max(p.x), m.y.max(p.y))
        })
    };
    assert_eq!(extent(&bands[0].0), Point::new(40., 20.));
    let last = extent(&bands[n - 1].0);
    assert!(last.x < 40. * 0.1 && last.y < 20. * 0.1, "{last:?}");
    assert!(bands.windows(2).all(|pair| pair[0].1 > pair[1].1));
    for (band, _) in &bands {
        assert!(
            band.iter()
                .all(|p| (-1e-3..=40.001).contains(&p.x) && (-1e-3..=20.001).contains(&p.y))
        );
    }
}

#[test]
fn oval_bands_stay_inside_the_oval() {
    let oval = shape(
        GraphicKind::Ellipse,
        Effects {
            shaded: true,
            shadow_pt: None,
        },
    );
    for (band, _) in oval.shading_bands() {
        for p in band {
            let (u, v) = ((p.x - 20.) / 20., (p.y - 10.) / 10.);
            assert!(u * u + v * v <= 1.01, "{p:?}");
        }
    }
}

#[test]
fn shadows_follow_closed_shapes_only() {
    let effects = Effects {
        shaded: false,
        shadow_pt: Some(4.),
    };
    let rectangle = shape(GraphicKind::Rectangle, effects);
    let d = crate::style::DEFAULT.world(4.);
    let shadow = rectangle.shadow_commands().expect("a closed shape");
    assert_eq!(shadow[0], PathCommand::Move(Point::new(d, d)));
    assert!(
        shape(GraphicKind::Line, effects)
            .shadow_commands()
            .is_none()
    );
    assert!(
        shape(
            GraphicKind::Line,
            Effects {
                shaded: true,
                shadow_pt: None
            }
        )
        .shading_bands()
        .is_empty()
    );
}

#[test]
fn plain_styles_save_as_before_and_effects_round_trip() {
    let plain = serde_json::to_value(GraphicStyle::default()).unwrap();
    assert!(plain.get("effects").is_none(), "{plain}");
    let style = GraphicStyle {
        pattern: LinePattern::Wavy,
        effects: Effects {
            shaded: true,
            shadow_pt: Some(2.),
        },
        ..Default::default()
    };
    let json = serde_json::to_value(&style).unwrap();
    assert_eq!(
        json["effects"],
        serde_json::json!({"shaded": true, "shadow_pt": 2.0})
    );
    assert_eq!(json["pattern"], "wavy");
    assert_eq!(serde_json::from_value::<GraphicStyle>(json).unwrap(), style);
    assert!(
        Effects {
            shaded: false,
            shadow_pt: Some(0.)
        }
        .validate()
        .is_err()
    );
}

#[test]
fn effects_wavy_strokes_and_line_heads_need_version_40() {
    use crate::document::{DRAWING_ELEMENTS, Document};
    let mut doc = Document::default();
    doc.graphics
        .push(shape(GraphicKind::Rectangle, Effects::default()));
    assert!(doc.file_version() < DRAWING_ELEMENTS);
    for change in [
        |g: &mut Graphic| g.style.effects.shaded = true,
        |g: &mut Graphic| g.style.effects.shadow_pt = Some(4.),
        |g: &mut Graphic| g.style.pattern = LinePattern::Wavy,
        |g: &mut Graphic| {
            g.kind = GraphicKind::Line;
            g.heads = Some(PathHeads::default());
        },
    ] {
        let mut changed = doc.clone();
        change(&mut changed.graphics[0]);
        assert_eq!(changed.file_version(), DRAWING_ELEMENTS);
    }
}

#[test]
fn the_scene_paints_shadow_then_bands_then_the_outline_and_counts_the_shadow_in_bounds() {
    use crate::{document::Document, scene::Primitive};
    let effects = Effects {
        shaded: true,
        shadow_pt: Some(4.),
    };
    let doc = Document {
        graphics: vec![shape(GraphicKind::Ellipse, effects)],
        ..Default::default()
    };
    let drawing = crate::scene::primitives(&doc);
    let fills: Vec<_> = drawing
        .iter()
        .filter_map(|p| match p {
            Primitive::Path { style, filled, .. } => Some((style.fill.map(|c| c.rgb()), *filled)),
            _ => None,
        })
        .collect();
    assert_eq!(fills[0], (Some(SHADOW_RGB), true));
    let n = doc.graphics[0].shading_bands().len();
    assert!(n >= MIN_BANDS);
    assert_eq!(fills.len(), 1 + n + 1);
    assert_eq!(fills[1].0, Some(shade([0; 3], (n as f32 - 0.5) / n as f32)));
    assert_eq!(fills[n].0, Some(shade([0; 3], 0.5 / n as f32)));
    assert!(!fills[n + 1].1, "the outline strokes over the bands");
    let d = crate::style::DEFAULT.world(4.);
    let (_, hi) = crate::scene::bounds(&drawing);
    assert!(hi.x >= 40. + d - 0.01 && hi.y >= 20. + d - 0.01, "{hi:?}");
}

#[test]
fn a_shadowed_outline_is_paper_inside() {
    use crate::{document::Document, scene::Primitive};
    let mut rectangle = shape(
        GraphicKind::Rectangle,
        Effects {
            shaded: false,
            shadow_pt: Some(4.),
        },
    );
    rectangle.style.fill = None;
    let doc = Document {
        graphics: vec![rectangle],
        ..Default::default()
    };
    let paths: Vec<_> = crate::scene::primitives(&doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Path { style, filled, .. } => Some((style.fill.map(|c| c.rgb()), filled)),
            _ => None,
        })
        .collect();
    assert_eq!(paths, [(Some(SHADOW_RGB), true), (Some([255; 3]), true)]);
}

#[test]
fn a_shaded_path_shrinks_toward_its_highlight_and_stays_inside() {
    let mut path = shape(
        GraphicKind::Curve,
        Effects {
            shaded: true,
            shadow_pt: None,
        },
    );
    path.kind = GraphicKind::Path;
    path.style.effects.shaded = true;
    path.reset_frame();
    path.path = vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Line(Point::new(40., 0.)),
        PathCommand::Line(Point::new(0., 40.)),
        PathCommand::Close,
    ];
    let bands = path.shading_bands();
    let n = bands.len();
    assert!(n >= MIN_BANDS);
    for (band, _) in &bands {
        assert!(
            band.iter()
                .all(|p| p.x >= -1e-3 && p.y >= -1e-3 && p.x + p.y <= 40.001)
        );
    }
    // The innermost band sits at the highlight, 30% across and 25% down.
    let inner = &bands[n - 1].0;
    let (sx, sy) = inner.iter().fold((0., 0.), |(x, y), p| (x + p.x, y + p.y));
    let n = inner.len() as f32;
    assert!((sx / n - 12.).abs() < 1.5 && (sy / n - 10.).abs() < 1.5);
}

#[test]
fn only_closed_shapes_take_effects_from_the_drawing_style() {
    let effects = Effects {
        shaded: true,
        shadow_pt: Some(4.),
    };
    assert_eq!(shape(GraphicKind::Ellipse, effects).style.effects, effects);
    for kind in [GraphicKind::Line, GraphicKind::Curve, GraphicKind::Brackets] {
        assert!(shape(kind, effects).style.effects.is_plain(), "{kind:?}");
    }
    // Clicks and snapping use the shape, not its shadow.
    let rectangle = shape(GraphicKind::Rectangle, effects);
    let d = crate::style::DEFAULT.world(4.);
    assert!(!rectangle.hit(Point::new(40. + d * 0.8, 20. + d * 0.8), 0.5));
}

#[test]
fn a_tilted_shape_casts_its_shadow_in_its_own_frame() {
    // ChemDraw's tilted oval: major axis down-right, minor axis down-left.
    let mut oval = shape(
        GraphicKind::Ellipse,
        Effects {
            shaded: false,
            shadow_pt: Some(4.),
        },
    );
    oval.axis_x = Point::new(25., 43.3);
    oval.axis_y = Point::new(-17.32, 10.);
    let d = crate::style::DEFAULT.world(4.);
    let moved = |c: &PathCommand| match c {
        PathCommand::Move(p) => *p,
        _ => panic!("starts with a move"),
    };
    let shadow = oval.shadow_commands().unwrap();
    let (a, b) = (moved(&oval.commands()[0]), moved(&shadow[0]));
    // Down and to the left, as ChemDraw draws it.
    assert!((b.x - a.x - d * (0.5 - 0.866)).abs() < 0.01, "{a:?} {b:?}");
    assert!((b.y - a.y - d * (0.866 + 0.5)).abs() < 0.01, "{a:?} {b:?}");
}

#[test]
fn a_corner_radius_rounds_only_rounded_rectangles_and_needs_version_40() {
    let mut rounded = shape(GraphicKind::RoundedRectangle, Effects::default());
    assert!(
        (rounded.corner_radius() - 20. * 0.18).abs() < 1e-4,
        "18% by default"
    );
    rounded.style.corner_pt = Some(2.);
    assert!((rounded.corner_radius() - crate::style::DEFAULT.world(2.)).abs() < 1e-4);
    rounded.style.corner_pt = Some(72.);
    assert_eq!(
        rounded.corner_radius(),
        10.,
        "at most half the shorter side"
    );
    let mut style = rounded.style.clone();
    style.corner_pt = Some(2.);
    let plain = Graphic::dragged(
        2,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(10., 10.),
        style,
        Default::default(),
        false,
    );
    assert_eq!(plain.style.corner_pt, None);
    let doc = crate::document::Document {
        graphics: vec![rounded],
        ..Default::default()
    };
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
}

#[test]
fn waves_budget_short_headed_lines_and_symbols() {
    // Many long subpaths share one wave budget.
    let mut commands = vec![];
    for i in 0..100 {
        let y = i as f32 * 10.;
        commands.push(PathCommand::Move(Point::new(0., y)));
        commands.push(PathCommand::Line(Point::new(1_000_000., y)));
    }
    let waved = wavy(&commands, 1., [0.; 2]);
    assert!(
        waved.len() <= 2 * MAX_WAVES as usize + 400,
        "{}",
        waved.len()
    );
    // Too short to wave beside its head: straight under it.
    let short = [
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Line(Point::new(10., 0.)),
    ];
    assert_eq!(wavy(&short, 2., [0., 6.]).len(), 2);
    let style = GraphicStyle {
        pattern: LinePattern::Wavy,
        ..Default::default()
    };
    let symbol = Graphic::dragged(
        1,
        GraphicKind::Symbol(crate::scientific::SymbolKind::Plus),
        Point::default(),
        Point::new(10., 0.),
        style,
        Default::default(),
        false,
    );
    assert_eq!(symbol.style.pattern, LinePattern::Solid);
}

#[test]
fn a_small_wavy_outline_still_encloses_its_area() {
    // 32 pt round, waves of 7.2 pt radius: two would retrace each other.
    let w = crate::style::DEFAULT.world(10.);
    let h = crate::style::DEFAULT.world(6.);
    let square = vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Line(Point::new(w, 0.)),
        PathCommand::Line(Point::new(w, h)),
        PathCommand::Line(Point::new(0., h)),
        PathCommand::Close,
    ];
    let waved = wavy(&square, wave_radius(4.), [0.; 2]);
    let outline = flattened(&waved).concat();
    let area: f32 = outline
        .iter()
        .zip(outline.iter().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
        .abs()
        / 2.;
    assert!(area > w * h * 0.5, "{area} of {}", w * h);
}
