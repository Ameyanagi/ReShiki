//! Side contacts on the resolved, open, painted step shafts.
use super::super::{Projection, center_of, shapes::Shaft, side_arrow, spaced_joins};
use crate::{
    arrows::ArrowStyle,
    document::{Arrow, Point},
};

#[derive(Clone, Copy)]
struct Attachment<'a> {
    shaft: &'a Shaft,
    lane: usize,
    ink: (Point, Point),
    incoming: bool,
    projected: f64,
    s: f64,
}

pub(super) fn arrows<'a>(
    id: u64,
    shafts: impl IntoIterator<Item = &'a Shaft>,
    parts: &[(bool, (Point, Point))],
    bond: f32,
    style: &ArrowStyle,
) -> Vec<Arrow> {
    let shafts: Vec<_> = shafts.into_iter().collect();
    let mut attachments = vec![];
    for &(incoming, ink) in parts {
        let center = center_of(ink);
        // A tie keeps the first (outside, forward) lane. A reversible
        // contact never floats on the undrawn baseline between lanes.
        let mut best: Option<(usize, &Shaft, Projection)> = None;
        for (lane, &shaft) in shafts.iter().enumerate() {
            let Some(projected) = shaft.project(center) else {
                continue;
            };
            if best
                .as_ref()
                .is_none_or(|(_, _, old)| projected.distance < old.distance)
            {
                best = Some((lane, shaft, projected));
            }
        }
        let Some((lane, shaft, projected)) = best else {
            continue;
        };
        let ahead = (1.2 * f64::from(bond)).max(1.1 * projected.distance);
        attachments.push(Attachment {
            shaft,
            lane,
            ink,
            incoming,
            projected: projected.s,
            s: (projected.s + if incoming { ahead } else { -ahead })
                .clamp(0.15 * shaft.length(), 0.85 * shaft.length()),
        });
    }
    for lane in 0..shafts.len() {
        let mut ordered: Vec<_> = attachments
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, a)| a.lane == lane)
            .collect();
        ordered.sort_by(|(_, a), (_, b)| a.projected.total_cmp(&b.projected));
        let Some((_, first)) = ordered.first().filter(|_| ordered.len() > 1) else {
            continue;
        };
        let length = first.shaft.length();
        let gap = (f64::from(bond) / length).min(0.7 / (ordered.len() - 1) as f64) as f32;
        let preferred: Vec<_> = ordered.iter().map(|(_, a)| (a.s / length) as f32).collect();
        for ((index, _), t) in ordered.into_iter().zip(spaced_joins(&preferred, gap)) {
            if let Some(attachment) = attachments.get_mut(index) {
                attachment.s = f64::from(t) * length;
            }
        }
    }
    attachments
        .into_iter()
        .filter_map(|a| {
            // An entering reagent continues along the leaving side of a
            // corner; a departing product follows the arriving side.
            let (touch, tangent) = a.shaft.sample(a.s, !a.incoming)?;
            Some(side_arrow(id, touch, tangent, a.ink, a.incoming, style))
        })
        .collect()
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

    fn arrow<'a>(
        id: u64,
        shafts: impl IntoIterator<Item = &'a Shaft>,
        ink: (Point, Point),
        incoming: bool,
        bond: f32,
        style: &ArrowStyle,
    ) -> Option<Arrow> {
        arrows(id, shafts, &[(incoming, ink)], bond, style)
            .into_iter()
            .next()
    }

    #[test]
    fn paired_contacts_keep_projection_order_and_one_bond_of_painted_lane_clearance() {
        let shafts = [
            straight(Point::new(0., -10.), Point::new(1000., -10.)),
            straight(Point::new(1000., 10.), Point::new(0., 10.)),
        ];
        for (lane, y, first, second) in [(0, -50., 450., 550.), (1, 50., 550., 450.)] {
            let ink = |x: f32| (Point::new(x - 1., y - 1.), Point::new(x + 1., y + 1.));
            let parts = [(true, ink(first)), (false, ink(second))];
            let routes = arrows(1, &shafts, &parts, 100., &ArrowStyle::DEFAULT);
            assert_eq!(routes.len(), 2);
            let a = shafts[lane].project(routes[0].end).unwrap();
            let b = shafts[lane].project(routes[1].start).unwrap();
            assert!(a.distance < 0.001 && b.distance < 0.001);
            assert!(b.s - a.s >= 99.99, "{lane}: {} then {}", a.s, b.s);
            assert!(a.s >= 150. && b.s <= 850.);
            assert_eq!(
                routes,
                arrows(1, &shafts, &parts, 100., &ArrowStyle::DEFAULT)
            );
        }
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
