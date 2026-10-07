use super::*;
fn distance(a: Point3, b: Point3) -> f64 {
    let p = sub(a, b);
    dot(p, p).sqrt()
}
#[test]
fn proper_fit_and_inverse_preserve_distance_and_y_convention() {
    let source = vec![
        Point3 {
            x: 0.,
            y: 0.,
            z: 0.,
        },
        Point3 {
            x: 1.5,
            y: 0.,
            z: 0.,
        },
        Point3 {
            x: 0.,
            y: 1.,
            z: 0.,
        },
        Point3 {
            x: 0.,
            y: 0.,
            z: 1.,
        },
    ];
    let rotation = Rotation::axis(53., 1).composed(Rotation::axis(-27., 0));
    let target: Vec<_> = source
        .iter()
        .map(|p| {
            add(
                rotation.apply(*p),
                Point3 {
                    x: 3.,
                    y: -2.,
                    z: 4.,
                },
            )
        })
        .collect();
    let mut frame = ViewFrame::fitted(&source, &target, 28.).unwrap();
    for (p, t) in source.iter().zip(&target) {
        let (xy, z) = frame.project(*p);
        assert!((f64::from(xy.x) - t.x * 28.).abs() < 0.001);
        assert!((f64::from(xy.y) + t.y * 28.).abs() < 0.001);
        assert!(distance(frame.unproject(xy, z), *p) < 1e-6);
    }
    for _ in 0..4 {
        frame.rotate(90., 90.);
        frame.roll(90.);
    }
    let a = frame.rotation.apply(source[1]);
    let b = frame.rotation.apply(source[2]);
    assert!((distance(a, b) - distance(source[1], source[2])).abs() < 1e-10);
    let (xy, z) = frame.project(source[3]);
    assert!(distance(frame.unproject(xy, z), source[3]) < 1e-6);
}
#[test]
fn mirrored_target_does_not_make_an_improper_rotation() {
    let p = vec![
        Point3 {
            x: 1.,
            y: 0.,
            z: 0.,
        },
        Point3 {
            x: 0.,
            y: 1.,
            z: 0.,
        },
        Point3 {
            x: 0.,
            y: 0.,
            z: 1.,
        },
        Point3::default(),
    ];
    let mirrored: Vec<_> = p.iter().map(|a| Point3 { x: -a.x, ..*a }).collect();
    let frame = ViewFrame::fitted(&p, &mirrored, 28.).unwrap();
    let x = frame.rotation.apply(Point3 {
        x: 1.,
        y: 0.,
        z: 0.,
    });
    let y = frame.rotation.apply(Point3 {
        x: 0.,
        y: 1.,
        z: 0.,
    });
    let z = frame.rotation.apply(Point3 {
        x: 0.,
        y: 0.,
        z: 1.,
    });
    let cross = Point3 {
        x: x.y * y.z - x.z * y.y,
        y: x.z * y.x - x.x * y.z,
        z: x.x * y.y - x.y * y.x,
    };
    assert!((dot(cross, z) - 1.).abs() < 1e-12);
}
