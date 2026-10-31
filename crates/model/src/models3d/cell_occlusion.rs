//! Cell ink visible past the model's space-filling balls. Work in rotated
//! ångström coordinates, matching the balls' projected discs and front depth.
use super::{View, length, sub, zip3};

/// Whether the drawn ball covers a point. Its front surface uses the screen
/// offset mapped back by the ball centre's projection factor.
fn covered(point: [f32; 3], center: [f32; 3], radius: f32, camera: Option<f32>) -> bool {
    let [px, py, pz] = point;
    let [cx, cy, cz] = center;
    let factor = camera.map_or(1., |camera| (camera - cz) / (camera - pz));
    let (x, y) = (px * factor - cx, py * factor - cy);
    let front = radius * radius - x * x - y * y;
    front >= 0. && pz - cz < front.sqrt()
}

/// Visible pieces of a cell edge, found by sampling it and bisecting each
/// change of visibility. All shown balls take part, copies included.
pub(super) fn edge(view: &View<'_>, from: [f32; 3], to: [f32; 3]) -> Vec<([f32; 3], [f32; 3])> {
    const STEPS: u16 = 128;
    let at = |t: f32| zip3(from, to, |f, e| f + (e - f) * t);
    let hid = |t| hidden(view, at(t), false);
    let mut visible = vec![];
    let (mut last, mut was) = (0., hid(0.));
    let mut start = 0.;
    for k in 1..=STEPS {
        let t = f32::from(k) / f32::from(STEPS);
        let is = hid(t);
        if is != was {
            let (mut a, mut b) = (last, t);
            for _ in 0..24 {
                let mid = (a + b) / 2.;
                if hid(mid) == was {
                    a = mid;
                } else {
                    b = mid;
                }
            }
            let cut = (a + b) / 2.;
            if is {
                visible.push((at(start), at(cut)));
            } else {
                start = cut;
            }
        }
        (last, was) = (t, is);
    }
    if !was {
        visible.push((at(start), to));
    }
    visible
}

/// Whether a shown ball covers `point`; with `keep_own`, a ball centred on
/// the point itself does not.
pub(super) fn hidden(view: &View<'_>, point: [f32; 3], keep_own: bool) -> bool {
    let camera = (view.perspective > 0.).then_some(view.camera);
    view.symbols
        .iter()
        .zip(&view.rotated)
        .zip(&view.shown)
        .filter(|(_, shown)| **shown)
        .any(|((symbol, &center), _)| {
            !(keep_own && length(sub(center, point)) < 1e-5)
                && covered(point, center, view.style.atom_radius(symbol), camera)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::Point, models3d::Style};

    fn view(centers: &[[f32; 3]], perspective: bool) -> View<'static> {
        View {
            center: [0.; 3],
            symbols: vec!["C"; centers.len()],
            rotated: centers.to_vec(),
            shown: vec![true; centers.len()],
            originals: centers.len(),
            solid: centers.len(),
            copies: Default::default(),
            bonds: vec![],
            style: Style::SpaceFilling,
            ellipsoids: vec![None; centers.len()],
            reach: 5.,
            origin: Point::default(),
            scale: 1.,
            camera: 10.,
            perspective: if perspective { 1. } else { 0. },
            wire: 1.,
        }
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }

    #[test]
    fn orthographic_clips_to_the_silhouette_behind_a_ball() {
        let view = view(&[[0.; 3]], false);
        let pieces = edge(&view, [-3., 0., -2.], [3., 0., -2.]);
        assert_eq!(pieces.len(), 2);
        close(pieces[0].1[0], -1.7);
        close(pieces[1].0[0], 1.7);
        assert_eq!(edge(&view, [-3., 0., 2.], [3., 0., 2.]).len(), 1);
    }

    #[test]
    fn facing_surface_ends_visibility_and_back_surface_does_not_restore_it() {
        for perspective in [false, true] {
            let view = view(&[[0.; 3]], perspective);
            let pieces = edge(&view, [0., 0., 3.], [0., 0., -3.]);
            assert_eq!(pieces.len(), 1);
            close(pieces[0].1[2], 1.7);
            assert!(edge(&view, [0.; 3], [0., 0., -3.]).is_empty());
            assert!(!hidden(&view, [0., 0., 1.7], false));
            assert!(hidden(&view, [0., 0., -1.7], false));
            assert!(!hidden(&view, [0., 0., 1.8], false));
            assert!(hidden(&view, [0., 0., -1.8], false));
        }
    }

    #[test]
    fn perspective_clips_to_the_drawn_disc() {
        let view = view(&[[0.; 3]], true);
        let pieces = edge(&view, [-4., 0., -2.], [4., 0., -2.]);
        assert_eq!(pieces.len(), 2);
        close(pieces[0].1[0], -2.04);
        close(pieces[1].0[0], 2.04);
        assert!(hidden(&view, [2.03, 0., -2.], false));
        // A true sphere's tangent cone extends past the rendered silhouette.
        assert!(!hidden(&view, [2.06, 0., -2.], false));
    }

    #[test]
    fn perspective_off_axis_disc_uses_the_ball_centers_depth() {
        let mut view = view(&[[3., 2., 2.]], true);
        view.scale = 17.;
        view.origin = Point::new(12., -8.);
        let pieces = edge(&view, [-1., 3., -2.], [9., 3., -2.]);
        assert_eq!(pieces.len(), 2);
        close(pieces[0].1[0], 1.95);
        close(pieces[1].0[0], 7.05);
        let (_, center, radius) = view.balls().next().unwrap();
        let left = view.project(pieces[0].1).0;
        let right = view.project(pieces[1].0).0;
        close(left.x, center.x - radius);
        close(right.x, center.x + radius);
        close(left.y, center.y);
        close(right.y, center.y);
        assert!(!hidden(&view, [1.94, 3., -2.], false));
        assert!(hidden(&view, [1.96, 3., -2.], false));
        assert!(hidden(&view, [7.04, 3., -2.], false));
        assert!(!hidden(&view, [7.06, 3., -2.], false));
    }

    #[test]
    fn perspective_varying_depth_crosses_the_mapped_front_surface() {
        let center = [3., 2., 1.];
        let view = view(&[center], true);
        let radius = view.style.atom_radius("C");
        // These local screen offsets and front depths lie on the same ball:
        // 0.8² + 0.6² = 1. Undo its centre factor to obtain edge points.
        let surface = |x: f32, z: f32| {
            let z = center[2] + radius * z;
            let factor = (view.camera - z) / (view.camera - center[2]);
            [(center[0] + radius * x) * factor, center[1] * factor, z]
        };
        let a = surface(-0.8, 0.6);
        let b = surface(0.6, 0.8);
        let from = zip3(a, b, |a, b| a - (b - a) / 2.);
        let to = zip3(a, b, |a, b| b + (b - a) / 2.);
        let pieces = edge(&view, from, to);
        assert_eq!(pieces.len(), 2);
        for i in 0..3 {
            close(pieces[0].1[i], a[i]);
            close(pieces[1].0[i], b[i]);
        }
        let (projected_center, factor) = view.project(center);
        for point in [pieces[0].1, pieces[1].0] {
            let projected = view.project(point).0;
            let x = (projected.x - projected_center.x) / (view.scale * factor);
            let y = (projected.y - projected_center.y) / (view.scale * factor);
            let front = center[2] + (radius * radius - x * x - y * y).sqrt();
            close(point[2], front);
        }
        assert!(!hidden(&view, from, false));
        assert!(hidden(&view, zip3(a, b, |a, b| (a + b) / 2.), false));
        assert!(!hidden(&view, to, false));
    }

    #[test]
    fn shown_appended_copies_cover_edges_and_hidden_copies_do_not() {
        let mut view = view(&[[-2., 0., 0.]], false);
        view.symbols.push("C");
        view.rotated.push([2., 0., 0.]);
        view.shown.push(true);
        let pieces = edge(&view, [-5., 0., -2.], [5., 0., -2.]);
        assert_eq!(pieces.len(), 3);
        close(pieces[0].1[0], -3.7);
        close(pieces[1].0[0], -0.3);
        close(pieces[1].1[0], 0.3);
        close(pieces[2].0[0], 3.7);
        view.shown[1] = false;
        assert_eq!(edge(&view, [-5., 0., -2.], [5., 0., -2.]).len(), 2);
        assert!(!hidden(&view, [2., 0., -2.], false));
    }

    #[test]
    fn origin_exempts_only_its_own_atom() {
        for perspective in [false, true] {
            let mut view = view(&[[0.; 3]], perspective);
            assert!(hidden(&view, [0.; 3], false));
            assert!(!hidden(&view, [0.; 3], true));
            view.symbols.push("C");
            view.rotated.push([0., 0., 1.]);
            view.shown.push(true);
            assert!(hidden(&view, [0.; 3], true));
        }
    }
}
