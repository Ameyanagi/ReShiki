//! Ring-relative delocalization strokes. Bond orders remain chemical data.
use crate::{
    aromatic::Circle,
    document::{Bond, Document, Point},
    graphics::{GraphicStyle, PathCommand},
    scene::Primitive,
};
use std::collections::HashSet;

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
fn eligible(b: &Bond) -> bool {
    matches!(b.order, 1 | 2 | 4)
        && (b.display == "plain" || b.order == 4 && b.projection)
        && b.stereo.is_none()
        && b.secondary_display.is_none()
}

pub fn toggle(doc: &mut Document, selected: &[u64]) -> Result<bool, String> {
    let selected: HashSet<_> = selected.iter().copied().collect();
    if doc
        .bonds
        .iter()
        .any(|b| b.ring_arc && selected.contains(&b.a) && selected.contains(&b.b))
    {
        for b in &mut doc.bonds {
            if selected.contains(&b.a) && selected.contains(&b.b) {
                b.ring_arc = false;
            }
        }
        return Ok(false);
    }
    let mut rings = crate::aromatic::ring_circles(doc, false);
    rings.sort_by_key(|r| r.atoms.len());
    let ring = rings.iter().find(|r| {
        selected.len() >= 3 && selected.iter().all(|id| r.atoms.contains(id))
            && doc.bonds.iter().filter(|b| r.contains_bond(b.a,b.b) && selected.contains(&b.a) && selected.contains(&b.b) && eligible(b)).count() >= 2
    }).ok_or("Select three or more consecutive atoms in one ring. Use plain bonds without assigned stereochemistry.")?;
    let edges: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| {
            ring.contains_bond(b.a, b.b) && selected.contains(&b.a) && selected.contains(&b.b)
        })
        .collect();
    if edges.iter().any(|b| !eligible(b)) || edges.len() + 1 < selected.len() {
        return Err("Select one continuous ring segment with plain bonds.".into());
    }
    for b in &mut doc.bonds {
        if ring.contains_bond(b.a, b.b) && selected.contains(&b.a) && selected.contains(&b.b) {
            b.ring_arc = true;
        }
    }
    Ok(true)
}

pub struct Arcs {
    pub primitives: Vec<Primitive>,
    pub(crate) owners: Vec<Vec<u64>>,
    pub bonds: HashSet<(u64, u64)>,
    pub(crate) crossings: Vec<(usize, crate::crossings::Gap)>,
}
impl Arcs {
    pub fn contains(&self, a: u64, b: u64) -> bool {
        self.bonds.contains(&pair(a, b))
    }
    pub fn intersects(&self, ring: &Circle) -> bool {
        self.bonds.iter().any(|(a, b)| ring.contains_bond(*a, *b))
    }
}

pub fn render(doc: &Document) -> Arcs {
    render_with_crossings(doc, true)
}
/// Original curve pieces for visibility preflight. Their geometry must not
/// depend on opaque crossing cuts that managed foreground ink can suppress.
pub(crate) fn uncut(doc: &Document) -> Arcs {
    render_with_crossings(doc, false)
}
fn render_with_crossings(doc: &Document, apply_crossings: bool) -> Arcs {
    let mut result = Arcs {
        primitives: vec![],
        owners: vec![],
        bonds: HashSet::new(),
        crossings: Vec::new(),
    };
    if !doc.bonds.iter().any(|b| b.ring_arc) {
        return result;
    }
    let mut rings = crate::aromatic::ring_circles(doc, false);
    rings.sort_by_key(|r| r.atoms.len());
    for ring in rings {
        let points: Vec<_> = ring
            .atoms
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| a.position))
            .collect();
        let projected = crate::aromatic::ring_plane(doc, &ring.atoms);
        let planar = projected
            .as_ref()
            .map(|(p, _)| p.as_slice())
            .unwrap_or(&points);
        let center = if projected.is_some() {
            Point::default()
        } else {
            ring.center
        };
        let angles: Vec<_> = planar
            .iter()
            .map(|p| (p.y - center.y).atan2(p.x - center.x))
            .collect();
        let edges: Vec<_> = ring
            .atoms
            .iter()
            .zip(ring.atoms.iter().cycle().skip(1))
            .map(|(a, b)| {
                doc.bonds.iter().find(|bond| {
                    pair(bond.a, bond.b) == pair(*a, *b)
                        && bond.ring_arc
                        && eligible(bond)
                        && !result.contains(*a, *b)
                })
            })
            .collect();
        if !edges.iter().any(Option::is_some) {
            continue;
        }
        // Start at a break so a run through the last/first atom stays continuous.
        let first = (0..edges.len())
            .find(|i| {
                let previous = (i + edges.len() - 1) % edges.len();
                edges.get(*i).and_then(|b| *b).map(|b| b.color)
                    != edges.get(previous).and_then(|b| *b).map(|b| b.color)
            })
            .unwrap_or(0);
        let mut step = 0;
        while step < edges.len() {
            let index = (first + step) % edges.len();
            let Some(b) = edges.get(index).and_then(|b| *b) else {
                step += 1;
                continue;
            };
            let Some(start) = angles.get(index).copied() else {
                break;
            };
            let color = b.color;
            let mut sweep = 0.;
            let mut count = 0;
            while step + count < edges.len() {
                let i = (first + step + count) % edges.len();
                let Some(b) = edges.get(i).and_then(|b| *b).filter(|b| b.color == color) else {
                    break;
                };
                let (Some(a), Some(z)) = (angles.get(i), angles.get((i + 1) % angles.len())) else {
                    break;
                };
                sweep += (z - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                result.bonds.insert(pair(b.a, b.b));
                count += 1;
            }
            let trim = 0.16_f32.min(sweep.abs() * 0.15);
            let before = (first + step + edges.len() - 1) % edges.len();
            let after = (first + step + count) % edges.len();
            let start_trim = if edges.get(before).is_some_and(Option::is_some) {
                0.
            } else {
                trim
            };
            let end_trim = if edges.get(after).is_some_and(Option::is_some) {
                0.
            } else {
                trim
            };
            let start = start + start_trim * sweep.signum();
            let sweep = sweep - (start_trim + end_trim) * sweep.signum();
            let stroke = Primitive::Path {
                commands: arc(&ring, start, sweep),
                style: GraphicStyle {
                    stroke: color,
                    width_pt: ring.width_pt,
                    ..Default::default()
                },
                filled: false,
            };
            let (stroke, gaps) = if apply_crossings {
                crate::crossings::ring_stroke(doc, &ring, stroke)
            } else {
                (stroke, vec![])
            };
            result.primitives.push(stroke);
            result.owners.push(ring.atoms.clone());
            result.crossings.extend(gaps);
            step += count.max(1);
        }
    }
    result
}

fn arc(ring: &Circle, start: f32, sweep: f32) -> Vec<PathCommand> {
    let [u, v] = ring
        .projected_axes
        .unwrap_or([Point::new(1., 0.), Point::new(0., 1.)]);
    let at = |x: f32, y: f32| {
        ring.center.offset(
            ring.radius * (u.x * x + v.x * y),
            ring.radius * (u.y * x + v.y * y),
        )
    };
    let n = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.) as usize;
    let delta = sweep / n as f32;
    let mut commands = vec![PathCommand::Move(at(start.cos(), start.sin()))];
    for i in 0..n {
        let a = start + i as f32 * delta;
        let b = a + delta;
        let k = 4. / 3. * (delta / 4.).tan();
        commands.push(PathCommand::Cubic(
            at(a.cos() - k * a.sin(), a.sin() + k * a.cos()),
            at(b.cos() + k * b.sin(), b.sin() - k * b.cos()),
            at(b.cos(), b.sin()),
        ));
    }
    commands
}

#[cfg(test)]
mod tests;
