//! The developer demo surface and its place in a model's picture: an
//! analytic π-like field around the model's atoms, meshed once and cached
//! (never saved), then cut and merged for the model's view each time it is
//! drawn. See docs/plans/xtb-surfaces.md.
// Fixed three-axis arrays, and mesh and atom indices made by the mesher and
// the model's own geometry.
#![allow(clippy::indexing_slicing)]
use super::mesh::{self, Grid, Mesh};
use super::visible::{self, Input, Line, Occluder, Visible};
use super::{Fill, Look, fit};
use crate::document::Document;
use crate::models3d::{BLACK, Model3d, Style, View, WHITE, dot, mix, rgb, rotate, sub};
use crate::scene::{Outline, Primitive, SurfaceFill, SurfaceGroup, SurfaceStroke};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

/// Triangles for the drawing and export, and while the model turns.
pub const DETAIL: [usize; 2] = [20_000, 4_000];
/// Fit error of outlines, outline runs and mesh lines, and of shading-tone
/// contours (clipped to their region, so they may stray further), in
/// drawing units.
const FIT_ERROR: f32 = 0.12;
const TONE_FIT_ERROR: f32 = 0.5;
const MIN_AREA: f32 = 0.05;
const MIN_WIDTH: f32 = 0.12;
const MIN_RUN: f32 = 1.;
const TOLERANCE: f32 = 0.03;
const MAX_TONES: usize = 128;
const MAX_GROUPS: usize = 6;
const TONE_STEP: f32 = 1.;
/// The distance between mesh planes, in ångströms.
const MESH_SPACING: f64 = 0.3;
/// The demo's p-function decay, per ångström.
const DECAY: f32 = 1.6;
/// Box margin around the atoms before the mesher grows it, in ångström.
const MARGIN: f32 = 3.;
/// Phase colours on a light and on a dark canvas: positive, negative.
const POSITIVE: [f32; 3] = [61., 111., 214.];
const NEGATIVE: [[f32; 3]; 2] = [[236., 238., 242.], [205., 210., 218.]];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Demo {
    /// One sign on every heavy atom: a bonding π orbital.
    Pi,
    /// Signs alternating across bonds: an antibonding π* orbital.
    PiStar,
}

static SWITCH: OnceLock<(Demo, f32, Look)> = OnceLock::new();
thread_local! {
    static THREAD: std::cell::Cell<Option<(Demo, f32, Look)>> = const { std::cell::Cell::new(None) };
}

/// Turns the analytic demo surface on for every 3D model it supports, from
/// `RESHIKI_DEV_ORBITAL_SURFACE`: `name[:iso][,look=<token>][,opacity=<percent>]`.
pub fn enable_demo(spec: &str) -> Result<(), String> {
    SWITCH
        .set(parse(spec)?)
        .map_err(|_| "The demo surface is already on".to_string())
}

/// Turns the demo surface on for this thread only, ahead of the
/// process-wide switch, or with `None` goes back to that switch: for tests,
/// which share one process and never set the process-wide switch.
pub fn demo_on_this_thread(spec: Option<&str>) -> Result<(), String> {
    THREAD.set(spec.map(parse).transpose()?);
    Ok(())
}

fn switch() -> Option<(Demo, f32, Look)> {
    THREAD.get().or_else(|| SWITCH.get().copied())
}

fn parse(spec: &str) -> Result<(Demo, f32, Look), String> {
    let mut fields = spec.split(',');
    let field = fields.next().unwrap_or_default();
    let (name, iso) = field.split_once(':').unwrap_or((field, "0.06"));
    let demo = match name {
        "pi" => Demo::Pi,
        "pistar" => Demo::PiStar,
        _ => return Err(format!("Unknown demo surface {name:?}; use pi or pistar")),
    };
    let iso: f32 = iso
        .parse()
        .ok()
        .filter(|v: &f32| v.is_finite() && *v > 0.)
        .ok_or_else(|| format!("Isovalue {iso:?} is not a positive number"))?;
    let mut look = None;
    let mut opacity = None;
    for field in fields {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("Surface option {field:?} must use key=value"))?;
        match key {
            "look" if look.is_none() => look = Some(value.parse::<Look>()?),
            "opacity" if opacity.is_none() => {
                opacity = Some(value.parse::<u8>().map_err(|_| {
                    format!("Surface opacity {value:?} must be an integer between 20 and 100")
                })?);
            }
            "look" | "opacity" => return Err(format!("Duplicate surface option {key:?}")),
            _ => return Err(format!("Unknown surface option {key:?}")),
        }
    }
    let look = look.unwrap_or_default();
    Ok((
        demo,
        iso,
        Look::new(look.fill, look.mesh, opacity.unwrap_or(look.opacity))?,
    ))
}

/// The model's demo meshes at both details, if the switch is on and the
/// prototype supports the model (no crystals, no Wireframe or ellipsoids).
pub(in crate::models3d) fn meshes(doc: &Document, model: &Model3d) -> Option<Rc<[Mesh; 2]>> {
    let (demo, iso, _) = switch()?;
    if model.cell.is_some() || matches!(model.style, Style::Wireframe | Style::Ellipsoids) {
        return None;
    }
    let points = model.points(doc)?;
    let bonds = model.geometry_bonds(doc);
    let mut key: Vec<u32> = vec![demo as u32, iso.to_bits(), points.len() as u32];
    key.extend(points.iter().flat_map(|(s, p)| {
        let [x, y, z] = p.map(f32::to_bits);
        [u32::from(*s == "H"), x, y, z]
    }));
    key.extend(bonds.iter().flatten().map(|&i| i as u32));
    if let Some(found) =
        MESHES.with_borrow(|kept| kept.iter().find(|(k, _)| *k == key).map(|(_, m)| m.clone()))
    {
        return found;
    }
    // No heavy atom, no field: cheap to find again, so not kept.
    let field = field(&points, &bonds, demo)?;
    let (lo, hi) = points.iter().filter(|(s, _)| *s != "H").fold(
        ([f32::MAX; 3], [f32::MIN; 3]),
        |(lo, hi), (_, p)| {
            (
                std::array::from_fn(|k| lo[k].min(p[k] - MARGIN)),
                std::array::from_fn(|k| hi[k].max(p[k] + MARGIN)),
            )
        },
    );
    let (raw, step) = mesh::isosurface_with_step(&field, Grid::around(lo, hi), iso);
    // A lobe the grown box still cuts would show a flat cap: draw nothing,
    // and remember that.
    let found = (!raw.truncated).then(|| {
        let fine = mesh::simplify(&raw, DETAIL[0]);
        let coarse = mesh::simplify(&fine, DETAIL[1]);
        Rc::new([fine, coarse].map(|m| mesh::refine(&m, &field, iso, step)))
    });
    MESHES.with_borrow_mut(|kept| {
        if kept.len() >= KEPT {
            kept.remove(0);
        }
        kept.push((key, found.clone()));
    });
    found
}

const KEPT: usize = 4;
/// Recent models' meshes by their geometry, switch and isovalue.
type Kept = Vec<(Vec<u32>, Option<Rc<[Mesh; 2]>>)>;
thread_local! {
    static MESHES: RefCell<Kept> = Default::default();
}

/// A p function on every heavy atom along the normal of the molecule's
/// best-fit plane, signed as `demo` says.
fn field(
    points: &[(&str, [f32; 3])],
    bonds: &[[usize; 2]],
    demo: Demo,
) -> Option<impl Fn([f32; 3]) -> f32> {
    let normal = plane_normal(&points.iter().map(|(_, p)| *p).collect::<Vec<_>>());
    let heavy: Vec<usize> = (0..points.len())
        .filter(|&i| points.get(i).is_some_and(|(s, _)| *s != "H"))
        .collect();
    // Two-colour the heavy atoms across bonds, breadth first from each part.
    let mut sign = vec![0f32; points.len()];
    for &start in &heavy {
        if sign[start] != 0. {
            continue;
        }
        sign[start] = 1.;
        let mut queue = vec![start];
        while let Some(i) = queue.pop() {
            for &[a, b] in bonds {
                let other = if a == i {
                    b
                } else if b == i {
                    a
                } else {
                    continue;
                };
                if sign.get(other) == Some(&0.) && heavy.contains(&other) {
                    sign[other] = if demo == Demo::PiStar {
                        -sign[i]
                    } else {
                        sign[i]
                    };
                    queue.push(other);
                }
            }
        }
    }
    let centres: Vec<([f32; 3], f32)> = heavy.iter().map(|&i| (points[i].1, sign[i])).collect();
    (!centres.is_empty()).then_some(move |r: [f32; 3]| {
        centres
            .iter()
            .map(|&(c, s)| {
                let d = sub(r, c);
                let along = d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2];
                s * along * (-DECAY * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()).exp()
            })
            .sum()
    })
}

/// The unit normal of the plane that fits `points` best.
fn plane_normal(points: &[[f32; 3]]) -> [f32; 3] {
    let (_, values, vectors) = principal_axes(points);
    // A line or a lone atom gets a normal across it.
    let least = (0..3)
        .min_by(|&i, &j| values[i].total_cmp(&values[j]))
        .unwrap_or(2);
    vectors[least].map(|x| x as f32)
}

fn principal_axes(points: &[[f32; 3]]) -> ([f64; 3], [f64; 3], [[f64; 3]; 3]) {
    let n = points.len().max(1) as f64;
    let mean: [f64; 3] =
        std::array::from_fn(|k| points.iter().map(|p| p[k] as f64).sum::<f64>() / n);
    let mut c = [[0f64; 3]; 3];
    for p in points {
        let d: [f64; 3] = std::array::from_fn(|k| p[k] as f64 - mean[k]);
        for (i, row) in c.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v += d[i] * d[j];
            }
        }
    }
    let (values, vectors) = crate::models3d::ellipsoids::eigen(c);
    (mean, values, vectors)
}

/// Intersect each triangle with the three model-space families before projection.
fn mesh_lines(
    mesh: &Mesh,
    front: &[bool],
    backs: bool,
    atoms: &[[f32; 3]],
    project: impl Fn([f32; 3]) -> [f32; 3],
) -> Vec<Line> {
    let (mean, _, axes) = principal_axes(atoms);
    let coordinates: Vec<_> = mesh
        .positions
        .iter()
        .map(|p| {
            axes.map(|axis| {
                let distance = (0..3)
                    .map(|i| (f64::from(p[i]) - mean[i]) * axis[i])
                    .sum::<f64>();
                distance / MESH_SPACING - 0.5
            })
        })
        .collect();
    let mut lines = Vec::new();
    for (triangle, (&vertices, &is_front)) in mesh.triangles.iter().zip(front).enumerate() {
        if !is_front && !backs {
            continue;
        }
        let vertices = vertices.map(|i| i as usize);
        let [a, b, c] = vertices.map(|i| coordinates[i]);
        for ((a, b), c) in a.into_iter().zip(b).zip(c) {
            let heights = [a, b, c];
            let low = heights.into_iter().fold(f64::INFINITY, f64::min);
            let high = heights.into_iter().fold(f64::NEG_INFINITY, f64::max);
            // A vertex exactly on the plane counts as above it.
            for plane in (low.floor() as i64).saturating_add(1)..=high.floor() as i64 {
                let mut cuts = [None; 3];
                for (cut, (a, b)) in cuts.iter_mut().zip([(0, 1), (1, 2), (2, 0)]) {
                    let (a, b) = if vertices[a] < vertices[b] {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    let [da, db] = [heights[a], heights[b]].map(|h| h - plane as f64);
                    if (da < 0.) == (db < 0.) {
                        continue;
                    }
                    // A shared edge must produce the same point in both triangles.
                    let [p, q] =
                        [vertices[a], vertices[b]].map(|i| mesh.positions[i].map(f64::from));
                    let t = da / (da - db);
                    *cut = Some(std::array::from_fn(|i| (p[i] + t * (q[i] - p[i])) as f32));
                }
                let mut cuts = cuts.into_iter().flatten();
                if let (Some(a), Some(b)) = (cuts.next(), cuts.next())
                    && a != b
                {
                    lines.push(Line {
                        triangle: triangle as u32,
                        ends: [a, b].map(&project),
                    });
                }
            }
        }
    }
    lines
}

fn phase_base(positive: bool, dark: bool) -> [f32; 3] {
    if positive {
        POSITIVE
    } else {
        NEGATIVE[usize::from(dark)]
    }
}

fn shaded(base: [f32; 3], light: f32, backdrop: [f32; 3], fade: f32) -> [u8; 3] {
    rgb(shade(base, light, backdrop, fade))
}

fn shade(base: [f32; 3], light: f32, backdrop: [f32; 3], fade: f32) -> [f32; 3] {
    let color = if light < 0.6 {
        mix(base, BLACK, (0.6 - light) / 0.6 * 0.45)
    } else {
        mix(base, WHITE, (light - 0.6) / 0.4 * 0.5)
    };
    mix(color, backdrop, fade)
}

/// The bend of every shading ramp (lightness 0.6) on the Lambert axis
/// (lightness 0.35 + 0.65·u).
const BEND: f32 = 0.25 / 0.65;

/// One phase's shading ramp measured along its steepest colour channel, so
/// that equal tone steps are equal colour steps. The ramp is two straight
/// pieces in lightness: darkening below the bend, lightening above it.
#[derive(Clone, Copy)]
struct Ramp {
    /// Levels from the darkest lightness (0.35) to the bend, and from the
    /// bend to the lightest (1).
    below: f32,
    above: f32,
}

impl Ramp {
    fn new(base: [f32; 3], backdrop: [f32; 3], fade: f32) -> Self {
        let [dark, bend, light] = [0.35, 0.6, 1.].map(|l| shade(base, l, backdrop, fade));
        let levels =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0., f32::max);
        Ramp {
            below: levels(dark, bend),
            above: levels(bend, light),
        }
    }

    fn levels(self) -> f32 {
        self.below + self.above
    }

    /// The tone coordinate (0 to 1, even in colour) of Lambert term `u`.
    fn tone(self, u: f32) -> f32 {
        let total = self.levels().max(f32::MIN_POSITIVE);
        if u < BEND {
            self.below * u / BEND / total
        } else {
            (self.below + self.above * (u - BEND) / (1. - BEND)) / total
        }
    }

    /// The lightness (0.35 to 1) at tone coordinate `t`.
    fn lightness(self, t: f32) -> f32 {
        let at = t * self.levels();
        let u = if at < self.below {
            at / self.below * BEND
        } else if self.above > 0. {
            BEND + (at - self.below) / self.above * (1. - BEND)
        } else {
            BEND
        };
        0.35 + 0.65 * u.clamp(0., 1.)
    }
}

fn group_weight(alpha: f32, depth: usize) -> f32 {
    alpha * (1. - alpha).powi(depth as i32)
}

fn tone_counts(look: Look, dark: bool, backdrop: [f32; 3], fade: f32) -> Vec<usize> {
    let alpha = look.alpha();
    let groups = (1..MAX_GROUPS)
        .find(|&depth| group_weight(alpha, depth) < 1. / 50.)
        .unwrap_or(MAX_GROUPS);
    match look.fill {
        Fill::None => vec![0; groups],
        Fill::Solid | Fill::Outline => vec![1; groups],
        Fill::Shaded => {
            let levels = [true, false]
                .map(|positive| Ramp::new(phase_base(positive, dark), backdrop, fade).levels())
                .into_iter()
                .fold(0., f32::max);
            (0..groups)
                .map(|depth| {
                    let tones = levels * group_weight(alpha, depth) / TONE_STEP;
                    (tones.ceil() as usize).clamp(2, MAX_TONES)
                })
                .collect()
        }
    }
}

/// The surface cut and merged for `view`, ready to draw after the molecule.
pub(in crate::models3d) struct Prepared {
    positions: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
    front: Vec<bool>,
    visible: Visible,
    look: Look,
    tones: Vec<usize>,
}

impl Prepared {
    /// Cuts the model's surface for the view, or `None` without one or
    /// during the 2D → 3D morph.
    /// `outline` is the width of the balls' painted outline.
    pub(in crate::models3d) fn new(
        doc: &Document,
        model: &Model3d,
        view: &View,
        outline: f32,
        dark: bool,
        backdrop: [f32; 3],
        fade: f32,
    ) -> Option<Self> {
        if model.morph.is_some_and(|m| m < 1.) {
            return None;
        }
        let (_, _, look) = switch()?;
        let meshes = meshes(doc, model)?;
        let mesh = &meshes[usize::from(model.turning)];
        let rotated: Vec<[f32; 3]> = mesh
            .positions
            .iter()
            .map(|p| rotate(model.rotation, sub(*p, view.center)))
            .collect();
        let positions: Vec<[f32; 3]> = rotated.iter().map(|&q| screen(view, q)).collect();
        let front = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| rotated[i as usize]);
                let n = cross(sub(b, a), sub(c, a));
                let eye = view.eye(a);
                dot(n, eye) > 0.
            })
            .collect::<Vec<_>>();
        // The balls' light, from the upper left in front.
        let l = [-0.38f32, 0.42, 0.82];
        let length = dot(l, l).sqrt();
        let light = l.map(|v| v / length);
        // Tone coordinates even in colour, by each vertex's phase.
        let ramps =
            [true, false].map(|positive| Ramp::new(phase_base(positive, dark), backdrop, fade));
        let mut phase = vec![0; mesh.positions.len()];
        for (t, &positive) in mesh.triangles.iter().zip(&mesh.positive) {
            for &v in t {
                phase[v as usize] = usize::from(!positive);
            }
        }
        let lightness: Vec<f32> = mesh
            .normals
            .iter()
            .zip(&phase)
            .map(|(n, &phase)| {
                ramps[phase].tone(dot(rotate(model.rotation, *n), light).clamp(0., 1.))
            })
            .collect();
        let tones = tone_counts(look, dark, backdrop, fade);
        let lines = if look.mesh {
            let atoms: Vec<_> = model
                .atoms
                .iter()
                .map(|a| a.xyz)
                .chain(model.hydrogens.iter().map(|h| h.xyz))
                .collect();
            mesh_lines(mesh, &front, tones.len() > 1, &atoms, |p| {
                screen(view, rotate(model.rotation, sub(p, view.center)))
            })
        } else {
            Vec::new()
        };
        let occluders = occluders(view, model, outline / 2.);
        let visible = visible::visible(&Input {
            positions: &positions,
            triangles: &mesh.triangles,
            positive: &mesh.positive,
            front: &front,
            lightness: &lightness,
            occluders: &occluders,
            tones: &tones,
            lines: &lines,
            tolerance: TOLERANCE,
        });
        Some(Prepared {
            positions,
            triangles: mesh.triangles.clone(),
            front,
            visible,
            look,
            tones,
        })
    }

    /// Whether nothing of the surface lies in front of the ball of `radius`
    /// about the rotated point `q`, on the sight line through its centre,
    /// so the label there is drawn over the surface.
    pub(in crate::models3d) fn clear(&self, view: &View, q: [f32; 3], radius: f32) -> bool {
        let [x, y, _] = screen(view, q);
        let z = screen(view, [q[0], q[1], q[2] + radius])[2];
        let input = Input {
            positions: &self.positions,
            triangles: &self.triangles,
            positive: &[],
            front: &self.front,
            lightness: &[],
            occluders: &[],
            tones: &[1],
            lines: &[],
            tolerance: TOLERANCE,
        };
        visible::front_depth(&input, [x, y]).is_none_or(|depth| depth <= z)
    }

    /// Fit the visible shapes and paint each depth as a group.
    pub(in crate::models3d) fn primitive(
        &self,
        dark: bool,
        backdrop: [f32; 3],
        fade: f32,
        outline: Option<Outline>,
        line_width: f32,
    ) -> Option<Primitive> {
        let alpha = self.look.alpha();
        let mut groups = Vec::new();
        for (depth, &count) in self.tones.iter().enumerate().rev() {
            let mut fills = Vec::new();
            let mut strokes = Vec::new();
            for (sign, positive) in [true, false].into_iter().enumerate() {
                let base = phase_base(positive, dark);
                let ramp = Ramp::new(base, backdrop, fade);
                let color = |tone: usize| match self.look.fill {
                    Fill::Shaded => {
                        let light = ramp.lightness((tone as f32 + 0.5) / count as f32);
                        shaded(base, light, backdrop, fade)
                    }
                    Fill::Solid => rgb(mix(base, backdrop, fade)),
                    Fill::Outline | Fill::None => rgb(backdrop),
                };
                let mut layers = self
                    .visible
                    .layers
                    .iter()
                    .filter(|layer| layer.depth == depth && layer.positive == positive);
                if let Some(first) = layers.next() {
                    let region = fit::region(&first.region, FIT_ERROR, MIN_AREA, MIN_WIDTH);
                    if !region.is_empty() {
                        let tones = layers
                            .filter_map(|layer| {
                                let path =
                                    fit::region(&layer.region, TONE_FIT_ERROR, MIN_AREA, MIN_WIDTH);
                                (!path.is_empty()).then(|| (path, color(layer.tone)))
                            })
                            .collect();
                        fills.push(SurfaceFill {
                            region,
                            color: color(first.tone),
                            tones,
                        });
                    }
                }
                if self.look.mesh {
                    let paths = fit::runs(&self.visible.lines[depth][sign], FIT_ERROR, MIN_RUN);
                    if !paths.is_empty() {
                        strokes.push(SurfaceStroke {
                            paths,
                            outline: Outline {
                                color: rgb(mix(mix(base, BLACK, 0.45), backdrop, fade)),
                                alpha: 0.9,
                                width: outline
                                    .map_or(0.45 * line_width, |o| (0.45 * o.width).max(0.35)),
                            },
                        });
                    }
                }
            }
            if let Some(outline) = outline {
                let paths = fit::runs(&self.visible.rims[depth], FIT_ERROR, MIN_RUN);
                if !paths.is_empty() {
                    strokes.push(SurfaceStroke { paths, outline });
                }
            }
            if !fills.is_empty() || !strokes.is_empty() {
                let alpha = if self.look.fill == Fill::None {
                    group_weight(alpha, depth)
                } else {
                    alpha
                };
                groups.push(SurfaceGroup {
                    alpha,
                    fills,
                    strokes,
                });
            }
        }
        (!groups.is_empty()).then_some(Primitive::Surface { groups })
    }
}

/// A rotated point in screen-depth space: drawing units across, and a depth
/// that keeps planes flat under perspective (design §0).
fn screen(view: &View, q: [f32; 3]) -> [f32; 3] {
    let (p, f) = view.project(q);
    [p.x, p.y, view.scale * f * q[2]]
}

/// The balls and half-bonds as painted, as convex solids in screen-depth
/// space (design §2); balls reach `rim` further, for their outline.
fn occluders(view: &View, model: &Model3d, rim: f32) -> Vec<Occluder> {
    let depth = |q: [f32; 3], r: f32| screen(view, [q[0], q[1], q[2] + r])[2] - screen(view, q)[2];
    let mut out: Vec<Occluder> = view
        .balls()
        .filter_map(|(i, q, radius)| {
            let p = *view.rotated.get(i)?;
            let r = model.style.atom_radius(view.symbols.get(i)?);
            Some(Occluder::Ball {
                center: [q.x, q.y, screen(view, p)[2]],
                radius: radius + rim,
                depth: depth(p, r),
            })
        })
        .collect();
    let r = model.style.bond_radius();
    for &(a, b) in &view.bonds {
        let (Some(&pa), Some(&pb)) = (view.rotated.get(a), view.rotated.get(b)) else {
            continue;
        };
        let middle = [0, 1, 2].map(|k| (pa[k] + pb[k]) / 2.);
        for start in [pa, pb] {
            let [s, m] = [start, middle].map(|q| screen(view, q));
            let width = |q: [f32; 3]| r * view.scale * view.project(q).1;
            // A half seen nearly end-on is not painted (`View::joints`, the
            // BallStick length check); its atom's ball covers it.
            if (m[0] - s[0]).hypot(m[1] - s[1]) < crate::models3d::END_ON * width(start) {
                continue;
            }
            out.push(Occluder::Stick {
                a: s,
                b: m,
                half: [width(start), width(middle)],
                depth: [depth(start, r), depth(middle, r)],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The farthest mesh point from the view's centre, so the view's reach can
/// hold the surface (design §0).
pub(in crate::models3d) fn reach(meshes: &[Mesh; 2], center: [f32; 3]) -> f32 {
    meshes[0]
        .positions
        .iter()
        .map(|p| super::super::length(sub(*p, center)))
        .fold(0., f32::max)
}
