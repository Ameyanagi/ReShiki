//! Whiteout at front/rear crossings of projected 3D cages, checked on the
//! painted scene that the canvas and SVG/PNG/PDF exports share.
use crate::{
    document::{Document, Point},
    graphics::{PathCommand, flattened},
    palette::Color,
    scene::Primitive,
};

/// Painted ink: its owner (a bond index, or `None` for black ring ink),
/// paint order, whether it is faded, and its shape.
struct Ink {
    owner: Option<usize>,
    order: usize,
    faded: bool,
    shape: Shape,
}
enum Shape {
    Band(Point, Point, f32),
    Area(Vec<Point>),
}
impl Shape {
    fn covers(&self, p: Point) -> bool {
        match self {
            Self::Band(a, b, w) => {
                let (s, n) = along(p, *a, *b);
                (0. ..=a.distance(*b)).contains(&s) && n.abs() <= w / 2.
            }
            Self::Area(points) => {
                let mut inside = false;
                for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
                    if (a.y > p.y) != (b.y > p.y)
                        && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                    {
                        inside = !inside;
                    }
                }
                inside
            }
        }
    }
}
impl Shape {
    /// Distance from `p` to this ink; 0 inside it.
    fn distance(&self, p: Point) -> f32 {
        match self {
            Self::Band(a, b, w) => {
                let (s, n) = along(p, *a, *b);
                let ds = (-s).max(s - a.distance(*b)).max(0.);
                ds.hypot((n.abs() - w / 2.).max(0.))
            }
            Self::Area(points) if self.covers(p) || points.len() < 2 => {
                if self.covers(p) {
                    0.
                } else {
                    f32::INFINITY
                }
            }
            Self::Area(points) => points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .map(|(a, b)| crate::graphics::segment_distance(p, *a, *b))
                .fold(f32::INFINITY, f32::min),
        }
    }
    /// Points on the ink, its edges included.
    fn samples(&self) -> Vec<Point> {
        match self {
            Self::Band(a, b, w) => {
                let length = a.distance(*b).max(1e-6);
                let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
                let steps = (length / 0.2).ceil() as usize;
                (0..=steps)
                    .flat_map(|k| {
                        let s = length * k as f32 / steps as f32;
                        [-0.5, 0., 0.5].map(|o| a.offset(ux * s - uy * o * w, uy * s + ux * o * w))
                    })
                    .collect()
            }
            Self::Area(points) => points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .flat_map(|(a, b)| {
                    (0..8).map(move |k| {
                        let t = k as f32 / 8.;
                        a.offset((b.x - a.x) * t, (b.y - a.y) * t)
                    })
                })
                .collect(),
        }
    }
}
/// Along-axis and normal coordinates of `p` relative to segment `a`→`b`.
fn along(p: Point, a: Point, b: Point) -> (f32, f32) {
    let length = a.distance(b).max(1e-6);
    let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
    let (dx, dy) = (p.x - a.x, p.y - a.y);
    (dx * ux + dy * uy, -dx * uy + dy * ux)
}

/// Give every bond a distinct color so painted ink identifies its bond.
fn tag(doc: &mut Document) {
    for (i, bond) in doc.bonds.iter_mut().enumerate() {
        let i = i + 1;
        bond.color = Color::Custom([1, (i >> 8) as u8, i as u8]);
    }
}
fn owner(color: Color) -> Option<usize> {
    match color {
        Color::Custom([1, hi, lo]) => Some(((hi as usize) << 8 | lo as usize) - 1),
        _ => None,
    }
}
fn inks(primitives: &[Primitive]) -> Vec<Ink> {
    fn push(primitive: &Primitive, order: usize, faded: bool, out: &mut Vec<Ink>) {
        match primitive {
            Primitive::Opacity { alpha, primitive } => push(primitive, order, *alpha < 1., out),
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                // Ring circles and arcs are curves that take their ring's
                // bond color; only straight bond ink identifies a bond.
                let curve = commands.iter().any(|c| matches!(c, PathCommand::Cubic(..)));
                let owner = owner(style.stroke).filter(|_| !curve);
                for path in flattened(commands) {
                    if *filled {
                        out.push(Ink {
                            owner,
                            order,
                            faded,
                            shape: Shape::Area(path),
                        });
                    } else {
                        out.extend(
                            path.windows(2)
                                .filter(|pair| pair[0].distance(pair[1]) > 1e-4)
                                .map(|pair| Ink {
                                    owner,
                                    order,
                                    faded,
                                    shape: Shape::Band(pair[0], pair[1], style.width()),
                                }),
                        );
                    }
                }
            }
            Primitive::Line(a, b, w) => out.push(Ink {
                owner: None,
                order,
                faded,
                shape: Shape::Band(*a, *b, *w),
            }),
            Primitive::Polygon(points) => out.push(Ink {
                owner: None,
                order,
                faded,
                shape: Shape::Area(points.clone()),
            }),
            _ => {}
        }
    }
    let mut out = vec![];
    for (order, primitive) in primitives.iter().enumerate() {
        push(primitive, order, false, &mut out);
    }
    out
}

/// Rear ink may approach front ink only as a faded base painted beneath it;
/// opaque rear ink keeps a visible whiteout gap. Points near `skip` (the
/// rear bond's own atoms, where its junction lies) are left out.
fn rear_touches(
    front: &[&Ink],
    rear: &[&Ink],
    at: Point,
    radius: f32,
    skip: &[Point],
) -> Option<Point> {
    let clearance = crate::style::DEFAULT.world(0.5);
    let step = 0.2;
    let steps = (radius / step) as i32;
    for ix in -steps..=steps {
        for iy in -steps..=steps {
            let p = at.offset(ix as f32 * step, iy as f32 * step);
            if p.distance(at) > radius || skip.iter().any(|q| p.distance(*q) < 4.) {
                continue;
            }
            for back in rear.iter().filter(|ink| ink.shape.covers(p)) {
                let touched = front.iter().any(|ink| {
                    (ink.shape.covers(p)
                        || (0..8).any(|k| {
                            let angle = k as f32 * std::f32::consts::FRAC_PI_4;
                            ink.shape
                                .covers(p.offset(clearance * angle.cos(), clearance * angle.sin()))
                        }))
                        && !(back.faded && back.order < ink.order)
                });
                if touched {
                    return Some(p);
                }
            }
        }
    }
    None
}
/// Every rail of the front ink that runs on both sides of the crossing also
/// runs through it, and at least one rail does.
fn front_continuous(front: &[&Ink], a: Point, b: Point, at: Point) -> bool {
    let length = a.distance(b);
    let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
    let (s, n) = along(at, a, b);
    // Inked on both sides, short of the label-free atom ends.
    let reach = 6_f32.min(s - 1.).min(length - s - 1.);
    let point = |s: f32, o: f32| a.offset(ux * s - uy * o, uy * s + ux * o);
    // Rails, not the dashes of an aromatic inner line (a tenth of the bond).
    let span = |ink: &&&Ink| {
        let points = match &ink.shape {
            Shape::Band(p, q, _) => vec![*p, *q],
            Shape::Area(points) => points.clone(),
        };
        let along: Vec<f32> = points.iter().map(|p| along(*p, a, b).0).collect();
        along.iter().copied().fold(f32::MIN, f32::max)
            - along.iter().copied().fold(f32::MAX, f32::min)
    };
    let long: Vec<_> = front
        .iter()
        .filter(|ink| span(ink) > 0.3 * length)
        .collect();
    let inked = |p: Point| long.iter().any(|ink| ink.shape.covers(p));
    let mut rails = 0;
    for k in -40..=40 {
        let o = n + k as f32 * 0.29;
        if !inked(point(s - reach, o)) || !inked(point(s + reach, o)) {
            continue;
        }
        rails += 1;
        if !(-30..=30).all(|i| inked(point(s + i as f32 * reach / 30., o))) {
            return false;
        }
    }
    rails > 0
}

/// Centerline crossing of two segments, with both parameters and the sine
/// of their angle.
fn crossing(a: Point, b: Point, c: Point, d: Point) -> Option<(f32, f32, f32)> {
    let ab = Point::new(b.x - a.x, b.y - a.y);
    let cd = Point::new(d.x - c.x, d.y - c.y);
    let ac = Point::new(c.x - a.x, c.y - a.y);
    let det = ab.x * cd.y - ab.y * cd.x;
    let sine = det.abs() / (a.distance(b) * c.distance(d)).max(1e-6);
    // Nearly collinear projections overlap rather than cross.
    if sine < 0.25 {
        return None;
    }
    Some((
        (ac.x * cd.y - ac.y * cd.x) / det,
        (ac.x * ab.y - ac.y * ab.x) / det,
        sine,
    ))
}
fn ends(doc: &Document, bond: usize) -> (Point, Point, f32, f32) {
    let bond = &doc.bonds[bond];
    let a = doc.atom(bond.a).unwrap();
    let b = doc.atom(bond.b).unwrap();
    (a.position, b.position, a.depth, b.depth)
}

/// Checks every front/rear bond crossing and every bond crossing a ring
/// circle away from shared atoms. Returns the number of crossings checked.
fn assert_whiteout(doc: &Document, label: &str) -> usize {
    let inks = inks(&crate::scene::primitives(doc));
    // Rear opacity hides or fades whole cage surfaces, including bonds that
    // lie in front of another rear bond; only opaque front ink must be whole.
    let paint = crate::rear_opacity::Paint::new(doc);
    let opaque = |bond: usize, t: f32| paint.bond(&doc.bonds[bond], t) >= 1.;
    let of = |bond: usize| -> Vec<&Ink> { inks.iter().filter(|i| i.owner == Some(bond)).collect() };
    let mut failures = vec![];
    let mut checked = 0;
    for i in 0..doc.bonds.len() {
        for j in i + 1..doc.bonds.len() {
            let (a, b, za, zb) = ends(doc, i);
            let (c, d, zc, zd) = ends(doc, j);
            let Some((t, u, _)) = crossing(a, b, c, d) else {
                continue;
            };
            if !(0.2..0.8).contains(&t) || !(0.2..0.8).contains(&u) {
                continue;
            }
            checked += 1;
            let at = a.offset((b.x - a.x) * t, (b.y - a.y) * t);
            let (front, rear, fa, fb, ft, rear_ends) = if za + (zb - za) * t > zc + (zd - zc) * u {
                (i, j, a, b, t, [c, d])
            } else {
                (j, i, c, d, u, [a, b])
            };
            let (front_ink, rear_ink) = (of(front), of(rear));
            if let Some(p) = rear_touches(&front_ink, &rear_ink, at, 15., &rear_ends) {
                failures.push(format!(
                    "rear bond {rear} (order {}) touches front bond {front} (order {}) at {p:?}",
                    doc.bonds[rear].order, doc.bonds[front].order
                ));
            }
            if opaque(front, ft) && !front_continuous(&front_ink, fa, fb, at) {
                failures.push(format!(
                    "front bond {front} (order {}) is cut over bond {rear} at {at:?}",
                    doc.bonds[front].order
                ));
            }
        }
    }
    for circle in crate::aromatic::circles(doc) {
        let depth = circle
            .atoms
            .iter()
            .map(|id| doc.atom(*id).unwrap().depth)
            .sum::<f32>()
            / circle.atoms.len() as f32;
        let curve: Vec<_> = circle
            .graphic()
            .parts()
            .into_iter()
            .flat_map(|part| flattened(&part.commands))
            .collect();
        // This circle's ink: curve pieces lying on its own outline.
        let (lo, hi) = curve.iter().flatten().fold(
            (
                Point::new(f32::MAX, f32::MAX),
                Point::new(f32::MIN, f32::MIN),
            ),
            |(lo, hi), p| {
                (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            },
        );
        let on_curve = |p: Point| {
            (lo.x - 0.5..=hi.x + 0.5).contains(&p.x)
                && (lo.y - 0.5..=hi.y + 0.5).contains(&p.y)
                && curve.iter().flat_map(|path| path.windows(2)).any(|edge| {
                    let (s, n) = along(p, edge[0], edge[1]);
                    n.abs() < 0.3 && (-0.3..=edge[0].distance(edge[1]) + 0.3).contains(&s)
                })
        };
        let ring: Vec<&Ink> = inks
            .iter()
            .filter(|ink| {
                ink.owner.is_none()
                    && matches!(ink.shape, Shape::Band(a, b, _) if on_curve(a) && on_curve(b))
            })
            .collect();
        for (index, bond) in doc.bonds.iter().enumerate() {
            if circle.atoms.contains(&bond.a) || circle.atoms.contains(&bond.b) {
                continue;
            }
            let (a, b, za, zb) = ends(doc, index);
            for edge in curve.iter().flat_map(|path| path.windows(2)) {
                let Some((t, u, _)) = crossing(a, b, edge[0], edge[1]) else {
                    continue;
                };
                // Compare clearly separated cage layers only.
                let z = za + (zb - za) * t;
                if !(0.2..0.8).contains(&t) || !(0. ..=1.).contains(&u) || (z - depth).abs() < 20. {
                    continue;
                }
                checked += 1;
                let at = a.offset((b.x - a.x) * t, (b.y - a.y) * t);
                let bond_ink = of(index);
                let (front, rear, skip) = if z > depth {
                    (&bond_ink, &ring, vec![])
                } else {
                    (&ring, &bond_ink, vec![a, b])
                };
                if let Some(p) = rear_touches(front, rear, at, 6., &skip) {
                    failures.push(format!("bond {index} and a ring circle touch at {p:?}"));
                }
                if z > depth && opaque(index, t) && !front_continuous(&bond_ink, a, b, at) {
                    failures.push(format!(
                        "front bond {index} is cut by a rear circle at {at:?}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{label}: {} failures in {checked} crossings:\n{}",
        failures.len(),
        failures.join("\n")
    );
    checked
}

/// Depth of `bond` at the point of its centerline nearest `p`.
fn depth_near(doc: &Document, bond: usize, p: Point) -> f32 {
    let (a, b, za, zb) = ends(doc, bond);
    let t = (along(p, a, b).0 / a.distance(b).max(1e-6)).clamp(0., 1.);
    za + (zb - za) * t
}

/// Occlusion: wherever two bonds that share no atom come close, no opaque
/// ink of the one behind (by XYZ depth there) lies within 0.5 pt of the ink
/// of the one in front, at any angle and next to any atom. Faded rear ink
/// painted beneath the front ink is allowed. Returns the close pairs checked.
fn assert_occluded(doc: &Document, label: &str) -> usize {
    let mut inks = inks(&crate::scene::primitives(doc));
    // A multi-colored atom paints its underlay junction in one incident
    // bond's color; give each filled piece to the incident bond whose
    // centerline it hugs.
    for ink in &mut inks {
        if let (Some(owner), Shape::Area(points)) = (ink.owner, &ink.shape) {
            let atoms = [doc.bonds[owner].a, doc.bonds[owner].b];
            let distance = |bond: usize| {
                let (a, b, ..) = ends(doc, bond);
                points
                    .iter()
                    .map(|p| crate::graphics::segment_distance(*p, a, b))
                    .fold(0., f32::max)
            };
            // Nearly collinear neighbors hug each other's centerlines too;
            // only a clearly closer bond takes the piece.
            ink.owner = (0..doc.bonds.len())
                .filter(|i| {
                    let bond = &doc.bonds[*i];
                    atoms.contains(&bond.a) || atoms.contains(&bond.b)
                })
                .map(|i| (i, distance(i) + if i == owner { 0. } else { 0.5 }))
                .min_by(|x, y| x.1.total_cmp(&y.1))
                .map(|(i, _)| i);
        }
    }
    let clearance = crate::style::DEFAULT.world(0.5);
    let style = &doc.drawing_style;
    let reach = 2. * style.bond_length_world * style.bond_spacing_ratio
        + style.world(style.bold_width_pt)
        + 1.;
    let of = |bond: usize| -> Vec<&Ink> { inks.iter().filter(|i| i.owner == Some(bond)).collect() };
    let mut failures = vec![];
    let mut checked = 0;
    for (r, rear_bond) in doc.bonds.iter().enumerate() {
        let (a, b, ..) = ends(doc, r);
        let rear = of(r);
        for (f, front_bond) in doc.bonds.iter().enumerate() {
            let (c, d, ..) = ends(doc, f);
            if f == r
                || [front_bond.a, front_bond.b]
                    .iter()
                    .any(|id| *id == rear_bond.a || *id == rear_bond.b)
                || crate::graphics::segment_distance(a, c, d)
                    .min(crate::graphics::segment_distance(b, c, d))
                    .min(crate::graphics::segment_distance(c, a, b))
                    .min(crate::graphics::segment_distance(d, a, b))
                    > reach
                    && crossing(a, b, c, d).is_none_or(|(t, u, _)| {
                        !(0. ..=1.).contains(&t) || !(0. ..=1.).contains(&u)
                    })
            {
                continue;
            }
            checked += 1;
            let front = of(f);
            let hit = rear.iter().find_map(|back| {
                back.shape.samples().into_iter().find(|p| {
                    depth_near(doc, r, *p) < depth_near(doc, f, *p) - 0.001
                        && front.iter().any(|ink| {
                            ink.shape.distance(*p) < clearance
                                && !(back.faded && back.order < ink.order)
                        })
                })
            });
            if let Some(p) = hit {
                failures.push(format!(
                    "rear bond {r} (order {}) touches front bond {f} (order {}) at {p:?}",
                    rear_bond.order, front_bond.order
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{label}: {} failures in {checked} close pairs:\n{}",
        failures.len(),
        failures.join("\n")
    );
    checked
}

/// Every stroke piece a bond keeps is at least one stroke width long: no
/// dots or stubs are left over from occlusion.
fn assert_no_fragments(doc: &Document, label: &str) {
    fn walk(primitive: &Primitive, out: &mut Vec<(usize, f32, f32)>) {
        match primitive {
            Primitive::Opacity { primitive, .. } => walk(primitive, out),
            Primitive::Path {
                commands,
                style,
                filled: false,
            } if !commands.iter().any(|c| matches!(c, PathCommand::Cubic(..))) => {
                if let Some(bond) = owner(style.stroke) {
                    for path in flattened(commands) {
                        let length = path.windows(2).map(|p| p[0].distance(p[1])).sum();
                        out.push((bond, length, style.width()));
                    }
                }
            }
            _ => {}
        }
    }
    let mut pieces = vec![];
    for primitive in crate::scene::primitives(doc) {
        walk(&primitive, &mut pieces);
    }
    // An aromatic inner line's dashes are a tenth of a bond, however short.
    let dash = |bond: usize, length: f32| {
        let (a, b, ..) = ends(doc, bond);
        doc.bonds[bond].order == 4 && (length - a.distance(b) * 0.1).abs() < 0.01
    };
    let short: Vec<_> = pieces
        .iter()
        .filter(|(bond, length, width)| *length < width * 0.99 && !dash(*bond, *length))
        .map(|(bond, length, _)| format!("bond {bond}: {length}"))
        .collect();
    assert!(short.is_empty(), "{label}: short fragments {short:?}");
}

#[test]
fn c60_rear_ink_is_occluded_by_front_ink_in_every_view() {
    type Edit = fn(&mut Document);
    let edits: [(&str, Edit); 3] = [
        ("kekule", |_| {}),
        ("aromatic", |doc| {
            doc.bonds.iter_mut().for_each(|b| b.order = 4)
        }),
        ("inner lines", |doc| {
            for bond in doc.bonds.iter_mut().filter(|b| b.order == 2) {
                bond.order = 4;
            }
        }),
    ];
    for (name, edit) in edits {
        let checked: usize = views(edit)
            .map(|(doc, label)| {
                let label = format!("{name} {label}");
                let checked = assert_occluded(&doc, &label);
                assert_no_fragments(&doc, &label);
                checked
            })
            .sum();
        assert!(checked > 100, "{name}: only {checked} close pairs");
    }
}

#[test]
fn a_nearly_parallel_rear_bond_is_hidden_behind_a_front_double_bond() {
    // A rear single bond runs 5° off a front double bond, just behind it,
    // never crossing its centerline, and reaches past its ends.
    let mut doc = Document::default();
    let (sin, cos) = 5_f32.to_radians().sin_cos();
    let atoms: Vec<_> = [
        (Point::new(-21., 0.), 20.),
        (Point::new(21., 0.), 20.),
        (Point::new(-40. * cos, 4. - 40. * sin), -20.),
        (Point::new(40. * cos, 4. + 40. * sin), -20.),
    ]
    .into_iter()
    .map(|(p, z)| {
        let id = doc.add_atom("C", p);
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(atoms[0], atoms[1], 2, "plain");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    for position in [
        crate::bonds::DoublePosition::Left,
        crate::bonds::DoublePosition::Right,
        crate::bonds::DoublePosition::Center,
    ] {
        doc.bonds[0].double_position = position;
        let mut tagged = doc.clone();
        tag(&mut tagged);
        let label = format!("{position:?}");
        assert_eq!(assert_occluded(&tagged, &label), 2);
        assert_no_fragments(&tagged, &label);
        let inks = inks(&crate::scene::primitives(&tagged));
        let rear: Vec<_> = inks.iter().filter(|i| i.owner == Some(1)).collect();
        let front: Vec<_> = inks.iter().filter(|i| i.owner == Some(0)).collect();
        // Fully hidden where it overlaps; visible beyond the front bond.
        for x in [-20., -10., 0., 8.] {
            let p = Point::new(x, 4. + x * sin / cos);
            assert!(
                !rear.iter().any(|i| i.shape.covers(p)),
                "{label}: rear ink at {p:?}"
            );
        }
        for x in [-32., 32.] {
            let p = Point::new(x, 4. + x * sin / cos);
            assert!(
                rear.iter().any(|i| i.shape.covers(p)),
                "{label}: no rear ink at {p:?}"
            );
        }
        let (from, to) = (Point::new(-21., 0.), Point::new(21., 0.));
        assert!(front_continuous(&front, from, to, Point::new(0., 0.)));
    }
}

#[test]
fn parallel_bonds_tied_in_depth_at_one_end_still_occlude() {
    // B lies in front of A except at their right ends, where depth ties.
    let mut doc = Document::default();
    let atoms: Vec<_> = [
        (Point::new(0., 0.), 0.),
        (Point::new(42., 0.), 42.),
        (Point::new(0., 1.), 42.),
        (Point::new(42., 1.), 42.),
    ]
    .into_iter()
    .map(|(p, z)| {
        let id = doc.add_atom("C", p);
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(atoms[0], atoms[1], 1, "plain");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    assert!(!super::gaps(&doc)[0].is_empty());
    assert!(super::gaps(&doc)[1].is_empty());
    tag(&mut doc);
    assert_eq!(assert_occluded(&doc, "tied"), 2);
    assert_no_fragments(&doc, "tied");
}

/// A whiteout clears only front ink that is there: a single bond keeps its
/// ink wherever no other bond's rails and clearance reach, even at a shallow
/// crossing.
fn assert_cuts_stay_near_front_ink(doc: &Document, label: &str) {
    let inks = inks(&crate::scene::primitives(doc));
    let style = &doc.drawing_style;
    let reach = style.bond_length_world * style.bond_spacing_ratio
        + style.line_width()
        + crate::style::DEFAULT.world(1.1)
        + 1.;
    for (index, bond) in doc.bonds.iter().enumerate().filter(|(_, b)| b.order == 1) {
        let (a, b, ..) = ends(doc, index);
        for k in 3..=17 {
            let p = a.offset((b.x - a.x) * k as f32 / 20., (b.y - a.y) * k as f32 / 20.);
            let clear = doc.bonds.iter().enumerate().all(|(other, o)| {
                let (c, d, ..) = ends(doc, other);
                other == index
                    || [o.a, o.b].iter().any(|id| *id == bond.a || *id == bond.b)
                    || crate::graphics::segment_distance(p, c, d) > reach
            });
            assert!(
                !clear
                    || inks
                        .iter()
                        .any(|ink| ink.owner == Some(index) && ink.shape.covers(p)),
                "{label}: bond {index} lost ink at {p:?}, away from every front bond"
            );
        }
    }
}

fn c60() -> Document {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-100.rsk"
    ))
    .unwrap()
}

const VIEWS: [(f32, f32); 6] = [
    (0., 0.),
    (20., 0.),
    (-40., 25.),
    (60., -50.),
    (15., 70.),
    (-75., -30.),
];

/// The C60 view in each tilt, with tagged bonds and `edit` applied.
fn views(edit: impl Fn(&mut Document)) -> impl Iterator<Item = (Document, String)> {
    VIEWS.into_iter().map(move |(x, y)| {
        let mut doc = c60();
        let ids = doc.all_ids();
        crate::projection::tilt(&mut doc, &ids, x, true);
        crate::projection::tilt(&mut doc, &ids, y, false);
        edit(&mut doc);
        tag(&mut doc);
        (doc, format!("tilt {x}°/{y}°"))
    })
}

#[test]
fn c60_double_bonds_stay_continuous_over_cut_rear_bonds_in_every_view() {
    let checked: usize = views(|_| {})
        .map(|(doc, label)| {
            assert_cuts_stay_near_front_ink(&doc, &label);
            assert_whiteout(&doc, &label)
        })
        .sum();
    assert!(checked > 40, "only {checked} crossings");
}

#[test]
fn c60_aromatic_circles_and_inner_lines_keep_front_ink_whole() {
    // Every bond aromatic: each face draws a ring circle.
    let circles: usize = views(|doc| doc.bonds.iter_mut().for_each(|b| b.order = 4))
        .map(|(doc, label)| assert_whiteout(&doc, &format!("circles {label}")))
        .sum();
    // Aromatic doubles beside single bonds: no circle, so inner lines.
    let lines: usize = views(|doc| {
        for bond in doc.bonds.iter_mut().filter(|b| b.order == 2) {
            bond.order = 4;
        }
    })
    .map(|(doc, label)| assert_whiteout(&doc, &format!("inner lines {label}")))
    .sum();
    assert!(
        circles > 40 && lines > 40,
        "{circles} and {lines} crossings"
    );
}

#[test]
fn c60_faded_rear_ink_is_painted_beneath_front_bonds() {
    for alpha in [0., 0.25] {
        let checked: usize = views(|doc| {
            let ids = doc.all_ids();
            crate::depth_appearance::set_rear_opacity(doc, &ids, alpha).unwrap();
        })
        .map(|(doc, label)| assert_whiteout(&doc, &format!("{alpha} {label}")))
        .sum();
        assert!(checked > 40, "only {checked} crossings");
    }
}

#[test]
fn rear_double_bond_rails_clear_a_front_bond_at_an_oblique_crossing() {
    // A foreshortened rear double bond crosses a front single bond at 40°.
    let mut doc = Document::default();
    let angle = 40_f32.to_radians();
    let atoms: Vec<_> = [
        (Point::new(-30., 0.), 20.),
        (Point::new(30., 0.), 20.),
        (Point::new(-12. * angle.cos(), -12. * angle.sin()), -20.),
        (Point::new(12. * angle.cos(), 12. * angle.sin()), -20.),
    ]
    .into_iter()
    .map(|(p, z)| {
        let id = doc.add_atom("C", p);
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(atoms[0], atoms[1], 1, "plain");
    doc.add_bond(atoms[2], atoms[3], 2, "plain");
    for position in [
        crate::bonds::DoublePosition::Left,
        crate::bonds::DoublePosition::Right,
        crate::bonds::DoublePosition::Center,
    ] {
        doc.bonds[1].double_position = position;
        let mut tagged = doc.clone();
        tag(&mut tagged);
        assert_eq!(assert_whiteout(&tagged, &format!("{position:?}")), 1);
        // The front bond keeps its rails; the whiteout is in the rear bond.
        assert!(super::gaps(&doc)[0].is_empty());
        assert!(!super::gaps(&doc)[1].is_empty());
        // Swapping depth swaps the cut.
        let mut swapped = tagged.clone();
        for atom in &mut swapped.atoms {
            atom.depth = -atom.depth;
        }
        assert_whiteout(&swapped, &format!("swapped {position:?}"));
    }
}

#[test]
fn a_ring_circle_cut_behind_keeps_the_same_bond_in_front_elsewhere() {
    // A tilted double bond passes behind one side of a flat aromatic circle
    // and in front of the other, both at a shallow angle.
    let mut doc = Document::default();
    let ring: Vec<_> = (0..6)
        .map(|k| {
            let angle = (k as f32 * 60.).to_radians();
            doc.add_atom(
                "C",
                Point::new(72.883_6 * angle.cos(), 72.883_6 * angle.sin()),
            )
        })
        .collect();
    for k in 0..6 {
        doc.add_bond(ring[k], ring[(k + 1) % 6], 4, "plain");
    }
    let a = doc.add_atom("C", Point::new(-21., 49.5));
    let b = doc.add_atom("C", Point::new(21., 49.5));
    doc.atom_mut(a).unwrap().depth = -20.;
    doc.atom_mut(b).unwrap().depth = 20.;
    doc.add_bond(a, b, 2, "plain");
    doc.bonds[6].double_position = crate::bonds::DoublePosition::Center;
    tag(&mut doc);
    let inks = inks(&crate::scene::primitives(&doc));
    let bond: Vec<&Ink> = inks.iter().filter(|i| i.owner == Some(6)).collect();
    let circle: Vec<&Ink> = inks.iter().filter(|i| i.owner.is_none()).collect();
    let (from, to) = (Point::new(-21., 49.5), Point::new(21., 49.5));
    // The circle runs between the rails near the top, so the bond is cut
    // where it lies behind (x < 0) and stays whole where it is in front.
    let at = Point::new(8., 49.5);
    assert!(front_continuous(&bond, from, to, at));
    assert_eq!(rear_touches(&bond, &circle, at, 6., &[]), None);
    let at = Point::new(-8., 49.5);
    assert_eq!(rear_touches(&circle, &bond, at, 6., &[from, to]), None);
}

#[test]
fn crossed_rear_double_strokes_clear_a_front_bond() {
    // Wavy doubles draw two diagonal strokes, not parallel rails.
    let mut doc = Document::default();
    let (sin, cos) = 40_f32.to_radians().sin_cos();
    let atoms: Vec<_> = [
        (Point::new(-30., 0.), -20.),
        (Point::new(30., 0.), -20.),
        (Point::new(-15. - 30. * cos, -30. * sin), 20.),
        (Point::new(-15. + 30. * cos, 30. * sin), 20.),
    ]
    .into_iter()
    .map(|(p, z)| {
        let id = doc.add_atom("C", p);
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(atoms[0], atoms[1], 2, "wavy");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    tag(&mut doc);
    assert_eq!(assert_whiteout(&doc, "wavy"), 1);
}

#[test]
fn side_positioned_crossed_doubles_cut_and_are_cut_like_centered_ones() {
    // Crossed doubles draw their diagonals between centered offsets
    // whatever their double position.
    for (crossed, single) in [(20., -20.), (-20., 20.)] {
        let mut doc = Document::default();
        let atoms: Vec<_> = [
            (Point::new(-30., 0.), crossed),
            (Point::new(30., 0.), crossed),
            (Point::new(-15., -30.), single),
            (Point::new(-15., 30.), single),
        ]
        .into_iter()
        .map(|(p, z)| {
            let id = doc.add_atom("C", p);
            doc.atom_mut(id).unwrap().depth = z;
            id
        })
        .collect();
        doc.add_bond(atoms[0], atoms[1], 2, "wavy");
        doc.add_bond(atoms[2], atoms[3], 1, "plain");
        tag(&mut doc);
        let centered = format!("{:?}", super::gaps(&doc));
        for position in [
            crate::bonds::DoublePosition::Left,
            crate::bonds::DoublePosition::Right,
        ] {
            doc.bonds[0].double_position = position;
            let label = format!("crossed at depth {crossed}, {position:?}");
            assert_eq!(format!("{:?}", super::gaps(&doc)), centered, "{label}");
            assert_eq!(assert_occluded(&doc, &label), 2);
        }
    }
}

/// Atoms at the given positions and depths.
fn atoms_at(doc: &mut Document, atoms: &[(Point, f32)]) -> Vec<u64> {
    atoms
        .iter()
        .map(|(p, z)| {
            let id = doc.add_atom("C", *p);
            doc.atom_mut(id).unwrap().depth = *z;
            id
        })
        .collect()
}

#[test]
fn widely_spaced_crossed_rear_strokes_are_cut_where_they_cross_the_front_bond() {
    // At wide spacing the diagonals cross the front bond away from where
    // parallel rails at their end offsets would.
    let mut doc = Document::default();
    doc.drawing_style.bond_spacing_ratio = 0.4;
    let (sin, cos) = 40_f32.to_radians().sin_cos();
    let atoms = atoms_at(
        &mut doc,
        &[
            (Point::new(-30., 0.), -20.),
            (Point::new(30., 0.), -20.),
            (Point::new(-30. * cos, -30. * sin), 20.),
            (Point::new(30. * cos, 30. * sin), 20.),
        ],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "wavy");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    tag(&mut doc);
    assert_eq!(assert_occluded(&doc, "wide crossed"), 2);
}

#[test]
fn a_crossed_rear_stroke_under_a_front_bond_leaves_the_other_stroke() {
    let mut doc = Document::default();
    doc.drawing_style.bond_spacing_ratio = 0.4;
    let half = doc.drawing_style.bond_length_world * 0.2;
    let atoms = atoms_at(
        &mut doc,
        &[
            (Point::new(-30., 0.), -20.),
            (Point::new(30., 0.), -20.),
            (Point::new(-30., -half), 20.),
            (Point::new(30., half), 20.),
        ],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "wavy");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    tag(&mut doc);
    assert_eq!(assert_occluded(&doc, "crossed under"), 2);
    let inks = inks(&crate::scene::primitives(&doc));
    // On the stroke from (-30, half) to (30, -half), clear of the front bond.
    let p = Point::new(-20., half * 2. / 3.);
    assert!(
        inks.iter().any(|i| i.owner == Some(0) && i.shape.covers(p)),
        "no rear ink at {p:?}"
    );
}

#[test]
fn a_cut_crossed_stroke_keeps_ink_clear_of_a_second_front_bond() {
    // After the first cut, a piece of one diagonal near where the diagonals
    // cross must not take the other diagonal's gap.
    let mut doc = Document::default();
    doc.drawing_style.bond_spacing_ratio = 0.4;
    let atoms = atoms_at(
        &mut doc,
        &[
            (Point::new(-30., 0.), -20.),
            (Point::new(30., 0.), -20.),
            (Point::new(-10., -30.), 20.),
            (Point::new(-10., 30.), 20.),
            (Point::new(0., -15.), 20.),
            (Point::new(30., 15.), 20.),
        ],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "wavy");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    doc.add_bond(atoms[4], atoms[5], 1, "plain");
    tag(&mut doc);
    assert_eq!(assert_occluded(&doc, "two fronts"), 6);
    let half = doc.drawing_style.bond_length_world * 0.2;
    let p = Point::new(8., 8. * half / 30.);
    assert!(
        inks(&crate::scene::primitives(&doc))
            .iter()
            .any(|i| i.owner == Some(0) && i.shape.covers(p)),
        "no rear ink at {p:?}"
    );
}

#[test]
fn a_crossed_rear_bond_behind_a_ring_circle_is_cut_where_its_strokes_cross_it() {
    let mut doc = Document::default();
    doc.drawing_style.bond_spacing_ratio = 0.4;
    let ring: Vec<_> = (0..6)
        .map(|i| {
            let angle = (i as f32 * 60.).to_radians();
            (Point::new(42. * angle.cos(), 42. * angle.sin()), 0.)
        })
        .collect();
    let ring = atoms_at(&mut doc, &ring);
    for i in 0..6 {
        doc.add_bond(ring[i], ring[(i + 1) % 6], 4, "plain");
    }
    let atoms = atoms_at(
        &mut doc,
        &[(Point::new(-30., 15.), -20.), (Point::new(30., 15.), -20.)],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "wavy");
    tag(&mut doc);
    let inks = inks(&crate::scene::primitives(&doc));
    let circle: Vec<_> = inks.iter().filter(|i| i.owner.is_none()).collect();
    assert!(!circle.is_empty(), "no ring circle");
    let clearance = crate::style::DEFAULT.world(0.5);
    let hit = inks.iter().filter(|i| i.owner == Some(6)).find_map(|rear| {
        rear.shape
            .samples()
            .into_iter()
            .find(|p| circle.iter().any(|c| c.shape.distance(*p) < clearance))
    });
    assert_eq!(hit, None, "rear crossed stroke touches the ring circle");
}

#[test]
fn a_rear_bond_is_hidden_between_widely_spaced_front_rails() {
    let mut doc = Document::default();
    doc.drawing_style.bond_spacing_ratio = 0.4;
    let atoms = atoms_at(
        &mut doc,
        &[
            (Point::new(-30., 0.), 20.),
            (Point::new(30., 0.), 20.),
            (Point::new(0., -30.), -20.),
            (Point::new(0., 30.), -20.),
        ],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "plain");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    doc.bonds[0].double_position = crate::bonds::DoublePosition::Center;
    tag(&mut doc);
    let inks = inks(&crate::scene::primitives(&doc));
    assert!(
        !inks
            .iter()
            .any(|i| i.owner == Some(1) && i.shape.covers(Point::new(0., 0.))),
        "rear ink between the front rails"
    );
}

#[test]
fn an_omitted_inner_rail_hides_nothing() {
    // A short positioned double draws no inner rail; a rear bond where that
    // rail would be, beyond the drawn rail's reach, stays whole.
    let mut doc = Document::default();
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    let atoms = atoms_at(
        &mut doc,
        &[
            (Point::new(-4., 0.), 20.),
            (Point::new(4., 0.), 20.),
            (Point::new(-20., spacing), -20.),
            (Point::new(20., spacing), -20.),
        ],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "plain");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    doc.bonds[0].double_position = crate::bonds::DoublePosition::Right;
    assert!(super::gaps(&doc)[1].is_empty());
}

#[test]
fn a_thin_inner_rail_keeps_its_own_width() {
    // A bold double with a plain inner rail clears and prunes that rail by
    // its plain width.
    let mut doc = Document::default();
    let atoms = atoms_at(
        &mut doc,
        &[
            (Point::new(-30., 0.), -20.),
            (Point::new(30., 0.), -20.),
            (Point::new(15., -30.), 20.),
            (Point::new(15., 30.), 20.),
        ],
    );
    doc.add_bond(atoms[0], atoms[1], 2, "bold");
    doc.add_bond(atoms[2], atoms[3], 1, "plain");
    doc.bonds[0].double_position = crate::bonds::DoublePosition::Right;
    doc.bonds[0].secondary_display = Some("plain".into());
    let style = &doc.drawing_style;
    let (thin, bold) = (style.line_width(), style.world(style.bold_width_pt));
    let mut widths: Vec<_> = super::gaps(&doc)[0].iter().map(|g| g.min).collect();
    widths.sort_by(f32::total_cmp);
    assert_eq!(widths, [thin, bold]);
    tag(&mut doc);
    assert_eq!(assert_occluded(&doc, "thin inner rail"), 2);
}

#[test]
fn a_nearly_edge_on_warped_ring_keeps_its_depth_gradient() {
    // A slightly warped aromatic ring seen almost edge-on: its circle lies
    // far behind a bond that crosses it.
    let mut doc = Document::default();
    let ring: Vec<_> = (0..6)
        .map(|k| {
            let angle = k as f32 * std::f32::consts::FRAC_PI_3;
            let id = doc.add_atom("C", Point::new(80. * angle.cos(), 5. * angle.sin()));
            doc.atom_mut(id).unwrap().depth = 80. * angle.sin() + 0.1 * (2. * angle).cos();
            id
        })
        .collect();
    for k in 0..6 {
        doc.add_bond(ring[k], ring[(k + 1) % 6], 4, "plain");
    }
    let (from, to) = (Point::new(-80., -2.), Point::new(80., -2.));
    let a = doc.add_atom("C", from);
    let b = doc.add_atom("C", to);
    doc.atom_mut(a).unwrap().depth = -10.;
    doc.atom_mut(b).unwrap().depth = -10.;
    doc.add_bond(a, b, 1, "plain");
    // Listed first, so index order alone would put the bond behind the ring.
    let last = doc.bonds.len() - 1;
    doc.bonds.swap(0, last);
    tag(&mut doc);
    let inks = inks(&crate::scene::primitives(&doc));
    let bond: Vec<&Ink> = inks.iter().filter(|i| i.owner == Some(0)).collect();
    assert_eq!(crate::aromatic::circles(&doc).len(), 1);
    for x in [-44.6, 44.6] {
        assert!(front_continuous(&bond, from, to, Point::new(x, -2.)));
    }
}
