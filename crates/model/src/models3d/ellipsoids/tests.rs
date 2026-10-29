use super::*;
use crate::{
    document::Document,
    models3d::{ForceField, Model3d, Placed},
    scene,
};

fn cell(vectors: [[f32; 3]; 3]) -> Cell {
    Cell {
        vectors,
        crossing: vec![],
    }
}

fn point(doc: &Document) -> Model3d {
    Model3d::new(
        doc,
        vec![Placed {
            id: doc.atoms[0].id,
            xyz: [0.; 3],
        }],
        vec![],
        ForceField::Uff,
    )
}

fn view<'a>(doc: &'a Document, model: &'a Model3d, perspective: bool) -> View<'a> {
    let mut view = View::new(doc, model).unwrap();
    view.origin = Point::new(30., 40.);
    view.scale = 10.;
    view.camera = 10.;
    view.perspective = if perspective { 1. } else { 0. };
    view
}

fn document() -> Document {
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    doc
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 5e-6, "{a} != {b}");
}

#[test]
fn cif_covariance_cancels_orthogonal_lengths_and_uses_lattice_columns() {
    let u = [0.04, 0.09, 0.16, 0.01, 0.02, 0.03];
    for vectors in [
        [[3., 0., 0.], [0., 3., 0.], [0., 0., 3.]],
        [[3., 0., 0.], [0., 5., 0.], [0., 0., 7.]],
    ] {
        let actual = covariance(u, &cell(vectors));
        let expected = [[0.04, 0.01, 0.02], [0.01, 0.09, 0.03], [0.02, 0.03, 0.16]];
        for (a, b) in actual
            .into_iter()
            .flatten()
            .zip(expected.into_iter().flatten())
        {
            close(a, b);
        }
    }

    // a* = √2, b* = c* = 1, hence A N has columns (√2,0,0), (1,1,0), (0,0,1).
    let skew = cell([[1., 0., 0.], [1., 1., 0.], [0., 0., 1.]]);
    let actual = covariance([1., 2., 3., 0.1, 0.2, 0.3], &skew);
    let s = 2f64.sqrt();
    let expected = [
        [4. + 0.2 * s, 2. + 0.1 * s, 0.2 * s + 0.3],
        [2. + 0.1 * s, 2., 0.3],
        [0.2 * s + 0.3, 0.3, 3.],
    ];
    for (a, b) in actual
        .into_iter()
        .flatten()
        .zip(expected.into_iter().flatten())
    {
        close(a, b);
    }
}

#[test]
fn jacobi_recovers_principal_axes_and_reconstructs_a_symmetric_tensor() {
    let matrix = [[2., 1., 0.], [1., 2., 0.], [0., 0., 4.]];
    let (values, vectors) = eigen(matrix);
    let mut sorted = values;
    sorted.sort_by(f64::total_cmp);
    for (a, b) in sorted.into_iter().zip([1., 3., 4.]) {
        close(a, b);
    }
    for i in 0..3 {
        for j in 0..3 {
            close(
                vectors[i].iter().zip(vectors[j]).map(|(a, b)| a * b).sum(),
                if i == j { 1. } else { 0. },
            );
            close(
                (0..3)
                    .map(|k| vectors[k][i] * values[k] * vectors[k][j])
                    .sum(),
                matrix[i][j],
            );
        }
    }
}

#[test]
fn probability_scales_semiaxes_and_invalid_adps_have_no_ellipsoid() {
    for probability in Probability::ALL {
        let ball = Ellipsoid::of(Adp::Iso(0.04), None, probability).unwrap();
        assert!(!ball.anisotropic);
        close(ball.radius() as f64, 0.2 * probability.scale() as f64);
        close(ball.radius_along([0., 0., 3.]) as f64, ball.radius() as f64);
    }
    assert_eq!(Probability::default(), Probability::Fifty);
    assert_eq!(Probability::ALL.map(Probability::percent), [30, 50, 70, 90]);
    let unit = cell([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]);
    for u in [
        [-1., 1., 1., 0., 0., 0.],
        [1., 1., 1., 2., 0., 0.],
        [0., 1., 1., 0., 0., 0.],
    ] {
        assert!(Ellipsoid::of(Adp::Aniso(u), Some(&unit), Probability::Fifty).is_none());
    }
    assert!(Ellipsoid::of(Adp::Iso(0.), None, Probability::Fifty).is_none());
    assert!(Ellipsoid::of(Adp::Iso(-0.1), None, Probability::Fifty).is_none());
}

#[test]
fn orthographic_and_perspective_silhouettes_match_known_axes() {
    let doc = document();
    let ellipsoid = Ellipsoid {
        axes: [[2., 0., 0.], [0., 1., 0.], [0., 0., 0.5]],
        anisotropic: true,
    };
    let flat = ellipsoid
        .project(&view(&doc, &point(&doc), false), [0.; 3])
        .unwrap();
    assert_eq!(flat.center, Point::new(30., 40.));
    close(flat.axes[0].x.hypot(flat.axes[0].y) as f64, 20.);
    close(flat.axes[1].x.hypot(flat.axes[1].y) as f64, 10.);
    assert!(flat.contains(Point::new(50., 40.), 0.));
    assert!(!flat.contains(Point::new(50.1, 40.), 0.));
    assert!(flat.contains(Point::new(50.1, 40.), 0.2));
    let perspective = ellipsoid
        .project(&view(&doc, &point(&doc), true), [0.; 3])
        .unwrap();
    let spread = 1. / (1. - 0.5f64.powi(2) / 100.).sqrt();
    close(
        perspective.axes[0].x.hypot(perspective.axes[0].y) as f64,
        20. * spread,
    );
    close(
        perspective.axes[1].x.hypot(perspective.axes[1].y) as f64,
        10. * spread,
    );
    let off_axis = Ellipsoid::sphere(1.)
        .project(&view(&doc, &point(&doc), true), [3., 0., 0.])
        .unwrap();
    close(off_axis.center.x as f64, 30. + 3000. / 99.);
}

#[test]
fn every_perspective_rim_ray_is_tangent_to_the_rotated_ellipsoid() {
    let doc = document();
    let model = point(&doc);
    let view = view(&doc, &model, true);
    let ellipsoid = Ellipsoid {
        axes: [[1.8, 0., 0.], [0., 0.7, 0.], [0., 0., 0.35]],
        anisotropic: true,
    }
    .rotated(super::super::turn([1., 0., 0., 0.], 45., -32., false));
    let center = [1.6, -0.8, 1.2];
    let projection = ellipsoid.project(&view, center).unwrap();
    let eye = [0., 0., view.camera];
    let relative = sub(eye, center);
    for i in 0..24 {
        let t = std::f32::consts::TAU * i as f32 / 24.;
        let p = projection.center.offset(
            projection.axes[0].x * t.cos() + projection.axes[1].x * t.sin(),
            projection.axes[0].y * t.cos() + projection.axes[1].y * t.sin(),
        );
        let ray = [
            (p.x - view.origin.x) / view.scale,
            -(p.y - view.origin.y) / view.scale,
            -view.camera,
        ];
        let mut a = 0.;
        let mut b = 0.;
        let mut c = -1.;
        for axis in ellipsoid.axes {
            let squared = dot(axis, axis);
            let r = dot(ray, axis) / squared;
            let e = dot(relative, axis) / squared;
            a += r * r;
            b += r * e;
            c += e * e;
        }
        assert!(
            (b * b - a * c).abs() / (a * c).abs() < 2e-5,
            "rim ray {i} is not tangent"
        );
    }
}

#[test]
fn aniso_sections_render_through_the_shared_svg_path_and_iso_is_plain() {
    let doc = document();
    let model = point(&doc);
    let view = view(&doc, &model, true);
    let ball = Ellipsoid::sphere(1.);
    let plain = ball
        .draw(&view, [0.; 3], [0.3, 0.5, 0.8], false, None)
        .unwrap();
    assert!(matches!(plain, Primitive::Ellipsoid { section: None, .. }));
    let ellipsoid = Ellipsoid {
        anisotropic: true,
        ..ball
    }
    .rotated(super::super::turn([1., 0., 0., 0.], 40., 20., false));
    let primitive = ellipsoid
        .draw(&view, [0.; 3], [0.3, 0.5, 0.8], false, None)
        .unwrap();
    let Primitive::Ellipsoid {
        section: Some(ref section),
        ..
    } = primitive
    else {
        panic!()
    };
    assert_eq!(section.arcs.len(), 3);
    assert_eq!(section.octant.len(), 72);
    let svg = scene::svg_part(&[primitive], Point::new(0., 0.), Point::new(80., 80.));
    let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(80, 80).unwrap();
    resvg::render(&tree, Default::default(), &mut pixmap.as_mut());
    assert_eq!(pixmap.pixel(30, 40).unwrap().alpha(), 255);
    assert_eq!(pixmap.pixel(10, 40).unwrap().alpha(), 0);
    assert!(svg.contains("<ellipse"));
    assert!(svg.contains("gradientTransform"));
}
