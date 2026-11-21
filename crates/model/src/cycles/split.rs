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
    part: Vec<u64>,
    ids: Vec<u64>,
    body: Ink,
    ink: Ink,
    incoming: bool,
    along: f32,
}

fn view(doc: &Document, id: u64, step: usize) -> Option<(Drawn, SideLayout)> {
    let g = doc.graphics.iter().find(|g| g.id == id)?;
    let mut found = None;
    let drawn = drawn_layouts(doc, g, |s, layout| {
        if s == step && layout.arc.radius > 1e-3 && layout.arc.sweep.abs() > 1e-3 {
            found = Some(layout.clone());
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
    // This repair searches circular spans. Noncircle insertion preserves
    // inherited side coordinates; native drawing reconnects them to the step.
    if n == 0 || cycle.shape != Shape::Circle {
        return None;
    }
    let i = i.min(n);
    let before = (i + n - 1) % n;
    let step = cycle.steps.get(before)?;
    if step.reagents.is_empty() && step.products.is_empty() {
        return None;
    }
    let (_, layout) = view(doc, target.cycle, before)?;
    let old = layout.arc;
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
                part: part.clone(),
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

fn path_bounds(path: &[Point]) -> Option<Ink> {
    let first = *path.first()?;
    Some(path.iter().fold((first, first), |(lo, hi), p| {
        (
            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
        )
    }))
}

fn crossing(a: &[Point], b: &[Point], clear: f32) -> bool {
    let Some((a_bounds, b_bounds)) = path_bounds(a).zip(path_bounds(b)) else {
        return false;
    };
    // Most obstacles are remote. Avoid testing every pair of sampled segments.
    if a_bounds.1.x + clear < b_bounds.0.x
        || b_bounds.1.x + clear < a_bounds.0.x
        || a_bounds.1.y + clear < b_bounds.0.y
        || b_bounds.1.y + clear < a_bounds.0.y
    {
        return false;
    }
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

struct Painted {
    arrow: Arrow,
    path: Vec<Point>,
}

struct Obstacles {
    original: Vec<Arrow>,
    original_ink: Vec<Ink>,
    own: Vec<Painted>,
    paths: Vec<Vec<Point>>,
    ink: Vec<Ink>,
    labels: Vec<Ink>,
    gap: f32,
    stroke: f32,
    join: f32,
}

impl Obstacles {
    fn clear(&self, layout: &SideLayout, batch: &[SideInk], ink: &[Ink], active: &[bool]) -> bool {
        let arrows = layout.routes(batch);
        // A degenerate lane must not shift the role-to-route correspondence.
        if arrows.len() != batch.len()
            || ink.len() != batch.len()
            || active.len() != batch.len()
            || self.original.len() != batch.len()
            || self.original_ink.len() != batch.len()
        {
            return false;
        }
        let paths: Vec<_> = arrows.iter().map(points).collect();
        for (i, (((incoming, _), b), arrow)) in batch.iter().zip(ink).zip(&arrows).enumerate() {
            let Some((enabled, (original, old_ink))) = active
                .get(i)
                .zip(self.original.get(i).zip(self.original_ink.get(i)))
            else {
                return false;
            };
            let moved = b != old_ink;
            if !enabled || (!moved && arrow == original) {
                continue;
            }
            let Some(route) = paths.get(i) else {
                return false;
            };
            let touch = if *incoming { arrow.end } else { arrow.start };
            // The join exception belongs only to the painted lane it touches.
            // In a reversible pair the opposite lane always stays an obstacle.
            let joined = self
                .own
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let distance = |a: &Painted| {
                        a.arrow
                            .arc_circle()
                            .map_or(f32::INFINITY, |(c, r, _)| (touch.distance(c) - r).abs())
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .map(|(index, _)| index);
            if self.own.iter().enumerate().any(|(j, old)| {
                (moved && hits(grown(*b, self.gap * 0.3), &old.path))
                    || if Some(j) == joined {
                        route
                            .windows(2)
                            .filter(|s| s.iter().all(|p| p.distance(touch) > self.join))
                            .any(|s| crossing(s, &old.path, self.stroke))
                    } else {
                        crossing(route, &old.path, self.stroke)
                    }
            }) || self.ink.iter().chain(&self.labels).any(|old| {
                (moved && overlap(grown(*b, self.gap * 0.5), grown(*old, self.gap * 0.5)))
                    || hits(grown(*old, self.gap * 0.3), route)
            }) || self.paths.iter().any(|old| {
                (moved && hits(grown(*b, self.gap * 0.3), old)) || crossing(route, old, self.stroke)
            }) {
                return false;
            }
            for (j, other) in ink
                .iter()
                .enumerate()
                .filter(|(j, _)| active.get(*j).copied().unwrap_or(false) && *j != i)
            {
                let Some(other_route) = paths.get(j) else {
                    return false;
                };
                if (moved && overlap(grown(*b, self.gap * 0.5), grown(*other, self.gap * 0.5)))
                    || hits(grown(*other, self.gap * 0.3), route)
                    || (moved && hits(grown(*b, self.gap * 0.3), other_route))
                    || crossing(route, other_route, self.stroke)
                {
                    return false;
                }
            }
        }
        true
    }
}

pub(super) fn apply(doc: &mut Document, mut split: Split) {
    let Some((drawn, layout)) = view(doc, split.cycle, split.step) else {
        return;
    };
    let arc = layout.arc;
    split
        .sides
        .retain(|p| projection(arc, center_of(p.body)).is_some_and(|t| t < -0.02 || t > 1.02));
    if split.sides.is_empty() {
        return;
    }
    split.sides.sort_by(|a, b| a.along.total_cmp(&b.along));
    let mut batch = layout.boxes();
    let mut active = vec![true; batch.len()];
    let mut indexed = vec![];
    for side in split.sides {
        let Some(index) = layout
            .parts
            .iter()
            .position(|p| p.incoming == side.incoming && p.ids == side.part)
        else {
            return;
        };
        let Some(enabled) = active.get_mut(index) else {
            return;
        };
        *enabled = false;
        indexed.push((index, side));
    }
    let part_ids: Vec<_> = layout
        .parts
        .iter()
        .map(|p| crate::molecule_names::include_annotations(doc, &p.ids))
        .collect();
    let Some(mut part_ink) = crate::scene::selections_bounds(doc, &part_ids)
        .into_iter()
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    // This step is checked as a whole, with each route exempting only its own
    // part. Names and linked property captions travel with their molecules.
    let held: HashSet<_> = part_ids.iter().flatten().copied().collect();
    let Some(g) = doc.graphics.iter().find(|g| g.id == split.cycle) else {
        return;
    };
    let Some(cycle) = g.cycle.as_deref() else {
        return;
    };
    let (center, radius) = g.cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    let gap = DEFAULT.world(cycle.gap_pt);
    let own = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .filter(|(_, s)| **s == Some(split.step))
        .map(|(a, _)| Painted {
            arrow: a.clone(),
            path: points(a),
        })
        .collect();
    let mut paths: Vec<_> = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .filter(|(_, s)| s.is_some_and(|s| s != split.step))
        .map(|(a, _)| points(a))
        .collect();
    // Other steps stay fixed. Moving one join on this step can change every
    // neighbour's fitted join, so its complete batch is regenerated below.
    let parts = cycle
        .parts()
        .zip(boxes(doc, cycle))
        .filter(|((r, _), b)| !matches!(r, Role::Intermediate(_)) && b.is_some());
    for (((role, _), _), a) in parts.zip(
        drawn
            .arrows
            .iter()
            .zip(&drawn.arcs)
            .filter(|(_, s)| s.is_none())
            .map(|(a, _)| a),
    ) {
        if !matches!(role, Role::Reagent(s) | Role::Product(s) if s == split.step) {
            paths.push(points(a));
        }
    }
    paths.extend(doc.arrows.iter().map(points));
    let units: Vec<_> = units(doc, &doc.all_ids())
        .into_iter()
        .map(|ids| crate::molecule_names::include_annotations(doc, &ids))
        .filter(|ids| ids.iter().all(|id| !held.contains(id)))
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
    for g in doc
        .graphics
        .iter()
        .filter(|g| g.id != split.cycle && g.cycle.is_some() && g.style.effects.opacity != Some(0.))
    {
        let other = super::drawn(doc, g);
        paths.extend(other.arrows.iter().map(points));
        ink.extend(other.labels.iter().map(label_ink));
    }
    let mut obstacles = Obstacles {
        original: layout.routes(&batch),
        original_ink: part_ink.clone(),
        own,
        paths,
        ink,
        labels: drawn.labels.iter().map(label_ink).collect(),
        gap,
        stroke: DEFAULT.world(layout.style.width_pt) * 1.5,
        join: gap.max(bond * 2.),
    };
    let mut moves = vec![];
    let mut trials = 0;
    for (index, side) in indexed {
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
        // Keep the nearest clear place on this step, with either end open.
        let (lo, hi) = (0.15, 0.85);
        let near = arc.along(from).clamp(lo, hi);
        let mut best: Option<(Point, Ink, f32)> = None;
        let Some(enabled) = active.get_mut(index) else {
            return;
        };
        *enabled = true;
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
                if best.as_ref().is_some_and(|b| cost >= b.2) {
                    continue;
                }
                // Bound work even for a crowded step with many side parts.
                // Exhausting the search preserves the entire original layout.
                if trials == 4096 {
                    return;
                }
                trials += 1;
                let delta = Point::new(to.x - from.x, to.y - from.y);
                let b = translated(side.ink, delta);
                let Some((current, current_ink)) =
                    batch.get_mut(index).zip(part_ink.get_mut(index))
                else {
                    return;
                };
                current.1 = translated(side.body, delta);
                *current_ink = b;
                if obstacles.clear(&layout, &batch, &part_ink, &active) {
                    best = Some((delta, b, cost));
                }
            }
        }
        // Crowded drawings may have no clear local placement. Leave every
        // original placement intact rather than making a partial layout.
        let Some((delta, b, _)) = best else {
            return;
        };
        let Some((current, current_ink)) = batch.get_mut(index).zip(part_ink.get_mut(index)) else {
            return;
        };
        current.1 = translated(side.body, delta);
        *current_ink = b;
        moves.push((side.ids, delta));
    }
    // Label placement depends on the resulting paths too. Re-render a planned
    // copy before committing, so a refitted step name cannot cover a new route.
    let mut planned = doc.clone();
    for (ids, delta) in &moves {
        planned.translate(ids, delta.x, delta.y);
    }
    let Some((drawn, final_layout)) = view(&planned, split.cycle, split.step) else {
        return;
    };
    let Some(final_ink) = crate::scene::selections_bounds(&planned, &part_ids)
        .into_iter()
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    obstacles.labels = drawn.labels.iter().map(label_ink).collect();
    if !obstacles.clear(&final_layout, &final_layout.boxes(), &final_ink, &active) {
        return;
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
    fn candidate_checks_the_refitted_route_of_an_unmoved_neighbour() {
        let arc = Span {
            center: Point::new(0., 0.),
            radius: 100.,
            first: 0.,
            sweep: 90.,
        };
        let layout = SideLayout::new(
            1,
            arc,
            1.,
            3.,
            &ArrowStyle::preset(Preset::Forward),
            20.,
            6.,
        );
        let ink = |t| {
            let c = arc.at(t, 20.);
            (c.offset(-2., -2.), c.offset(2., 2.))
        };
        let original = vec![(true, ink(0.35)), (false, ink(0.8))];
        let candidate = vec![(true, ink(0.35)), (false, ink(0.36))];
        let before = layout.routes(&original);
        let after = layout.routes(&candidate);
        assert_ne!(before[0], after[0], "the untouched neighbour must refit");
        let block = after[0].point(0.5);
        let obstacles = Obstacles {
            original: before,
            original_ink: original.iter().map(|(_, b)| *b).collect(),
            own: vec![],
            paths: vec![vec![block.offset(-0.1, 0.), block.offset(0.1, 0.)]],
            ink: vec![],
            labels: vec![],
            gap: 6.,
            stroke: 0.2,
            join: 40.,
        };
        let original_ink: Vec<_> = original.iter().map(|(_, b)| *b).collect();
        // Existing imperfect geometry does not block unrelated repair work.
        assert!(obstacles.clear(&layout, &original, &original_ink, &[true, true]));
        let candidate_ink: Vec<_> = candidate.iter().map(|(_, b)| *b).collect();
        // Isolate the unmoved neighbour: its new fitted path hits the obstacle.
        assert!(!obstacles.clear(&layout, &candidate, &candidate_ink, &[true, false]));
    }

    #[test]
    fn missing_rendered_lane_rejects_the_complete_batch() {
        let arc = Span {
            center: Point::new(0., 0.),
            radius: 100.,
            first: 0.,
            sweep: 90.,
        };
        let mut layout = SideLayout::new(
            1,
            arc,
            1.,
            3.,
            &ArrowStyle::preset(Preset::Forward),
            20.,
            6.,
        );
        let batch = vec![(true, (Point::new(80., 80.), Point::new(84., 84.)))];
        let obstacles = Obstacles {
            original: layout.routes(&batch),
            original_ink: batch.iter().map(|(_, b)| *b).collect(),
            own: vec![],
            paths: vec![],
            ink: vec![],
            labels: vec![],
            gap: 6.,
            stroke: 0.2,
            join: 40.,
        };
        layout.lanes.clear();
        let ink: Vec<_> = batch.iter().map(|(_, b)| *b).collect();
        assert!(!obstacles.clear(&layout, &batch, &ink, &[true]));
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
