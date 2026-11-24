//! The developer demo surface and its place in a model's picture: an
//! analytic π-like field around the model's atoms, meshed once and cached
//! (never saved), then cut and merged for the model's view each time it is
//! drawn. See docs/plans/xtb-surfaces.md.
// Fixed three-axis arrays, and mesh and atom indices made by the mesher and
// the model's own geometry.
#![allow(clippy::indexing_slicing)]
use super::mesh::{self, Grid, Mesh};
use super::visible::{self, Input, Occluder, Visible};
use crate::document::{Document, Point};
use crate::models3d::{Model3d, Style, View, rotate, sub};
use crate::scene::{Outline, Primitive};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

/// Triangles for the drawing and export, and while the model turns.
pub const DETAIL: [usize; 2] = [20_000, 4_000];
/// Shading levels per phase.
const BANDS: usize = 10;
/// Outline thinning, in drawing units.
const TOLERANCE: f32 = 0.05;
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

static SWITCH: OnceLock<(Demo, f32)> = OnceLock::new();
thread_local! {
    static THREAD: std::cell::Cell<Option<(Demo, f32)>> = const { std::cell::Cell::new(None) };
}

/// Turns the analytic demo surface on for every 3D model it supports, from
/// `RESHIKI_DEV_ORBITAL_SURFACE`: `pi` or `pistar`, optionally `:isovalue`.
pub fn enable_demo(spec: &str) -> Result<(), String> {
    SWITCH
        .set(parse(spec)?)
        .map_err(|_| "The demo surface is already on".to_string())
}

/// Turns the demo surface on or off for this thread only, ahead of the
/// process-wide switch: for tests, which share one process.
pub fn demo_on_this_thread(spec: Option<&str>) -> Result<(), String> {
    THREAD.set(spec.map(parse).transpose()?);
    Ok(())
}

fn switch() -> Option<(Demo, f32)> {
    THREAD.get().or_else(|| SWITCH.get().copied())
}

fn parse(spec: &str) -> Result<(Demo, f32), String> {
    let (name, iso) = spec.split_once(':').unwrap_or((spec, "0.06"));
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
    Ok((demo, iso))
}

/// The model's demo meshes at both details, if the switch is on and the
/// prototype supports the model (no crystals, no Wireframe or ellipsoids).
pub(in crate::models3d) fn meshes(doc: &Document, model: &Model3d) -> Option<Rc<[Mesh; 2]>> {
    let (demo, iso) = switch()?;
    if model.cell.is_some() || matches!(model.style, Style::Wireframe | Style::Ellipsoids) {
        return None;
    }
    let points = model.points(doc)?;
    let bonds = model.geometry_bonds(doc);
    let mut key: Vec<u32> = vec![demo as u32, iso.to_bits()];
    key.extend(points.iter().flat_map(|(_, p)| p.map(f32::to_bits)));
    key.extend(bonds.iter().flatten().map(|&i| i as u32));
    if let Some(found) =
        MESHES.with_borrow(|kept| kept.iter().find(|(k, _)| *k == key).map(|(_, m)| m.clone()))
    {
        return Some(found);
    }
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
    let raw = mesh::isosurface(&field, Grid::around(lo, hi), iso);
    let fine = mesh::simplify(&raw, DETAIL[0]);
    let coarse = mesh::simplify(&fine, DETAIL[1]);
    let found = Rc::new([fine, coarse]);
    MESHES.with_borrow_mut(|kept| {
        if kept.len() >= KEPT {
            kept.remove(0);
        }
        kept.push((key, found.clone()));
    });
    Some(found)
}

const KEPT: usize = 4;
/// Recent models' meshes by their geometry, switch and isovalue.
type Kept = Vec<(Vec<u32>, Rc<[Mesh; 2]>)>;
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

/// The unit normal of the plane that fits `points` best: the direction of
/// least spread, from the two directions of most spread.
fn plane_normal(points: &[[f32; 3]]) -> [f32; 3] {
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
    let largest = |c: &[[f64; 3]; 3]| {
        let mut v = [0.6, 0.7, 0.4];
        for _ in 0..100 {
            let w: [f64; 3] = std::array::from_fn(|i| (0..3).map(|j| c[i][j] * v[j]).sum());
            let length = w.iter().map(|x| x * x).sum::<f64>().sqrt();
            if length < 1e-12 {
                break;
            }
            v = w.map(|x| x / length);
        }
        v
    };
    let a = largest(&c);
    let la: f64 = (0..3)
        .map(|i| a[i] * (0..3).map(|j| c[i][j] * a[j]).sum::<f64>())
        .sum();
    let deflated: [[f64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| c[i][j] - la * a[i] * a[j]));
    let b = largest(&deflated);
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = n.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length < 1e-9 {
        // Collinear or a single atom: any direction across the line.
        return [0., 0., 1.];
    }
    n.map(|x| (x / length) as f32)
}

/// The surface cut and merged for `view`, ready to draw after the molecule.
pub(in crate::models3d) struct Prepared {
    positions: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
    front: Vec<bool>,
    visible: Visible,
}

impl Prepared {
    /// Cuts the model's surface for the view, or `None` without one or
    /// during the 2D → 3D morph.
    pub(in crate::models3d) fn new(doc: &Document, model: &Model3d, view: &View) -> Option<Self> {
        if model.morph.is_some_and(|m| m < 1.) {
            return None;
        }
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
                n[0] * eye[0] + n[1] * eye[1] + n[2] * eye[2] > 0.
            })
            .collect::<Vec<_>>();
        // The balls' light, from the upper left in front.
        let light = {
            let l = [-0.38f32, 0.42, 0.82];
            let length = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            l.map(|v| v / length)
        };
        let lightness: Vec<f32> = mesh
            .normals
            .iter()
            .map(|n| {
                let n = rotate(model.rotation, *n);
                0.35 + 0.65 * (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.)
            })
            .collect();
        let occluders = occluders(view, model);
        let visible = visible::visible(&Input {
            positions: &positions,
            triangles: &mesh.triangles,
            positive: &mesh.positive,
            front: &front,
            lightness: &lightness,
            occluders: &occluders,
            bands: BANDS,
            tolerance: TOLERANCE,
        });
        Some(Prepared {
            positions,
            triangles: mesh.triangles.clone(),
            front,
            visible,
        })
    }

    /// Whether nothing of the surface lies in front of the rotated point
    /// `q`, so a label there is drawn over the surface.
    pub(in crate::models3d) fn clear(&self, view: &View, q: [f32; 3]) -> bool {
        let [x, y, z] = screen(view, q);
        let input = Input {
            positions: &self.positions,
            triangles: &self.triangles,
            positive: &[],
            front: &self.front,
            lightness: &[],
            occluders: &[],
            bands: BANDS,
            tolerance: TOLERANCE,
        };
        visible::front_depth(&input, [x, y]).is_none_or(|depth| depth <= z)
    }

    /// The surface primitive: band layers per phase shaded from dark to
    /// light, faded toward `backdrop` by `fade`, then the outline.
    pub(in crate::models3d) fn primitive(
        self,
        dark: bool,
        backdrop: [f32; 3],
        fade: f32,
        outline: Option<Outline>,
    ) -> Primitive {
        let point = |p: &[f32; 2]| Point::new(p[0], p[1]);
        let layers = self
            .visible
            .layers
            .into_iter()
            .map(|layer| {
                let base = if layer.positive {
                    POSITIVE
                } else {
                    NEGATIVE[usize::from(dark)]
                };
                let t = (layer.band as f32 + 0.5) / BANDS as f32;
                let shaded = if t < 0.6 {
                    super::super::mix(base, super::super::BLACK, (0.6 - t) / 0.6 * 0.45)
                } else {
                    super::super::mix(base, super::super::WHITE, (t - 0.6) / 0.4 * 0.5)
                };
                let color = super::super::rgb(super::super::mix(shaded, backdrop, fade));
                let region = layer
                    .region
                    .iter()
                    .map(|c| c.iter().map(point).collect())
                    .collect();
                (region, color)
            })
            .collect();
        let rims = self
            .visible
            .rims
            .iter()
            .map(|r| r.iter().map(point).collect())
            .collect();
        Primitive::Surface {
            layers,
            rims,
            outline,
        }
    }
}

/// A rotated point in screen-depth space: drawing units across, and a depth
/// that keeps planes flat under perspective (design §0).
fn screen(view: &View, q: [f32; 3]) -> [f32; 3] {
    let (p, f) = view.project(q);
    [p.x, p.y, view.scale * f * q[2]]
}

/// The balls and half-bonds as painted, as convex solids in screen-depth
/// space (design §2).
fn occluders(view: &View, model: &Model3d) -> Vec<Occluder> {
    let depth = |q: [f32; 3], r: f32| screen(view, [q[0], q[1], q[2] + r])[2] - screen(view, q)[2];
    let mut out: Vec<Occluder> = view
        .balls()
        .filter_map(|(i, q, radius)| {
            let p = *view.rotated.get(i)?;
            let r = model.style.atom_radius(view.symbols.get(i)?);
            Some(Occluder::Ball {
                center: [q.x, q.y, screen(view, p)[2]],
                radius,
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
