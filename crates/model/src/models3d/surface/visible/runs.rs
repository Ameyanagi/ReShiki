use std::collections::{BTreeMap, HashSet};

use super::{Input, Line, Segment, along, chain, segment_interval};

pub(super) struct LineInterval {
    pub source: usize,
    pub depth: usize,
    pub interval: (f64, f64),
}

/// Split a rim by the number of nearer front triangles covering each span.
pub(super) fn rim_segments(
    a: [f64; 2],
    b: [f64; 2],
    covers: impl Iterator<Item = (usize, (f64, f64))>,
    rims: &mut [Vec<Segment>],
) {
    if rims.is_empty() {
        return;
    }
    let mut intervals: Vec<_> = covers
        .filter_map(|(triangle, (start, end))| {
            let start = start.max(0.);
            let end = end.min(1.);
            (start < end).then_some((triangle, start, end))
        })
        .collect();
    intervals.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    // Clipping can split one triangle along a cut that lies on this rim.
    // Its pieces then cover the same interval, but the triangle counts once.
    let mut united: Vec<(usize, f64, f64)> = Vec::with_capacity(intervals.len());
    for (triangle, start, end) in intervals {
        if let Some(last) = united.last_mut()
            && last.0 == triangle
            && start <= last.2
        {
            last.2 = last.2.max(end);
        } else {
            united.push((triangle, start, end));
        }
    }
    let mut events = Vec::with_capacity(2 * united.len());
    for (_, start, end) in united {
        events.push((start, true));
        events.push((end, false));
    }
    events.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let mut cursor = 0.;
    let mut count = 0;
    let mut event = 0;
    while event < events.len() {
        let t = events[event].0;
        if cursor < t
            && let Some(segments) = rims.get_mut(count)
        {
            segments.push((along(a, b, cursor), along(a, b, t)));
        }
        // Endings and beginnings at one point affect only the next span.
        while event < events.len() && events[event].0 == t {
            if events[event].1 {
                count += 1;
            } else {
                count -= 1;
            }
            event += 1;
        }
        cursor = t;
    }
    if cursor < 1.
        && let Some(segments) = rims.get_mut(count)
    {
        segments.push((along(a, b, cursor), b));
    }
}

/// Clip a source mesh line to one fragment of its own triangle.
pub(super) fn clip_line(line: &Line, contour: &[[f64; 2]]) -> Option<(f64, f64)> {
    if contour.len() < 3 {
        return None;
    }
    let [a, b] = line.ends.map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let (start, end) = segment_interval(a, b, contour)?;
    ((end - start) * (b[0] - a[0]).hypot(b[1] - a[1]) > 1e-9).then_some((start, end))
}

/// Keep shared-boundary spans at their deepest count, then chain each group.
pub(super) fn chain_lines(
    segments: Vec<LineInterval>,
    input: &Input<'_>,
) -> Vec<[Vec<Vec<[f32; 2]>>; 2]> {
    let depths = input.tones.len();
    let mut resolved = vec![[Vec::new(), Vec::new()]; depths];
    let mut sources = vec![Vec::new(); input.lines.len()];
    for span in segments {
        sources[span.source].push((span.depth, span.interval));
    }
    for (line, spans) in input.lines.iter().zip(sources) {
        if spans.is_empty() {
            continue;
        }
        let sign = usize::from(!input.positive[line.triangle as usize]);
        let [a, b] = line.ends.map(|p| [f64::from(p[0]), f64::from(p[1])]);
        let mut events = Vec::with_capacity(2 * spans.len());
        for (depth, (start, end)) in spans {
            if start < end {
                events.push((start, depth, true));
                events.push((end, depth, false));
            }
        }
        events.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut active = BTreeMap::<usize, usize>::new();
        let mut intervals = Vec::<(usize, f64, f64)>::new();
        let mut cursor = 0.;
        let mut event = 0;
        while event < events.len() {
            let t = events[event].0;
            if cursor < t
                && let Some((&depth, _)) = active.last_key_value()
                && depth < depths
            {
                // Independent clips of a shared cut can leave a roundoff gap.
                if let Some((previous, _, end)) = intervals.last_mut()
                    && *previous == depth
                    && cursor - *end < 1e-6
                {
                    *end = t;
                } else {
                    intervals.push((depth, cursor, t));
                }
            }
            while event < events.len() && events[event].0 == t {
                let (_, depth, start) = events[event];
                if start {
                    *active.entry(depth).or_default() += 1;
                } else if let Some(count) = active.get_mut(&depth) {
                    *count -= 1;
                    if *count == 0 {
                        active.remove(&depth);
                    }
                }
                event += 1;
            }
            cursor = t;
        }
        let at = |t| {
            let point = along(a, b, t);
            let rounded = point.map(|v| f64::from(v as f32));
            // Neighbouring triangles share the projected f32 endpoint.
            if rounded == a || rounded == b {
                rounded
            } else {
                point
            }
        };
        for (depth, start, end) in intervals {
            resolved[depth][sign].push((at(start), at(end)));
        }
    }
    resolved
        .into_iter()
        .map(|groups| {
            groups.map(|segments| {
                let mut seen = HashSet::new();
                let segments = segments
                    .into_iter()
                    .filter(|&(a, b)| {
                        let [ka, kb] = [a, b].map(endpoint_key);
                        let key = if ka <= kb { (ka, kb) } else { (kb, ka) };
                        seen.insert(key)
                    })
                    .collect();
                chain(segments, f64::from(input.tolerance))
            })
        })
        .collect()
}

pub(super) fn endpoint_key(point: [f64; 2]) -> (i64, i64) {
    (
        (point[0] * 1e4).round() as i64,
        (point[1] * 1e4).round() as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_line_bridges_roundoff_between_same_depth_fragments() {
        let input = Input {
            positions: &[],
            triangles: &[],
            positive: &[true],
            front: &[],
            lightness: &[],
            occluders: &[],
            tones: &[0],
            lines: &[Line {
                triangle: 0,
                ends: [[0., 0., 0.], [1., 0., 0.]],
            }],
            tolerance: 0.,
        };
        let spans =
            Vec::from(
                [(0., 0.500_049_8), (0.500_050_2, 1.)].map(|interval| LineInterval {
                    source: 0,
                    depth: 0,
                    interval,
                }),
            );
        assert_eq!(chain_lines(spans, &input)[0][0], [vec![[0., 0.], [1., 0.]]]);
    }
}
