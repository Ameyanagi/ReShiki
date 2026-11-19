//! The paint plan: placed pieces become one ordered list of plain paths, each
//! tagged with its owner, pass and material. Painting, hit testing and
//! ChemDraw export all read this list, so they agree on what is on top.
//! Nothing is clipped at render time: seated cones, hatching and liquid are
//! cut here, geometrically.
use super::eval::{Evaluated, Values, evaluate, resolve, tube_outline};
use super::format::*;
use super::geometry::{Affine, Seg, add, area, dot, mul, normalize, perp, polygon, sub};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use std::collections::BTreeSet;

/// A joint drawing's extra paths: material, colour, ink, style and path.
type Detail = (Material, Option<Rgb>, Ink, Option<StyleKind>, Vec<Seg>);
/// A piece's glass: fill path, outline path (open where a cut is not stroked), colour and style.
type GlassPath = (Vec<Seg>, Vec<Seg>, Option<Rgb>, Option<StyleKind>);

/// One placed piece.
#[derive(Debug, Clone)]
pub struct Instance<'a> {
    pub definition: &'a Definition,
    pub values: Values,
    /// Millimetres to world units: a similarity.
    pub transform: Affine,
    /// Joints with a partner seated in them.
    pub seated: BTreeSet<String>,
    /// A tube piece's centre line, in the piece's millimetres.
    pub centre_line: Vec<Xy>,
}

impl<'a> Instance<'a> {
    pub fn new(definition: &'a Definition, transform: Affine) -> Self {
        Self {
            definition,
            values: Values::new(),
            transform,
            seated: BTreeSet::new(),
            centre_line: Vec::new(),
        }
    }
}

/// Paint order inside one compositing group (an assembly or a free piece).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pass {
    Back,
    /// Glass outlines at twice the stroke width, piece by piece …
    Outline,
    /// … each followed by its opaque glass fills, so a piece is one silhouette.
    Glass,
    Liquid,
    Inner,
    Body,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    /// The outline stroke (0.6 pt by default; twice that in the outline pass).
    Outline,
    Detail,
    Hatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ink {
    Fill,
    Stroke(Weight),
    FillStroke(Weight),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlanPath {
    /// Index of the owning instance.
    pub owner: usize,
    pub pass: Pass,
    pub material: Material,
    pub color: Option<Rgb>,
    pub ink: Ink,
    /// Only in this style; `None` in both.
    pub style: Option<StyleKind>,
    /// World units.
    #[serde(serialize_with = "segs")]
    pub path: Vec<Seg>,
}

fn segs<S: serde::Serializer>(path: &[Seg], s: S) -> Result<S::Ok, S::Error> {
    let r = |v: f64| (v * 1000.).round() / 1000.;
    let mut text = String::new();
    for seg in path {
        let part = match *seg {
            Seg::Move(p) => format!("M{} {}", r(p[0]), r(p[1])),
            Seg::Line(p) => format!("L{} {}", r(p[0]), r(p[1])),
            Seg::Cubic(a, b, p) => format!(
                "C{} {} {} {} {} {}",
                r(a[0]),
                r(a[1]),
                r(b[0]),
                r(b[1]),
                r(p[0]),
                r(p[1])
            ),
            Seg::Close => "Z".into(),
        };
        text.push_str(&part);
    }
    s.serialize_str(&text)
}

/// Appearance-independent settings the geometry depends on.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Page points per world unit (14.4/42 by default).
    pub points_per_world: f64,
    /// Ground-glass hatch spacing in points.
    pub hatch_pt: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            points_per_world: 14.4 / 42.,
            hatch_pt: 1.5,
        }
    }
}

const BORE: Rgb = [201, 213, 220];

struct Item {
    pass: Pass,
    depth: i8,
    owner: usize,
    path: PlanPath,
}

/// The paint plan of one compositing group, in paint order.
pub fn paint_plan(group: &[Instance], settings: Settings) -> Result<Vec<PlanPath>, String> {
    let mut items = Vec::new();
    for (owner, instance) in group.iter().enumerate() {
        let values = resolve(instance.definition, &instance.values)?;
        let evaluated = evaluate(instance.definition, &values)?;
        piece(owner, instance, &evaluated, settings, &mut items)?;
    }
    // Stable sort keeps each piece's own order inside a pass.
    items.sort_by_key(|i| {
        let per_piece = matches!(i.pass, Pass::Outline | Pass::Glass | Pass::Liquid);
        (
            if per_piece { Pass::Outline } else { i.pass },
            if per_piece { 0 } else { i.depth },
            i.owner,
            if per_piece { i.pass } else { Pass::Back },
        )
    });
    Ok(items.into_iter().map(|i| i.path).collect())
}

fn layer_pass(layer: Layer) -> Pass {
    match layer {
        Layer::Back => Pass::Back,
        Layer::Body => Pass::Body,
        Layer::Inner => Pass::Inner,
        Layer::Front => Pass::Front,
    }
}

fn piece(
    owner: usize,
    instance: &Instance,
    e: &Evaluated,
    settings: Settings,
    items: &mut Vec<Item>,
) -> Result<(), String> {
    let t = instance.transform;
    let mm = t.scale_factor();
    let push = |items: &mut Vec<Item>,
                pass: Pass,
                depth: i8,
                material: Material,
                color: Option<Rgb>,
                ink: Ink,
                style: Option<StyleKind>,
                path: &[Seg]| {
        items.push(Item {
            pass,
            depth,
            owner,
            path: PlanPath {
                owner,
                pass,
                material,
                color,
                ink,
                style,
                path: t.map(path),
            },
        })
    };
    // Glass on the body layer joins one silhouette: outlines, then fills.
    let mut glass: Vec<GlassPath> = Vec::new();
    for d in &e.shapes {
        let s = d.shape;
        if s.role == Role::Glass && s.layer == Layer::Body {
            glass.push((d.path.clone(), d.path.clone(), s.color, s.style));
            continue;
        }
        let ink = match (s.role, s.material) {
            (Role::Detail, _) => Ink::Stroke(Weight::Detail),
            (_, Material::Ink) => Ink::Fill,
            _ => Ink::FillStroke(Weight::Outline),
        };
        push(
            items,
            layer_pass(s.layer),
            s.depth,
            s.material,
            s.color,
            ink,
            s.style,
            &d.path,
        );
    }
    let hatch_mm = settings.hatch_pt / (mm * settings.points_per_world);
    let mut details: Vec<Detail> = Vec::new();
    for j in &e.joints {
        let Some(size) = j.joint.standard() else {
            continue;
        };
        let seated = instance.seated.contains(j.joint.id());
        standard_joint(
            j.joint.gender(),
            size,
            j.at,
            j.dir,
            seated,
            hatch_mm,
            &mut glass,
            &mut details,
        );
    }
    glass.sort_by(|a, b| fill_area(&a.0).total_cmp(&fill_area(&b.0)));
    for (_, outline, _, style) in &glass {
        push(
            items,
            Pass::Outline,
            0,
            Material::Glass,
            None,
            Ink::Stroke(Weight::Outline),
            *style,
            outline,
        );
    }
    for (fill, _, color, style) in &glass {
        push(
            items,
            Pass::Glass,
            0,
            Material::Glass,
            *color,
            Ink::Fill,
            *style,
            fill,
        );
    }
    for (material, color, ink, style, path) in details {
        push(items, Pass::Body, 0, material, color, ink, style, &path);
    }
    if let Some(tube) = &instance.definition.tube
        && instance.centre_line.len() >= 2
    {
        let path = tube_outline(&instance.centre_line, tube.width_mm);
        push(
            items,
            Pass::Front,
            0,
            tube.material,
            tube.color,
            Ink::FillStroke(Weight::Outline),
            None,
            &path,
        );
    }
    for l in &e.liquids {
        // The surface stays level on the page, so liquid is cut in world space.
        let region: Vec<Vec<Xy>> = l
            .region
            .iter()
            .map(|c| c.iter().map(|p| t.apply(*p)).collect())
            .collect();
        for path in liquid(&region, l.fill, l.liquid.meniscus, mm) {
            items.push(Item {
                pass: Pass::Liquid,
                depth: 0,
                owner,
                path: PlanPath {
                    owner,
                    pass: Pass::Liquid,
                    material: Material::Liquid,
                    color: Some(l.color),
                    ink: path.0,
                    style: None,
                    path: path.1,
                },
            });
        }
    }
    Ok(())
}

fn fill_area(path: &[Seg]) -> f64 {
    super::geometry::flatten(path)
        .iter()
        .map(|(p, _)| area(p).abs())
        .sum()
}

/// A shape symmetric about an axis: profile of (distance along `u`, radius) pairs.
fn axial(p: Xy, u: Xy, profile: &[(f64, f64)]) -> Vec<Xy> {
    let v = perp(u);
    let at = |s: f64, r: f64| add(add(p, mul(u, s)), mul(v, r));
    profile
        .iter()
        .map(|&(s, r)| at(s, r))
        .chain(profile.iter().rev().map(|&(s, r)| at(s, -r)))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn standard_joint(
    gender: Gender,
    t: &TaperSize,
    at: Xy,
    dir: Xy,
    seated: bool,
    hatch_mm: f64,
    glass: &mut Vec<GlassPath>,
    details: &mut Vec<Detail>,
) {
    let (big, small) = (t.d / 2., t.d / 2. - t.l / 20.);
    // A ground zone: frosted fill, its outline (open across a cut) and, in line style, hatching.
    let ground = |points: Vec<Xy>, cut: bool, details: &mut Vec<Detail>| {
        if cut {
            let mut open = points.clone();
            open.rotate_left(2);
            details.push((
                Material::Frosted,
                None,
                Ink::Fill,
                None,
                polygon(&points, true),
            ));
            details.push((
                Material::Frosted,
                None,
                Ink::Stroke(Weight::Hatch),
                None,
                polygon(&open, false),
            ));
        } else {
            details.push((
                Material::Frosted,
                None,
                Ink::FillStroke(Weight::Hatch),
                None,
                polygon(&points, true),
            ));
        }
        let lines = hatch(&points, hatch_mm);
        if !lines.is_empty() {
            details.push((
                Material::Frosted,
                None,
                Ink::Stroke(Weight::Hatch),
                Some(StyleKind::Line),
                lines,
            ));
        }
    };
    match gender {
        Gender::F => {
            let u = mul(normalize(dir), -1.);
            let ri = |s: f64| big - s / 20.;
            let ro = |s: f64| ri(s) + t.socket_wall;
            let rb = t.bead_radius();
            let body = axial(
                at,
                u,
                &[
                    (0., rb),
                    (2.8, rb),
                    (3.6, ro(3.6)),
                    (t.l, ro(t.l)),
                    (t.l + 4., t.neck / 2.),
                ],
            );
            glass.push((polygon(&body, true), polygon(&body, true), None, None));
            ground(
                axial(at, u, &[(1.2, ri(1.2)), (t.l - 0.6, ri(t.l - 0.6))]),
                false,
                details,
            );
            let v = perp(u);
            let rim = |s: f64, r: f64| add(add(at, mul(u, s)), mul(v, r));
            details.push((
                Material::Glass,
                None,
                Ink::Stroke(Weight::Detail),
                None,
                polygon(&[rim(2.8, rb), rim(2.8, -rb)], false),
            ));
            // The mouth, seen slightly from above: open while free, its front rim once seated.
            let ry = (t.d * 0.11).max(1.);
            let ellipse = |t0: f64, t1: f64| -> Vec<Xy> {
                (0..=24)
                    .map(|i| {
                        let a = t0 + (t1 - t0) * f64::from(i) / 24.;
                        add(add(at, mul(v, big * a.cos())), mul(u, ry * a.sin()))
                    })
                    .collect()
            };
            if seated {
                details.push((
                    Material::Glass,
                    None,
                    Ink::Stroke(Weight::Detail),
                    None,
                    polygon(&ellipse(0., std::f64::consts::PI), false),
                ));
            } else {
                let mut e = ellipse(0., std::f64::consts::TAU);
                e.pop();
                details.push((
                    Material::Glass,
                    Some(BORE),
                    Ink::FillStroke(Weight::Detail),
                    None,
                    polygon(&e, true),
                ));
            }
        }
        Gender::M => {
            let u = normalize(dir);
            let r = |s: f64| big - s / 20.;
            let collar = big + 0.8;
            if seated {
                // Cut at the socket mouth; the cut edge is filled but not stroked.
                let e = t.exposure();
                let half: Vec<(f64, f64)> =
                    vec![(-3.6, collar), (-0.5, collar), (0., big), (e, r(e))];
                let body = axial(at, u, &half);
                // Start after the cut edge so the open outline leaves it out.
                let mut outline = body.clone();
                outline.rotate_left(half.len());
                glass.push((polygon(&body, true), polygon(&outline, false), None, None));
                ground(axial(at, u, &[(0., big), (e, r(e))]), true, details);
            } else {
                let body = axial(
                    at,
                    u,
                    &[
                        (-3.6, collar),
                        (-0.5, collar),
                        (0., big),
                        (t.l, small),
                        (t.l + 1.4, small - 0.7),
                    ],
                );
                glass.push((polygon(&body, true), polygon(&body, true), None, None));
                ground(axial(at, u, &[(0., big), (t.l, small)]), false, details);
            }
        }
    }
}

/// 45° hatch lines inside a convex polygon, `h` apart.
fn hatch(points: &[Xy], h: f64) -> Vec<Seg> {
    let n = [
        std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ];
    let along = [
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ];
    let ts: Vec<f64> = points.iter().map(|p| dot(*p, n)).collect();
    let (t0, t1) = ts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| {
            (a.min(t), b.max(t))
        });
    let mut out = Vec::new();
    if h.is_nan() || h <= 0. || (t1 - t0) / h > 10_000. {
        return out;
    }
    let first = (t0 / h).ceil();
    // A counted loop: t + h may not advance t when the coordinates are huge.
    for k in 0..((t1 / h - first).ceil().max(0.) as usize) {
        let t = (first + k as f64) * h;
        let mut hits = Vec::new();
        for (a, b) in super::geometry::edges(points, true) {
            let (da, db) = (dot(a, n) - t, dot(b, n) - t);
            if (da <= 0. && db > 0.) || (da > 0. && db <= 0.) {
                hits.push(dot(add(a, mul(sub(b, a), da / (da - db))), along));
            }
        }
        if hits.len() >= 2 {
            let (s0, s1) = hits
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &s| {
                    (a.min(s), b.max(s))
                });
            let base = mul(n, t);
            out.push(Seg::Move(add(base, mul(along, s0))));
            out.push(Seg::Line(add(base, mul(along, s1))));
        }
    }
    out
}

/// Pools below the level that leaves `fill` of the region's area, and a meniscus on each.
fn liquid(region: &[Vec<Xy>], fill: f64, meniscus: Meniscus, scale: f64) -> Vec<(Ink, Vec<Seg>)> {
    if fill <= 0. || region.is_empty() {
        return Vec::new();
    }
    let (y0, y1) = region
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p[1]), b.max(p[1]))
        });
    let (x0, x1) = region
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p[0]), b.max(p[0]))
        });
    let below = |level: f64| -> Vec<Vec<Vec<[f64; 2]>>> {
        let clip = vec![
            [x0 - 1., level],
            [x1 + 1., level],
            [x1 + 1., y1 + 1.],
            [x0 - 1., y1 + 1.],
        ];
        let subject: Vec<Vec<[f64; 2]>> = region.to_vec();
        FloatOverlay::with_subj_and_clip(&subject, &clip)
            .overlay(OverlayRule::Intersect, FillRule::EvenOdd)
    };
    let pooled = |shapes: &[Vec<Vec<[f64; 2]>>]| -> f64 {
        shapes
            .iter()
            .map(|s| {
                s.iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if i == 0 {
                            area(c).abs()
                        } else {
                            -area(c).abs()
                        }
                    })
                    .sum::<f64>()
            })
            .sum()
    };
    let pools = |shapes: Vec<Vec<Vec<[f64; 2]>>>| -> Vec<(Ink, Vec<Seg>)> {
        shapes
            .into_iter()
            .map(|shape| {
                (
                    Ink::Fill,
                    shape.iter().flat_map(|c| polygon(c, true)).collect(),
                )
            })
            .collect()
    };
    // The same even-odd rule as the cut, so holes and windings agree.
    let total = pooled(&below(y0 - 1.));
    if fill >= 1. || total <= 0. {
        return pools(below(y0 - 1.));
    }
    let (mut lo, mut hi) = (y0, y1);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if pooled(&below(mid)) / total > fill {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let level = (lo + hi) / 2.;
    let shapes = below(level);
    // Each stretch of the cut surface gets its own meniscus, so none crosses dry glass.
    let surfaces: Vec<(f64, f64)> = shapes
        .iter()
        .flat_map(|shape| shape.first())
        .flat_map(|outer| super::geometry::edges(outer, true))
        .filter(|(a, b)| (a[1] - level).abs() < 1e-6 && (b[1] - level).abs() < 1e-6 && a[0] != b[0])
        .map(|(a, b)| (a[0].min(b[0]), a[0].max(b[0])))
        .collect();
    let mut out = pools(shapes);
    if meniscus == Meniscus::Flat {
        return out;
    }
    for (xl, xr) in surfaces {
        let m = (1.6 * scale).min((xr - xl) / 8.)
            * if meniscus == Meniscus::Convex {
                -1.
            } else {
                1.
            };
        let (a, c) = ([xl, level - m * 0.6], [xr, level - m * 0.6]);
        let q = [(xl + xr) / 2., level + m];
        out.push((
            Ink::Stroke(Weight::Detail),
            vec![
                Seg::Move(a),
                Seg::Cubic(
                    add(a, mul(sub(q, a), 2. / 3.)),
                    add(c, mul(sub(q, c), 2. / 3.)),
                    c,
                ),
            ],
        ));
    }
    out
}

/// The transform that seats `moving` (a joint of a piece drawn at `scale`
/// world units per mm) on a fixed joint at `at`/`dir` in world space: the
/// axes oppose, and a male shoulder sits 0.06 L outside the female mouth.
pub fn mating_transform(
    at: Xy,
    dir: Xy,
    exposure_world: f64,
    moving_at: Xy,
    moving_dir: Xy,
    scale: f64,
) -> Affine {
    let target = add(at, mul(normalize(dir), exposure_world));
    let want = mul(normalize(dir), -1.);
    let have = normalize(moving_dir);
    let angle = (want[1].atan2(want[0]) - have[1].atan2(have[0])).to_degrees();
    let r = Affine::rotate(angle, [0., 0.]).then(Affine::scale(scale, [0., 0.]));
    let q = r.apply(moving_at);
    Affine::translate(sub(target, q)).then(r)
}
