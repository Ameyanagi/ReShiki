use super::{Graphic, Path, PathNodeType, Point, Segment, along, unit, vector};

#[cfg(test)]
#[path = "smooth_tests.rs"]
mod tests;

const SAMPLES: usize = 128;

#[derive(Clone, Copy)]
struct Curve {
    start: Point,
    controls: Option<[Point; 2]>,
    end: Point,
}
impl Curve {
    fn samples(self) -> Vec<Point> {
        // Lines are represented exactly by their endpoints.
        let count = if self.controls.is_some() { SAMPLES } else { 1 };
        (0..=count)
            .map(|sample| self.point(sample as f64 / count as f64))
            .collect()
    }

    fn point(self, t: f64) -> Point {
        let lerp = |a: Point, b: Point| {
            Point::new(
                (f64::from(a.x) * (1. - t) + f64::from(b.x) * t) as f32,
                (f64::from(a.y) * (1. - t) + f64::from(b.y) * t) as f32,
            )
        };
        match self.controls {
            Some([a, b]) => lerp(
                lerp(lerp(self.start, a), lerp(a, b)),
                lerp(lerp(a, b), lerp(b, self.end)),
            ),
            None => lerp(self.start, self.end),
        }
    }
}

struct Removal {
    node: usize,
    controls: Option<[Point; 2]>,
    spans: Vec<usize>,
    deviation: f64,
}

impl Graphic {
    /// Align gentle cubic joints and remove redundant interior path nodes.
    /// Returns false without rewriting metadata when the path is already smooth.
    pub fn smooth_path(&mut self) -> Result<bool, String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        let before = path.commands();
        let before_types = path.node_types.clone();
        let endpoints = path.nodes.iter().skip(1).chain(path.nodes.first());
        let original: Vec<_> = path
            .segments
            .iter()
            .zip(path.nodes.iter().zip(endpoints))
            .map(|(segment, (start, end))| Curve {
                start: start.point,
                controls: segment.controls,
                end: end.point,
            })
            .collect();
        if original.iter().any(|curve| {
            !curve.start.is_finite()
                || !curve.end.is_finite()
                || curve
                    .controls
                    .is_some_and(|controls| controls.iter().any(|point| !point.is_finite()))
        }) {
            return Err("Path coordinates are out of range".into());
        }
        let original: Vec<_> = original.into_iter().map(Curve::samples).collect();

        for node in 0..path.nodes.len() {
            let Some(previous) = path.previous_segment(node) else {
                continue;
            };
            let (Some(left), Some(right)) = (path.segments.get(previous), path.segments.get(node))
            else {
                continue;
            };
            let (left, right) = (left.controls, right.controls);
            let anchor = path.point(node)?;
            let (incoming, outgoing) = match (left, right) {
                (Some([_, a]), Some([b, _])) => (a, b),
                (None, Some([b, _])) => (path.point(previous)?, b),
                (Some([_, a]), None) => (a, path.point((node + 1) % path.nodes.len())?),
                (None, None) => continue,
            };
            let (Some((a, incoming_length)), Some((b, outgoing_length))) = (
                unit(vector(incoming, anchor)),
                unit(vector(anchor, outgoing)),
            ) else {
                continue;
            };
            let cross = a.0 * b.1 - a.1 * b.0;
            let dot = a.0 * b.0 + a.1 * b.1;
            if cross.abs().atan2(dot) + 1e-7 >= 20_f64.to_radians() {
                continue;
            }
            // A straight neighbour fixes the tangent; only rotate the cubic
            // handle. Mixed joints retain the node type detected by part 1.
            if left.is_none() || right.is_none() {
                if cross.abs() > 1e-7 {
                    path.set_controls(
                        node,
                        if left.is_none() {
                            [incoming, along(anchor, a, outgoing_length)]
                        } else {
                            [along(anchor, b, -incoming_length), outgoing]
                        },
                    );
                }
                continue;
            }
            let Some(&kind) = path.node_types.get(node) else {
                continue;
            };
            let kind = kind.max(PathNodeType::Smooth);
            // Avoid rounding an already aligned legacy path merely to persist
            // its detected type. The same guard makes repeated smoothing inert.
            if cross.abs() > 1e-7
                || (kind == PathNodeType::Symmetric
                    && (incoming_length - outgoing_length).abs()
                        > incoming_length.max(outgoing_length) * 1e-7)
            {
                path.align_node(node, kind);
            }
            if let Some(node_type) = path.node_types.get_mut(node) {
                *node_type = kind;
            }
        }

        let tolerance = f64::from(self.style.width()) * 0.25;
        // Retain the pre-Smooth segments through every merge so small errors
        // cannot accumulate beyond the tolerance.
        let mut spans: Vec<_> = (0..original.len()).map(|original| vec![original]).collect();
        let minimum = if path.closed {
            super::MIN_CLOSED_NODES
        } else {
            super::MIN_OPEN_NODES
        };
        while path.nodes.len() > minimum {
            let mut best: Option<Removal> = None;
            let nodes = if path.closed {
                0..path.nodes.len()
            } else {
                1..path.nodes.len() - 1
            };
            for node in nodes {
                if let Some(candidate) = removal(&path, node, &spans, &original)
                    && candidate.deviation <= tolerance
                    && best
                        .as_ref()
                        .is_none_or(|best| candidate.deviation < best.deviation)
                {
                    best = Some(candidate);
                }
            }
            let Some(best) = best else {
                break;
            };
            let Some(previous) = path.previous_segment(best.node) else {
                break;
            };
            let (Some(segment), Some(span)) =
                (path.segments.get_mut(previous), spans.get_mut(previous))
            else {
                break;
            };
            *segment = Segment {
                controls: best.controls,
                indices: None,
            };
            *span = best.spans;
            path.segments.remove(best.node);
            spans.remove(best.node);
            path.nodes.remove(best.node);
            path.node_types.remove(best.node);
        }
        if path.commands() == before && path.node_types == before_types {
            return Ok(false);
        }
        path.reindex();
        path.store(self)?;
        Ok(true)
    }
}

fn removal(
    path: &Path,
    node: usize,
    spans: &[Vec<usize>],
    original: &[Vec<Point>],
) -> Option<Removal> {
    let previous = path.previous_segment(node)?;
    let left = path.segments.get(previous)?;
    let right = path.segments.get(node)?;
    let start = path.nodes.get(previous)?.point;
    let anchor = path.nodes.get(node)?.point;
    let end = path.nodes.get((node + 1) % path.nodes.len())?.point;
    let (incoming, outgoing) = match (left.controls, right.controls) {
        (None, None) => (vector(start, anchor), vector(anchor, end)),
        (Some([_, incoming]), Some([outgoing, _])) => {
            (vector(incoming, anchor), vector(anchor, outgoing))
        }
        _ => return None,
    };
    let (incoming_length, outgoing_length) = match (unit(incoming), unit(outgoing)) {
        (Some((a, incoming_length)), Some((b, outgoing_length))) => {
            if (a.0 * b.1 - a.1 * b.0).abs().atan2(a.0 * b.0 + a.1 * b.1) + 1e-7
                >= 20_f64.to_radians()
            {
                return None;
            }
            (incoming_length, outgoing_length)
        }
        // Repeated line anchors are redundant. A collapsed cubic tangent
        // cannot establish a smooth join, so leave it to the point editor.
        _ if left.controls.is_none() => {
            (incoming.0.hypot(incoming.1), outgoing.0.hypot(outgoing.1))
        }
        _ => return None,
    };
    let total = incoming_length + outgoing_length;
    let split = if total > 1e-9 {
        incoming_length / total
    } else {
        0.5
    };
    let controls = match (left.controls, right.controls) {
        (Some([a, _]), Some([_, b])) => Some([
            along(start, vector(start, a), 1. / split),
            along(end, vector(end, b), 1. / (1. - split)),
        ]),
        _ => None,
    };
    if controls.is_some_and(|controls| controls.iter().any(|point| !point.is_finite())) {
        return None;
    }
    let spans: Vec<_> = spans
        .get(previous)?
        .iter()
        .chain(spans.get(node)?)
        .copied()
        .collect();
    let merged = Curve {
        start,
        controls,
        end,
    };
    let mut points = Vec::new();
    for &span in &spans {
        points.extend_from_slice(original.get(span)?);
    }
    let deviation = deviation(&points, &merged.samples());
    Some(Removal {
        node,
        controls,
        spans,
        deviation,
    })
}

// Two-sided geometric distance: a different traversal speed must not count as
// a shape change, and neither curve may hide an excursion from the other.
fn deviation(a: &[Point], b: &[Point]) -> f64 {
    let mut maximum = 0_f64;
    for (points, polyline) in [(a, b), (b, a)] {
        for &point in points {
            let distance = polyline
                .iter()
                .zip(polyline.iter().skip(1))
                .map(|(&start, &end)| {
                    let ab = vector(start, end);
                    let ap = vector(start, point);
                    let length_squared = ab.0 * ab.0 + ab.1 * ab.1;
                    let t = if length_squared > 0. {
                        ((ap.0 * ab.0 + ap.1 * ab.1) / length_squared).clamp(0., 1.)
                    } else {
                        0.
                    };
                    (ap.0 - t * ab.0).powi(2) + (ap.1 - t * ab.1).powi(2)
                })
                .fold(f64::INFINITY, f64::min);
            maximum = maximum.max(distance);
        }
    }
    maximum.sqrt()
}
