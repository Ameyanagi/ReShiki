use super::*;
use crate::canvas::World;

#[test]
fn rulers_use_export_scale_and_follow_camera() {
    assert!((42. * Unit::Points.per_world() - 14.4).abs() < 0.00001);
    assert!((42. * Unit::Millimetres.per_world() - 5.08).abs() < 0.00001);
    for unit in Unit::ALL {
        for zoom in [0.25, 1., 1.7, 5.] {
            let camera = Camera {
                center: World::new(42., -84.),
                zoom,
            };
            for center in [camera.center.x, camera.center.y] {
                let ticks = ticks(center, zoom, 800., unit);
                assert!(ticks.len() > 4 && ticks.len() < 100);
                let mut previous = None;
                for tick in ticks {
                    if let Some(label) = tick.label {
                        let value: f64 = label.parse().expect("numeric ruler label");
                        let expected = ((value / unit.per_world() - f64::from(center))
                            * f64::from(zoom)
                            + 400.) as f32;
                        assert!((tick.pixel - expected).abs() < 0.001);
                        if let Some(p) = previous {
                            assert!(tick.pixel - p >= 63.9);
                        }
                        previous = Some(tick.pixel);
                    }
                }
            }
        }
    }
    for invalid in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(ticks(0., invalid, 800., Unit::Points).is_empty());
    }
}
