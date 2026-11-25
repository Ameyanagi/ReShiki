use super::{
    AREA, Buckets, Piece, Polygon, area, batches, boxes_overlap, edge_distance,
    intersection_centroid, split_polygon,
};

const FRAGMENT_LIMIT: usize = 4096;

pub(super) fn in_front(pieces: &[Piece], i: usize, j: usize, point: [f64; 2]) -> bool {
    let dz = pieces[i].z.at(point) - pieces[j].z.at(point);
    dz > 0. || (dz == 0. && i < j)
}

/// Each entry holds a nearer front piece and the area of its overlap.
pub(super) fn nearer_pieces(
    pieces: &[Piece],
    grid: &Buckets,
    front: &[bool],
) -> Vec<Vec<(usize, f64)>> {
    let comparisons = batches(pieces.len(), 512, |range| {
        let mut seen = vec![usize::MAX; pieces.len()];
        let mut candidates = Vec::new();
        let mut scratch = [Vec::with_capacity(16), Vec::with_capacity(16)];
        let mut comparisons = Vec::new();
        for i in range {
            let piece = &pieces[i];
            grid.candidates(piece.bounds, &mut seen, i, &mut candidates);
            for &j in &candidates {
                let other = &pieces[j];
                if j <= i
                    || piece.triangle == other.triangle
                    || !(front[piece.triangle] || front[other.triangle])
                    || !boxes_overlap(piece.bounds, other.bounds)
                {
                    continue;
                }
                let Some((center, overlap)) =
                    intersection_centroid(&piece.polygon, &other.polygon, &mut scratch)
                else {
                    continue;
                };
                let (near, far) = if in_front(pieces, j, i, center) {
                    (j, i)
                } else {
                    (i, j)
                };
                if front[pieces[near].triangle] {
                    comparisons.push((far, near, overlap));
                }
            }
        }
        comparisons
    });
    let mut nearer = vec![Vec::new(); pieces.len()];
    for (far, near, overlap) in comparisons.into_iter().flatten() {
        nearer[far].push((near, overlap));
    }
    nearer
}

pub(super) fn fully_covered(i: usize, pieces: &[Piece], nearer: &[Vec<(usize, f64)>]) -> bool {
    if nearer[i].is_empty() {
        return false;
    }
    let target = area(&pieces[i].polygon) - AREA;
    if nearer[i].iter().any(|&(_, overlap)| overlap >= target) {
        return true;
    }
    // Only disjoint intersections can contribute their full area to the sum.
    let mut disjoint: Vec<usize> = Vec::new();
    let mut total = 0.;
    for &(j, overlap) in &nearer[i] {
        if disjoint.iter().all(|&k| {
            !nearer[j].iter().any(|&(n, _)| n == k) && !nearer[k].iter().any(|&(n, _)| n == j)
        }) {
            total += overlap;
            if total >= target {
                return true;
            }
            disjoint.push(j);
        }
    }
    false
}

struct Fragment {
    polygon: Polygon,
    bounds: [f32; 4],
    depth: usize,
}

impl Fragment {
    fn new(polygon: Polygon, depth: usize) -> Self {
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for p in &polygon {
            for k in 0..2 {
                bounds[k] = bounds[k].min(p[k] as f32);
                bounds[k + 2] = bounds[k + 2].max(p[k] as f32);
            }
        }
        Self {
            polygon,
            bounds,
            depth,
        }
    }
}

/// Visit completed fragments immediately so a work-limit hit can preserve them.
pub(super) fn resolve(
    piece: &Piece,
    covers: &[(usize, f64)],
    pieces: &[Piece],
    groups: usize,
    has_lines: bool,
    mut emit: impl FnMut(Polygon, usize),
) -> (usize, bool) {
    if covers.is_empty() {
        emit(piece.polygon.clone(), 0);
        return (1, false);
    }
    let mut pending = vec![(
        0,
        Fragment {
            polygon: piece.polygon.clone(),
            bounds: piece.bounds,
            depth: 0,
        },
    )];
    let mut kept = 0;
    while let Some((start, fragment)) = pending.pop() {
        let Some((index, &(j, _))) = covers
            .iter()
            .enumerate()
            .skip(start)
            .find(|&(_, &(j, _))| boxes_overlap(fragment.bounds, pieces[j].bounds))
        else {
            emit(fragment.polygon, fragment.depth);
            kept += 1;
            continue;
        };
        let cover = &pieces[j].polygon;
        let mut inside = fragment.polygon;
        for (&p, &q) in cover.iter().zip(cover.iter().cycle().skip(1)) {
            let (remainder, outside) = split_polygon(inside, |v| edge_distance(p, q, v));
            if area(&outside) > AREA {
                pending.push((index + 1, Fragment::new(outside, fragment.depth)));
                if kept + pending.len() > FRAGMENT_LIMIT {
                    return (kept, true);
                }
            }
            inside = remainder;
            if area(&inside) <= AREA {
                break;
            }
        }
        if area(&inside) <= AREA {
            continue;
        }
        if fragment.depth + 1 == groups {
            // A discarded interior can still own a line on its shared boundary.
            if has_lines {
                emit(inside, groups);
            }
        } else {
            pending.push((index + 1, Fragment::new(inside, fragment.depth + 1)));
            if kept + pending.len() > FRAGMENT_LIMIT {
                return (kept, true);
            }
        }
    }
    (kept, false)
}
