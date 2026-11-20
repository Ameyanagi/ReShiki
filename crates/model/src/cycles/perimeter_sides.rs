//! Side contacts on the resolved, open, painted step shafts.
use super::super::{Projection, center_of, shapes::Shaft, side_arrow};
use crate::{
    arrows::ArrowStyle,
    document::{Arrow, Point},
};

pub(super) fn arrow(
    id: u64,
    shafts: &[Shaft],
    ink: (Point, Point),
    incoming: bool,
    bond: f32,
    style: &ArrowStyle,
) -> Option<Arrow> {
    let center = center_of(ink);
    // A tie keeps the first (outside, forward) lane. In particular, a
    // reversible contact never floats on the undrawn baseline between lanes.
    let mut best: Option<(&Shaft, Projection)> = None;
    for shaft in shafts {
        let projected = shaft.project(center)?;
        if best
            .as_ref()
            .is_none_or(|(_, old)| projected.distance < old.distance)
        {
            best = Some((shaft, projected));
        }
    }
    let (shaft, projected) = best?;
    let ahead = (1.2 * f64::from(bond)).max(1.1 * projected.distance);
    let s = (projected.s + if incoming { ahead } else { -ahead })
        .clamp(0.15 * shaft.length(), 0.85 * shaft.length());
    // An entering reagent continues along the leaving side of a corner;
    // a departing product follows the arriving side. Never average the turn.
    let (touch, tangent) = shaft.sample(s, !incoming)?;
    Some(side_arrow(id, touch, tangent, ink, incoming, style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        arrows::{Head, Preset},
        document::ArrowNode,
        graphics::PathNodeType,
    };

    fn straight(start: Point, end: Point) -> Shaft {
        Shaft::from_arrow(&Arrow::new(
            1,
            start,
            end,
            Preset::Forward,
            ArrowStyle {
                head: Head::None,
                ..ArrowStyle::DEFAULT
            },
        ))
        .unwrap()
    }

    #[test]
    fn equidistant_lanes_choose_forward_but_inside_contacts_follow_the_reverse_lane() {
        let shafts = [
            straight(Point::new(0., -10.), Point::new(1000., -10.)),
            straight(Point::new(1000., 10.), Point::new(0., 10.)),
        ];
        let ink = |y: f32| (Point::new(499., y - 1.), Point::new(501., y + 1.));
        let tied = arrow(1, &shafts, ink(0.), true, 1., &ArrowStyle::DEFAULT).unwrap();
        assert_eq!(tied.end.y, -10.);
        let incoming = arrow(1, &shafts, ink(9.), true, 1., &ArrowStyle::DEFAULT).unwrap();
        let outgoing = arrow(1, &shafts, ink(9.), false, 1., &ArrowStyle::DEFAULT).unwrap();
        assert_eq!(incoming.end.y, 10.);
        assert!(incoming.end.x < 500.);
        assert_eq!(outgoing.start.y, 10.);
        assert!(outgoing.start.x > 500.);
        assert!(incoming.cubic.unwrap()[1].x > incoming.end.x);
        assert!(outgoing.cubic.unwrap()[0].x < outgoing.start.x);
    }

    #[test]
    fn true_length_shift_hits_exact_corner_and_uses_the_applicable_one_sided_tangent() {
        let mut a = Arrow::new(
            1,
            Point::default(),
            Point::new(600., 400.),
            Preset::Forward,
            ArrowStyle {
                head: Head::None,
                ..ArrowStyle::DEFAULT
            },
        );
        a.cubic = Some([Point::new(200., 0.), Point::new(600., 800. / 3.)]);
        a.nodes.push(ArrowNode {
            point: Point::new(600., 0.),
            controls: [Point::new(400., 0.), Point::new(600., 400. / 3.)],
            kind: PathNodeType::Corner,
        });
        let shafts = [Shaft::from_arrow(&a).unwrap()];
        let incoming = arrow(
            1,
            &shafts,
            (Point::new(539., -11.), Point::new(541., -9.)),
            true,
            50.,
            &ArrowStyle::DEFAULT,
        )
        .unwrap();
        let outgoing = arrow(
            1,
            &shafts,
            (Point::new(609., 59.), Point::new(611., 61.)),
            false,
            50.,
            &ArrowStyle::DEFAULT,
        )
        .unwrap();
        assert_eq!(incoming.end, Point::new(600., 0.));
        assert_eq!(outgoing.start, incoming.end);
        assert_eq!(incoming.cubic.unwrap()[1].x, 600.);
        assert!(incoming.cubic.unwrap()[1].y < 0.);
        assert_eq!(outgoing.cubic.unwrap()[0].y, 0.);
        assert!(outgoing.cubic.unwrap()[0].x > 600.);
    }

    #[test]
    fn distant_probes_clamp_to_true_painted_length_and_empty_targets_draw_nothing() {
        let shafts = [straight(Point::default(), Point::new(1000., 0.))];
        let ink = (Point::new(499., -10001.), Point::new(501., -9999.));
        assert_eq!(
            arrow(1, &shafts, ink, true, 50., &ArrowStyle::DEFAULT)
                .unwrap()
                .end
                .x,
            850.
        );
        assert_eq!(
            arrow(1, &shafts, ink, false, 50., &ArrowStyle::DEFAULT)
                .unwrap()
                .start
                .x,
            150.
        );
        assert!(arrow(1, &[], ink, false, 50., &ArrowStyle::DEFAULT).is_none());
    }
}
