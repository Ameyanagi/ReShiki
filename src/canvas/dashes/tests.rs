use super::*;

#[test]
fn ellipse_dashes_stay_on_the_curve_at_high_zoom() {
    use reshiki::{
        document::Point as World,
        graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
    };
    let ellipse = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        World::new(-250., -250.),
        World::new(250., 250.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    let path = Path::new(|b| {
        for c in ellipse.commands() {
            let p = |v: World| Point::new(v.x, v.y);
            match c {
                PathCommand::Move(v) => b.move_to(p(v)),
                PathCommand::Line(v) => b.line_to(p(v)),
                PathCommand::Cubic(a, z, v) => b.bezier_curve_to(p(a), p(z), p(v)),
                PathCommand::Close => b.close(),
            }
        }
    });
    let dashes = dashed(&path, &[80., 30.]);
    let mut curves = 0;
    for event in dashes.raw() {
        if let Event::Cubic {
            from,
            ctrl1,
            ctrl2,
            to,
        } = event
        {
            let curve = Curve {
                from,
                ctrl1,
                ctrl2,
                to,
            };
            for i in 0..=20 {
                let p = curve.sample(i as f32 / 20.);
                assert!((p.to_vector().length() - 250.).abs() < 0.1);
            }
            curves += 1;
        }
        assert!(!matches!(event, Event::Line { .. }));
    }
    assert!(curves > 10);
}

#[test]
fn a_dash_crossing_a_rectangle_corner_keeps_the_corner() {
    let path = Path::new(|b| {
        b.move_to(Point::ORIGIN);
        b.line_to(Point::new(10., 0.));
        b.line_to(Point::new(10., 10.));
    });
    let result = dashed(&path, &[15., 5.]);
    let lines: Vec<_> = result
        .raw()
        .into_iter()
        .filter_map(|e| {
            if let Event::Line { from, to } = e {
                Some((from, to))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].1, Point2D::new(10., 0.));
    assert_eq!(lines[1].0, lines[0].1);
    assert_eq!(lines[1].1, Point2D::new(10., 5.));
}
