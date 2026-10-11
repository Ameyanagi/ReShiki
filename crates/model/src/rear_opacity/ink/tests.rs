use super::*;
use crate::palette::Color;
use resvg::tiny_skia::Pixmap;

fn rectangle(lo: Point, hi: Point, clockwise: bool) -> Vec<PathCommand> {
    let mut points = vec![lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    if !clockwise {
        points.reverse();
    }
    let mut commands = vec![PathCommand::Move(points[0])];
    commands.extend(points.into_iter().skip(1).map(PathCommand::Line));
    commands.push(PathCommand::Close);
    commands
}

fn path(commands: Vec<PathCommand>, width: f32, filled: bool) -> Primitive {
    Primitive::Path {
        commands,
        style: GraphicStyle {
            fill: Some(Color::Ink),
            width_pt: width * crate::style::DEFAULT.points_per_world(),
            ..Default::default()
        },
        filled,
    }
}

// Fill and stroke receive alpha independently, matching the two paint passes
// whose overlap single_paint replaces with a single filled silhouette.
fn raster(primitive: &Primitive, alpha: f32) -> Pixmap {
    let Primitive::Path {
        commands,
        style,
        filled,
    } = primitive
    else {
        panic!("Expected a path primitive");
    };
    let mut data = String::new();
    for command in commands {
        match command {
            PathCommand::Move(p) => data.push_str(&format!("M{} {} ", p.x, p.y)),
            PathCommand::Line(p) => data.push_str(&format!("L{} {} ", p.x, p.y)),
            PathCommand::Cubic(a, b, z) => data.push_str(&format!(
                "C{} {} {} {} {} {} ",
                a.x, a.y, b.x, b.y, z.x, z.y
            )),
            PathCommand::Close => data.push_str("Z "),
        }
    }
    let rgb = |color: Color| {
        let [r, g, b] = color.rgb();
        format!("rgb({r},{g},{b})")
    };
    let fill = if *filled {
        style.fill.map(rgb).unwrap_or_else(|| "none".into())
    } else {
        "none".into()
    };
    let stroke = rgb(style.stroke);
    let width = style.width();
    let dashes = style
        .dashes()
        .iter()
        .map(f32::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let header = "<svg xmlns='http://www.w3.org/2000/svg' width='144' height='144'>";
    let paints =
        format!("fill='{fill}' fill-opacity='{alpha}' stroke='{stroke}' stroke-opacity='{alpha}'");
    let geometry = format!("stroke-width='{width}' stroke-dasharray='{dashes}'");
    let ends = "stroke-linecap='round' stroke-linejoin='round'";
    let svg = format!("{header}<path d='{data}' {paints} {geometry} {ends}/></svg>");
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
    let mut image = Pixmap::new(144, 144).unwrap();
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut image.as_mut());
    image
}

fn alpha_at(image: &Pixmap, x: u32, y: u32) -> u8 {
    image.pixel(x, y).unwrap().alpha()
}

fn assert_alpha(image: &Pixmap, x: u32, y: u32, expected: u8) {
    let actual = alpha_at(image, x, y);
    assert!(
        actual.abs_diff(expected) <= 1,
        "At ({x}, {y}): alpha {actual}, expected {expected}"
    );
}

fn assert_single_fill(primitive: &Primitive) {
    let Primitive::Path { style, filled, .. } = primitive else {
        panic!("Expected a path primitive");
    };
    assert!(*filled);
    assert_eq!(style.fill, Some(style.stroke));
    assert_eq!(style.width_pt, 0.);
    assert_eq!(style.pattern, LinePattern::Solid);
}

#[test]
fn filled_stroke_blends_once_and_keeps_both_windings() {
    let samples = [
        (12, 40, false),
        (18, 40, true),
        (22, 40, true),
        (40, 40, true),
    ];
    for (alpha, ink, overlap) in [(1., 255, 255), (0.5, 128, 192)] {
        let mut images = Vec::new();
        for clockwise in [true, false] {
            let original = path(
                rectangle(Point::new(20., 20.), Point::new(60., 60.), clockwise),
                8.,
                true,
            );
            let merged = single_paint(original.clone());
            assert_single_fill(&merged);
            let image = raster(&merged, alpha);
            let reference = raster(&original, alpha);
            for (x, y, painted) in samples {
                assert_alpha(&image, x, y, if painted { ink } else { 0 });
                if alpha == 1. {
                    assert_eq!(alpha_at(&image, x, y), alpha_at(&reference, x, y));
                }
            }
            assert_alpha(&reference, 22, 40, overlap);
            assert_alpha(&reference, 18, 40, ink);
            assert_alpha(&reference, 40, 40, ink);
            images.push(image);
        }
        for (x, y, _) in samples {
            assert_eq!(alpha_at(&images[0], x, y), alpha_at(&images[1], x, y));
        }
    }
}

#[test]
fn opposite_winding_hole_keeps_clear_center_and_once_painted_edges() {
    let samples = [
        (12, 60, false),
        (18, 60, true),
        (22, 60, true),
        (30, 60, true),
        (38, 60, true),
        (42, 60, true),
        (60, 60, false),
        (78, 60, true),
        (82, 60, true),
    ];
    for (alpha, ink, overlap) in [(1., 255, 255), (0.5, 128, 192)] {
        for clockwise in [true, false] {
            let mut commands = rectangle(Point::new(20., 20.), Point::new(100., 100.), clockwise);
            commands.extend(rectangle(
                Point::new(40., 40.),
                Point::new(80., 80.),
                !clockwise,
            ));
            let original = path(commands, 8., true);
            let merged = single_paint(original.clone());
            assert_single_fill(&merged);
            let image = raster(&merged, alpha);
            let reference = raster(&original, alpha);
            for (x, y, painted) in samples {
                assert_alpha(&image, x, y, if painted { ink } else { 0 });
                if alpha == 1. {
                    assert_eq!(alpha_at(&image, x, y), alpha_at(&reference, x, y));
                }
            }
            assert_alpha(&reference, 38, 60, overlap);
            assert_alpha(&reference, 42, 60, ink);
            assert_alpha(&reference, 60, 60, 0);
        }
    }
}

#[test]
fn disjoint_opposite_winding_contours_cancel_the_smaller_inner_stroke_band() {
    for (alpha, ink, overlap) in [(1., 255, 255), (0.5, 128, 192)] {
        for clockwise in [true, false] {
            let mut commands = rectangle(Point::new(20., 20.), Point::new(60., 60.), clockwise);
            commands.extend(rectangle(
                Point::new(80., 25.),
                Point::new(110., 55.),
                !clockwise,
            ));
            let original = path(commands, 8., true);
            let merged = single_paint(original.clone());
            let image = raster(&merged, alpha);
            let reference = raster(&original, alpha);
            assert_alpha(&image, 22, 40, ink);
            assert_alpha(&image, 70, 40, 0);
            assert_alpha(&image, 78, 40, ink);
            assert_alpha(&image, 95, 40, ink);
            // Current behavior: all stroke contours follow the largest fill's
            // winding, cancelling the smaller opposite-winding fill where its
            // interior overlaps the stroke. The original paints those bands.
            for x in [82, 108] {
                assert_alpha(&image, x, 40, 0);
                assert_alpha(&reference, x, 40, overlap);
            }
        }
    }
}

#[test]
fn dashed_open_stroke_currently_fills_dash_gaps_but_keeps_end_caps() {
    let start = Point::new(20., 20.);
    let end = Point::new(120., 20.);
    let mut original = path(
        vec![PathCommand::Move(start), PathCommand::Line(end)],
        4.,
        false,
    );
    let Primitive::Path { style, .. } = &mut original else {
        unreachable!();
    };
    style.pattern = LinePattern::Dashed;
    let [dash, gap]: [f32; 2] = style.dashes().try_into().unwrap();
    let width = style.width();
    let pixel = |x: f32, y: f32| (x.floor() as u32, y.floor() as u32);
    let dash_samples = [0., 1., 2.].map(|i| pixel(start.x + i * (dash + gap) + dash / 2., start.y));
    let gap_samples =
        [0., 1., 2.].map(|i| pixel(start.x + i * (dash + gap) + dash + gap / 2., start.y));
    let cap_samples = [pixel(start.x - width / 4., start.y), pixel(end.x, end.y)];
    let blank_samples = [
        pixel(start.x - width, start.y),
        pixel(end.x + width, end.y),
        pixel(start.x + dash / 2., start.y + width),
    ];
    let merged = single_paint(original.clone());
    assert_single_fill(&merged);
    for (alpha, ink) in [(1., 255), (0.5, 128)] {
        let image = raster(&merged, alpha);
        let reference = raster(&original, alpha);
        for (x, y) in dash_samples.into_iter().chain(cap_samples) {
            assert_alpha(&image, x, y, ink);
            assert_alpha(&reference, x, y, ink);
        }
        // Current behavior: Path::stroke ignores Stroke::dash, so conversion
        // paints the entire open line and clears the dash pattern. The reference
        // preserves the gaps; keep that difference pinned at each gap's center.
        for (x, y) in gap_samples {
            assert_alpha(&image, x, y, ink);
            assert_alpha(&reference, x, y, 0);
        }
        for (x, y) in blank_samples {
            assert_alpha(&image, x, y, 0);
            assert_alpha(&reference, x, y, 0);
        }
    }
}

#[test]
fn zero_width_and_different_fill_color_pass_through_unchanged() {
    for width in [0., 8.] {
        let mut original = path(
            rectangle(Point::new(20., 20.), Point::new(60., 60.), false),
            width,
            true,
        );
        let Primitive::Path { style, .. } = &mut original else {
            unreachable!();
        };
        style.pattern = LinePattern::Dashed;
        if width > 0. {
            style.fill = Some(Color::Custom([220, 30, 40]));
        }
        let merged = single_paint(original.clone());
        assert_eq!(format!("{merged:?}"), format!("{original:?}"));
        for (alpha, ink) in [(1., 255), (0.5, 128)] {
            let image = raster(&merged, alpha);
            let reference = raster(&original, alpha);
            assert_eq!(image.data(), reference.data());
            assert_alpha(&image, 40, 40, ink);
            assert_alpha(&image, 12, 40, 0);
        }
    }
}
