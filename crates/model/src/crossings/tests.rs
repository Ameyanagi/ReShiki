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
