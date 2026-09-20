//! Carbon-chain geometry shared by the live preview and committed drawing.
use crate::document::{Document, Point};
use std::f32::consts::PI;

pub const MAX_ATOMS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainMode {
    Straight,
    Snaking,
}
impl std::fmt::Display for ChainMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Straight => "Straight chain",
            Self::Snaking => "Snaking chain",
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BondDrawing {
    pub fixed_length: bool,
    pub fixed_angles: bool,
    pub length: f32,
}
impl Default for BondDrawing {
    fn default() -> Self {
        Self {
            fixed_length: true,
            fixed_angles: true,
            length: crate::style::DEFAULT.bond_length_world,
        }
    }
}
impl BondDrawing {
    pub fn unconstrained(mut self, free: bool) -> Self {
        if free {
            self.fixed_length = false;
            self.fixed_angles = false;
        }
        self
    }
    pub fn angle(self, angle: f32) -> f32 {
        if self.fixed_angles {
            (angle / (PI / 12.)).round() * (PI / 12.)
        } else {
            angle
        }
    }
    pub fn endpoint(self, start: Point, cursor: Point) -> Point {
        let angle = self.angle(direction(start, cursor));
        let length = if self.fixed_length {
            self.length
        } else {
            start.distance(cursor)
        };
        start.offset(length * angle.cos(), length * angle.sin())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChainDrawing {
    /// Total atoms along the chain, including any existing attachment endpoints.
    pub atoms: Option<usize>,
    pub angle: f32,
}
impl Default for ChainDrawing {
    fn default() -> Self {
        Self {
            atoms: None,
            angle: 120.,
        }
    }
}
impl ChainDrawing {
    pub fn limit(self, _attached: bool) -> usize {
        self.atoms.unwrap_or(MAX_ATOMS).clamp(1, MAX_ATOMS) - 1
    }
}

pub fn direction(start: Point, end: Point) -> f32 {
    (end.y - start.y).atan2(end.x - start.x)
}

/// An alternating zigzag around the dragged axis. Free length fits its end-to-end
/// distance; with both constraints disabled it ends exactly at the pointer.
pub fn straight(
    start: Point,
    cursor: Point,
    attached: bool,
    bond: BondDrawing,
    chain: ChainDrawing,
    flip: bool,
) -> Vec<Point> {
    let half_turn = (180. - chain.angle.clamp(1., 179.)).to_radians() / 2.;
    let advance = bond.length * half_turn.cos();
    let count = chain
        .atoms
        .map(|_| chain.limit(attached))
        .unwrap_or_else(|| {
            ((start.distance(cursor) / advance).round() as usize).clamp(1, chain.limit(attached))
        });
    let side = if flip { 1. } else { -1. };
    let offset = if count % 2 == 1 {
        side * bond.length * half_turn.sin()
    } else {
        0.
    };
    let x = count as f32 * advance;
    let mut axis = bond.angle(direction(start, cursor));
    if !bond.fixed_angles && count > 0 {
        axis -= offset.atan2(x);
    }
    let scale = if !bond.fixed_length && count > 0 {
        start.distance(cursor) / x.hypot(offset)
    } else {
        1.
    };
    let mut points = vec![start];
    for i in 0..count {
        let angle = axis
            + if i % 2 == 0 {
                side * half_turn
            } else {
                -side * half_turn
            };
        let Some(last) = points.last().copied() else {
            break;
        };
        points.push(last.offset(
            bond.length * scale * angle.cos(),
            bond.length * scale * angle.sin(),
        ));
    }
    points
}

/// Grow at chemically conventional turns toward the pointer. Retracing a recent
/// vertex removes the tail, so a gesture can be corrected before mouse release.
pub fn snake(
    points: &mut Vec<Point>,
    cursor: Point,
    attached: bool,
    bond: BondDrawing,
    chain: ChainDrawing,
    flip: bool,
) {
    if points.is_empty() {
        return;
    }
    if let Some(i) = points
        .iter()
        .take(points.len().saturating_sub(1))
        .rposition(|p| p.distance(cursor) < bond.length * 0.55)
    {
        points.truncate(i + 1);
        return;
    }
    let turn = (180. - chain.angle.clamp(1., 179.)).to_radians();
    while points.len() - 1 < chain.limit(attached) {
        let Some(last) = points.last().copied() else {
            break;
        };
        let distance = last.distance(cursor);
        if distance < bond.length * 0.85 {
            break;
        }
        let angle = if !bond.fixed_angles {
            direction(last, cursor)
        } else if points.len() == 1 {
            bond.angle(direction(last, cursor)) + if flip { turn / 2. } else { -turn / 2. }
        } else {
            let Some(previous) = points.get(points.len().saturating_sub(2)) else {
                break;
            };
            let incoming = direction(*previous, last);
            [incoming - turn, incoming + turn]
                .into_iter()
                .min_by(|a, b| {
                    let distance = |angle: f32| {
                        last.offset(bond.length * angle.cos(), bond.length * angle.sin())
                            .distance(cursor)
                    };
                    distance(*a).total_cmp(&distance(*b))
                })
                .unwrap_or(incoming)
        };
        let length = if !bond.fixed_length && distance < bond.length * 1.5 {
            distance
        } else {
            bond.length
        };
        let next = last.offset(length * angle.cos(), length * angle.sin());
        if next.distance(cursor) >= distance
            || points.iter().any(|p| p.distance(next) < bond.length * 0.15)
        {
            break;
        }
        points.push(next);
    }
}

/// Reuse only explicit endpoint targets. Interior near misses never silently
/// merge atoms or change existing bond orders.
pub fn place(
    doc: &Document,
    points: &[Point],
    source: Option<u64>,
    target: Option<u64>,
    radius: f32,
) -> Result<(Document, Vec<u64>), String> {
    if points.is_empty()
        || points.len() > MAX_ATOMS + 1
        || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err("Invalid chain geometry".into());
    }
    if source.is_some_and(|id| doc.atom(id).is_none())
        || target.is_some_and(|id| doc.atom(id).is_none())
    {
        return Err("The chain attachment atom no longer exists".into());
    }
    let mut candidate = doc.clone();
    let mut selected = Vec::new();
    for (i, position) in points.iter().enumerate() {
        let endpoint = if i == 0 {
            source
        } else if i + 1 == points.len() {
            target
        } else {
            None
        };
        let p = endpoint
            .and_then(|id| doc.atom(id).map(|a| a.position))
            .unwrap_or(*position);
        if endpoint.is_none()
            && candidate
                .atoms
                .iter()
                .any(|a| a.position.distance(p) < radius.min(8.))
        {
            return Err(
                "Chain overlaps an atom · Release on an endpoint to connect, or change direction"
                    .into(),
            );
        }
        let id = endpoint.unwrap_or_else(|| candidate.add_atom("C", p));
        if let Some(&previous) = selected.last() {
            if previous == id
                || candidate
                    .bonds
                    .iter()
                    .any(|b| (b.a == previous && b.b == id) || (b.a == id && b.b == previous))
            {
                return Err("Chain would repeat an existing bond".into());
            }
            candidate.add_bond(previous, id, 1, "plain");
        }
        selected.push(id);
    }
    candidate.reconcile_molecule_groups();
    candidate.validate()?;
    Ok((candidate, selected))
}
