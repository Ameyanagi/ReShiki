use super::{cross, unit};

#[test]
fn x_cross_y_is_z() {
    assert_eq!(cross([1., 0., 0.], [0., 1., 0.]), [0., 0., 1.]);
}

#[test]
fn unit_rejects_small_and_nonfinite_lengths() {
    for vector in [
        [0., 0., 0.],
        [0.0005, 0., 0.],
        [0.001, 0., 0.],
        [f32::NAN, 0., 0.],
        [f32::INFINITY, 0., 0.],
        [f32::NEG_INFINITY, 0., 0.],
        [f32::MAX, 0., 0.],
    ] {
        assert_eq!(unit(vector), None);
    }
}

#[test]
fn unit_normalizes_lengths_above_threshold() {
    assert_eq!(unit([0.002, 0., 0.]), Some([1., 0., 0.]));
    assert_eq!(unit([3., 4., 0.]), Some([0.6, 0.8, 0.]));
}
