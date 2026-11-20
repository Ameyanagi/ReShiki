//! Keep inherited side parts beside the step that an insertion shortens.
use super::*;

type Ink = (Point, Point);

pub(super) struct Split {
    cycle: u64,
    step: usize,
    old: Span,
    sides: Vec<Side>,
}

struct Side {
    ids: Vec<u64>,
    body: Ink,
    ink: Ink,
    incoming: bool,
    along: f32,
}

fn view(doc: &Document, id: u64, step: usize) -> Option<(Drawn, Span)> {
    let g = doc.graphics.iter().find(|g| g.id == id)?;
    let mut found = None;
    let drawn = drawn_spans(doc, g, |s, arc| {
        if s == step && arc.radius > 1e-3 && arc.sweep.abs() > 1e-3 {
            found = Some(arc);
        }
    });
    Some((drawn, found?))
}

fn projection(arc: Span, p: Point) -> Option<f32> {
    (p.distance(arc.center) > 1e-3).then(|| arc.along(p))
}

pub(super) fn prepare(doc: &Document, target: &Target) -> Option<Split> {
    let Role::Intermediate(i) = target.role else {
        return None;
    };
    let cycle = doc
        .graphics
        .iter()
        .find(|g| g.id == target.cycle)?
        .cycle
        .as_deref()?;
    let n = cycle.intermediates.len();
    if n == 0 {
        return None;
    }
    let i = i.min(n);
    let before = (i + n - 1) % n;
    let step = cycle.steps.get(before)?;
    if step.reagents.is_empty() && step.products.is_empty() {
        return None;
    }
    let (_, old) = view(doc, target.cycle, before)?;
    let sides = step
        .reagents
        .iter()
        .map(|p| (p, true))
        .chain(step.products.iter().map(|p| (p, false)))
        .filter_map(|(part, incoming)| {
            let body = crate::scene::selection_bounds(doc, part)?;
            let along = projection(old, center_of(body))?;
            // An already remote placement is the user's, not this split's.
            if !(0. ..=1.).contains(&along) {
                return None;
            }
            let ids = crate::molecule_names::include_annotations(doc, part);
            Some(Side {
                ink: crate::scene::selection_bounds(doc, &ids)?,
                ids,
                body,
                incoming,
                along,
            })
        })
        .collect();
    Some(Split {
        cycle: target.cycle,
        step: if i == 0 { n } else { i - 1 },
        old,
        sides,
    })
}

fn points(a: &Arrow) -> Vec<Point> {
    (0..=96).map(|i| a.point(i as f32 / 96.)).collect()
}

fn overlap(a: Ink, b: Ink) -> bool {
    a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
}

fn segment_distance(a: &[Point], b: &[Point]) -> f32 {
    let (&[a0, a1], &[b0, b1]) = (a, b) else {
        return f32::INFINITY;
    };
    let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
    let vector = |a: Point, b: Point| Point::new(b.x - a.x, b.y - a.y);
    let r = vector(a0, a1);
    let s = vector(b0, b1);
    let d = cross(r, s);
    if d.abs() > 1e-6 {
        let from = vector(a0, b0);
        let (t, u) = (cross(from, s) / d, cross(from, r) / d);
        if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
            return 0.;
        }
    }
    let distance = |p: Point, a: Point, b: Point| {
        let v = vector(a, b);
        let length = v.x * v.x + v.y * v.y;
        let t = if length > 1e-8 {
            ((p.x - a.x) * v.x + (p.y - a.y) * v.y) / length
        } else {
            0.
        }
        .clamp(0., 1.);
        p.distance(a.lerp(b, t))
    };
    [
        distance(a0, b0, b1),
        distance(a1, b0, b1),
        distance(b0, a0, a1),
        distance(b1, a0, a1),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

fn crossing(a: &[Point], b: &[Point], clear: f32) -> bool {
    a.windows(2)
        .any(|a| b.windows(2).any(|b| segment_distance(a, b) <= clear))
}

fn hits(ink: Ink, path: &[Point]) -> bool {
    path.iter().any(|p| contains(ink, *p))
        || crossing(
            path,
            &[
                ink.0,
                Point::new(ink.1.x, ink.0.y),
                ink.1,
                Point::new(ink.0.x, ink.1.y),
                ink.0,
            ],
            0.05,
        )
}

fn translated(b: Ink, delta: Point) -> Ink {
    (b.0.offset(delta.x, delta.y), b.1.offset(delta.x, delta.y))
}

fn label_ink((origin, text, format): &(Point, String, TextFormat)) -> Ink {
    let layout = crate::typography::layout(text, format);
    (*origin, origin.offset(layout.width, layout.height))
}

fn support(ink: Ink, origin: Point, normal: Point) -> f32 {
    [
        ink.0,
        Point::new(ink.1.x, ink.0.y),
        ink.1,
        Point::new(ink.0.x, ink.1.y),
    ]
    .into_iter()
    .map(|p| (origin.x - p.x) * normal.x + (origin.y - p.y) * normal.y)
    .fold(0., f32::max)
}

pub(super) fn apply(doc: &mut Document, mut split: Split) {
    let Some((drawn, arc)) = view(doc, split.cycle, split.step) else {
        return;
    };
    split
        .sides
        .retain(|p| projection(arc, center_of(p.body)).is_some_and(|t| t < -0.02 || t > 1.02));
    if split.sides.is_empty() {
        return;
    }
    split.sides.sort_by(|a, b| a.along.total_cmp(&b.along));
    let moving: HashSet<_> = split
        .sides
        .iter()
        .flat_map(|p| p.ids.iter().copied())
        .collect();
    let Some(g) = doc.graphics.iter().find(|g| g.id == split.cycle) else {
        return;
    };
    let Some(cycle) = g.cycle.as_deref() else {
        return;
    };
    let (center, radius) = g.cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    let gap = DEFAULT.world(cycle.gap_pt);
    let style = ArrowStyle {
        color: g.style.stroke,
        width_pt: g.style.width_pt,
        pattern: g.style.pattern,
        ..ArrowStyle::preset(Preset::Forward)
    };
    let own: Vec<_> = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .filter(|(_, s)| **s == Some(split.step))
        .map(|(a, _)| points(a))
        .collect();
    let mut paths: Vec<_> = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .filter(|(_, s)| s.is_some_and(|s| s != split.step))
        .map(|(a, _)| points(a))
        .collect();
    // Keep untouched side curves clear too. Their order matches the side parts.
    let parts = cycle
        .parts()
        .filter(|(r, _)| !matches!(r, Role::Intermediate(_)))
        .filter(|(_, ids)| crate::scene::selection_bounds(doc, ids).is_some());
    for ((_, ids), a) in parts.zip(
        drawn
            .arrows
            .iter()
            .zip(&drawn.arcs)
            .filter(|(_, s)| s.is_none())
            .map(|(a, _)| a),
    ) {
        if ids.iter().all(|id| !moving.contains(id)) {
            paths.push(points(a));
        }
    }
    paths.extend(doc.arrows.iter().map(points));
    let units: Vec<_> = units(doc, &doc.all_ids())
        .into_iter()
        .map(|ids| crate::molecule_names::include_annotations(doc, &ids))
        .filter(|ids| ids.iter().all(|id| !moving.contains(id)))
        // A cycle only paints its resolved paths and labels. Its guide box
        // would incorrectly mark the whole interior as occupied.
        .chain(
            doc.graphics
                .iter()
                .filter(|g| g.cycle.is_none() && g.style.effects.opacity != Some(0.))
                .map(|g| vec![g.id]),
        )
        .collect();
    let mut ink: Vec<_> = crate::scene::selections_bounds(doc, &units)
        .into_iter()
        .flatten()
        .collect();
    ink.extend(drawn.labels.iter().map(label_ink));
    for g in doc
        .graphics
        .iter()
        .filter(|g| g.id != split.cycle && g.cycle.is_some() && g.style.effects.opacity != Some(0.))
    {
        let other = super::drawn(doc, g);
        paths.extend(other.arrows.iter().map(points));
        ink.extend(other.labels.iter().map(label_ink));
    }
    let mut moves = vec![];
    for side in split.sides {
        let from = center_of(side.body);
        let old_angle = split.old.angle(side.along);
        let old_normal = Point::new(old_angle.cos(), old_angle.sin());
        let offset = from.distance(split.old.center) - split.old.radius;
        let side_sign = if offset < 0. { -1. } else { 1. };
        let handed = side_sign * split.old.sweep.signum() * arc.sweep.signum();
        let clear = (offset.abs()
            - support(
                side.ink,
                from,
                Point::new(old_normal.x * side_sign, old_normal.y * side_sign),
            ))
        .max(gap);
        // Stored roles do not require a new layout convention. Keep the
        // nearest clear place on this step, with either end left open.
        let (lo, hi) = (0.15, 0.85);
        let near = arc.along(from).clamp(lo, hi);
        let mut best: Option<(Point, Ink, Vec<Point>, f32)> = None;
        for extra in [0., bond * 0.5, bond, bond * 2.] {
            for t in std::iter::once(near).chain((0..=64).map(|k| lo + (hi - lo) * k as f32 / 64.))
            {
                let angle = arc.angle(t);
                let normal = Point::new(angle.cos(), angle.sin());
                let out = handed
                    * (support(
                        side.ink,
                        from,
                        Point::new(normal.x * handed, normal.y * handed),
                    ) + clear
                        + extra);
                if arc.radius + out <= 1e-3 {
                    continue;
                }
                let to = arc.at(t, out);
                if to.distance(center) >= radius + 4. * bond - 0.05 {
                    continue;
                }
                let cost = from.distance(to);
                if best.as_ref().is_some_and(|b| cost >= b.3) {
                    continue;
                }
                let delta = Point::new(to.x - from.x, to.y - from.y);
                let b = translated(side.ink, delta);
                let route = points(&side_on(
                    split.cycle,
                    arc,
                    translated(side.body, delta),
                    side.incoming,
                    &style,
                    bond,
                    gap,
                ));
                let Some(touch) = (if side.incoming {
                    route.last()
                } else {
                    route.first()
                })
                .copied() else {
                    continue;
                };
                let stroke = DEFAULT.world(style.width_pt) * 1.5;
                // A side curve deliberately merges tangentially with this
                // step. Only that short join may share its stroke; foreign
                // arcs below always keep the full clearance.
                let crosses_own = own.iter().any(|old| {
                    hits(grown(b, gap * 0.3), old)
                        || route
                            .windows(2)
                            .filter(|s| s.iter().all(|p| p.distance(touch) > gap.max(bond * 2.)))
                            .any(|s| old.windows(2).any(|a| segment_distance(s, a) <= stroke))
                });
                if crosses_own
                    || ink.iter().any(|old| {
                        overlap(grown(b, gap * 0.5), grown(*old, gap * 0.5))
                            || hits(grown(*old, gap * 0.3), &route)
                    })
                    || paths
                        .iter()
                        .any(|old| hits(grown(b, gap * 0.3), old) || crossing(&route, old, stroke))
                {
                    continue;
                }
                best = Some((delta, b, route, cost));
            }
        }
        // Crowded drawings may have no clear local placement. Leave every
        // original placement intact rather than making a partial layout.
        let Some((delta, b, route, _)) = best else {
            return;
        };
        ink.push(b);
        paths.push(route);
        moves.push((side.ids, delta));
    }
    for (ids, delta) in moves {
        doc.translate(&ids, delta.x, delta.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::{BracketSides, GraphicStyle};

    #[test]
    fn sampled_vertices_and_collinear_overlaps_are_obstacles() {
        let p = Point::new;
        assert!(crossing(
            &[p(0., 0.), p(1., 0.), p(2., 0.)],
            &[p(1., -1.), p(1., 0.), p(1., 1.)],
            0.05
        ));
        assert!(crossing(
            &[p(0., 0.), p(4., 0.)],
            &[p(2., 0.), p(6., 0.)],
            0.05
        ));
        assert!(!crossing(
            &[p(0., 0.), p(4., 0.)],
            &[p(0., 2.), p(4., 2.)],
            1.
        ));
    }

    #[test]
    fn split_side_placement_keeps_graphics_and_the_cycle_title_clear() {
        let mut before = Document::from_json(include_bytes!(
            "../../../../tests/fixtures/cycles/bowed-insertion.rsk"
        ))
        .unwrap();
        let part: Vec<_> = (40..=85).collect();
        before.translate(&part, -234.75783, 388.1741);
        let target = drop_target(&before, &part).unwrap();
        let side = vec![(156..=159).collect(), (160..=168).collect()];
        let ink = |doc: &Document| crate::scene::selections_bounds(doc, &side);
        let mut baseline = before.clone();
        assign(&mut baseline, &target);
        let original = ink(&before);
        let placed = ink(&baseline);
        assert_ne!(placed, original);

        let mut with_shape = before.clone();
        let (lo, hi) = grown(placed[0].unwrap(), DEFAULT.world(2.));
        with_shape.graphics.push(Graphic::dragged(
            with_shape.next_id(),
            GraphicKind::Rectangle,
            lo,
            hi,
            GraphicStyle {
                fill: Some(crate::palette::Color::Ink),
                ..Default::default()
            },
            BracketSides::Both,
            false,
        ));
        assign(&mut with_shape, &target);
        assert!(
            ink(&with_shape)
                .into_iter()
                .flatten()
                .all(|b| !overlap(b, (lo, hi)))
        );

        let mut with_title = before;
        let title = vec!["Suzuki–Miyaura coupling".repeat(5); 5].join("\n");
        let g = with_title
            .graphics
            .iter_mut()
            .find(|g| g.id == target.cycle)
            .unwrap();
        g.cycle.as_mut().unwrap().label = title.clone();
        assign(&mut with_title, &target);
        let g = with_title
            .graphics
            .iter()
            .find(|g| g.id == target.cycle)
            .unwrap();
        let title = drawn(&with_title, g)
            .labels
            .into_iter()
            .find(|(_, text, _)| *text == title)
            .unwrap();
        let title = label_ink(&title);
        assert!(placed.into_iter().flatten().any(|b| overlap(b, title)));
        assert!(
            ink(&with_title)
                .into_iter()
                .flatten()
                .all(|b| !overlap(b, title))
        );
    }
}
