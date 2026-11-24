//! Compact snapshots of the merged 2D shapes of an orbital surface.

use std::borrow::Cow;

use base64::{Engine, engine::general_purpose::STANDARD};
use miniz_oxide::{
    deflate::compress_to_vec,
    inflate::{
        TINFLStatus,
        core::{
            DecompressorOxide, decompress, inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        },
    },
};

use crate::document::Point;
use crate::polygon::{closed_edges, signed_area};
use crate::scene::{Outline, Primitive};

const MAGIC: &[u8; 4] = b"RSF1";
const MAX_INFLATED_BYTES: usize = 16 * 1024 * 1024;
const MIN_CONTOUR_POINTS: usize = 3;
const MIN_RIM_POINTS: usize = 2;

/// Largest encoded snapshot, in bytes.
pub const MAX_BYTES: usize = 300 * 1024;

/// Encodes a surface with points relative to `origin`, the model frame's centre.
/// If the original encoding exceeds the size limits, thins contours and rims
/// with Douglas–Peucker, doubling the tolerance from 0.1 to 12.8 drawing units.
/// Containment between separately thinned contours is not checked. Returns
/// `None` for an invalid outline, a primitive other than a surface, or a surface
/// that cannot fit in the snapshot.
pub fn encode(surface: &Primitive, origin: Point) -> Option<String> {
    let Primitive::Surface {
        layers,
        rims,
        outline,
    } = surface
    else {
        return None;
    };
    if outline
        .as_ref()
        .is_some_and(|outline| !valid_outline(outline))
    {
        return None;
    }

    let layers = layers.iter().filter(|(contours, _)| {
        contours
            .iter()
            .any(|contour| contour.len() >= MIN_CONTOUR_POINTS)
    });
    let layer_count = u32::try_from(layers.clone().count()).ok()?.to_le_bytes();
    let mut bytes = Vec::new();
    for tolerance in [0., 0.1, 0.2, 0.4, 0.8, 1.6, 3.2, 6.4, 12.8] {
        bytes.clear();
        bytes.extend_from_slice(MAGIC);
        bytes.push(u8::from(outline.is_some()));
        if let Some(outline) = outline {
            bytes.extend_from_slice(&outline.color);
            bytes.extend_from_slice(&outline.alpha.to_le_bytes());
            bytes.extend_from_slice(&outline.width.to_le_bytes());
        }
        bytes.extend_from_slice(&layer_count);
        for (contours, color) in layers.clone() {
            bytes.extend_from_slice(color);
            write_paths(&mut bytes, contours, origin, tolerance, true)?;
        }
        write_paths(&mut bytes, rims, origin, tolerance, false)?;

        if bytes.len() <= MAX_INFLATED_BYTES {
            let packed = compress_to_vec(&bytes, 9);
            if packed.len().div_ceil(3) * 4 <= MAX_BYTES {
                return Some(STANDARD.encode(packed));
            }
        }
    }
    None
}

fn valid_outline(outline: &Outline) -> bool {
    outline.width.is_finite() && outline.width >= 0. && (0. ..=1.).contains(&outline.alpha)
}

fn write_paths(
    bytes: &mut Vec<u8>,
    paths: &[Vec<Point>],
    origin: Point,
    tolerance: f64,
    contour: bool,
) -> Option<()> {
    let min_points = if contour {
        MIN_CONTOUR_POINTS
    } else {
        MIN_RIM_POINTS
    };
    let paths = paths.iter().filter(|path| path.len() >= min_points);
    bytes.extend_from_slice(&u32::try_from(paths.clone().count()).ok()?.to_le_bytes());
    for path in paths {
        let points = thin(path, tolerance, contour);
        bytes.extend_from_slice(&u32::try_from(points.len()).ok()?.to_le_bytes());
        for point in points.iter() {
            let relative = point.offset(-origin.x, -origin.y);
            if !relative.is_finite() {
                return None;
            }
            bytes.extend_from_slice(&relative.x.to_le_bytes());
            bytes.extend_from_slice(&relative.y.to_le_bytes());
        }
    }
    Some(())
}

fn thin(points: &[Point], tolerance: f64, contour: bool) -> Cow<'_, [Point]> {
    let repeated_end = points.len() > 1 && points.first() == points.last();
    let closed = contour || repeated_end;
    let len = points.len() - usize::from(repeated_end);
    if tolerance == 0. || len <= if closed { 3 } else { 2 } {
        return Cow::Borrowed(points);
    }

    let last = if closed { len } else { len - 1 };
    let point_at = |i| points.get(if i == len { 0 } else { i });
    let farthest = |first, last| {
        let (a, b) = (point_at(first)?, point_at(last)?);
        let interior = points.get(first + 1..last)?;
        let (dx, dy) = (
            f64::from(b.x) - f64::from(a.x),
            f64::from(b.y) - f64::from(a.y),
        );
        let length_sq = dx * dx + dy * dy;
        interior
            .iter()
            .enumerate()
            .map(|(offset, p)| {
                let (x, y) = (
                    f64::from(p.x) - f64::from(a.x),
                    f64::from(p.y) - f64::from(a.y),
                );
                let t = if length_sq > 0. {
                    ((x * dx + y * dy) / length_sq).clamp(0., 1.)
                } else {
                    0.
                };
                (
                    first + 1 + offset,
                    (x - t * dx).powi(2) + (y - t * dy).powi(2),
                )
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
    };

    let mut stack = if closed {
        // Split the ring at opposite vertices, then keep a third vertex even
        // when both arcs are within tolerance of the same chord.
        let Some((opposite, _)) = farthest(0, last) else {
            return Cow::Borrowed(points);
        };
        let Some((third, _)) = farthest(0, opposite)
            .into_iter()
            .chain(farthest(opposite, last))
            .max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            return Cow::Borrowed(points);
        };
        let (a, b) = (opposite.min(third), opposite.max(third));
        vec![(b, last), (a, b), (0, a)]
    } else {
        vec![(0, last)]
    };
    let tolerance_sq = tolerance * tolerance;
    let mut simplified = Vec::new();
    while let Some((first, last)) = stack.pop() {
        if let Some((i, distance_sq)) = farthest(first, last)
            && distance_sq > tolerance_sq
        {
            // Visit the left segment first to emit points in path order.
            stack.extend([(i, last), (first, i)]);
        } else if let Some(&point) = points.get(first) {
            simplified.push(point);
        }
    }
    simplified.extend(points.get(last).copied());
    if simplified.len() == points.len() {
        return Cow::Borrowed(points);
    }
    if contour {
        let mut ring = simplified.as_slice();
        if repeated_end && let Some((_, vertices)) = ring.split_last() {
            ring = vertices;
        }
        let area = signed_area(points);
        let thinned_area = signed_area(ring);
        if !(area * thinned_area > 0.
            && (thinned_area - area).abs() <= 0.1 * area.abs()
            && !self_intersects(ring))
        {
            return Cow::Borrowed(points);
        }
    }
    Cow::Owned(simplified)
}

fn self_intersects(points: &[Point]) -> bool {
    let mut edges: Vec<_> = closed_edges(points).enumerate().collect();
    edges.sort_unstable_by(|(_, (a, b)), (_, (c, d))| a.x.min(b.x).total_cmp(&c.x.min(d.x)));
    let side = |a: Point, b: Point, p: Point| {
        (f64::from(b.x) - f64::from(a.x)) * (f64::from(p.y) - f64::from(a.y))
            - (f64::from(b.y) - f64::from(a.y)) * (f64::from(p.x) - f64::from(a.x))
    };
    for (i, &(first, (a, b))) in edges.iter().enumerate() {
        for &(second, (c, d)) in edges.iter().skip(i + 1) {
            if a.x.max(b.x) < c.x.min(d.x) {
                break;
            }
            let distance = first.abs_diff(second);
            if distance != 1
                && distance != points.len() - 1
                && a.y.min(b.y) <= c.y.max(d.y)
                && c.y.min(d.y) <= a.y.max(b.y)
                && side(a, b, c) * side(a, b, d) <= 0.
                && side(c, d, a) * side(c, d, b) <= 0.
            {
                return true;
            }
        }
    }
    false
}

fn inflate(mut packed: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = Box::<DecompressorOxide>::default();
    let mut data = vec![0; packed.len().min(MAX_INFLATED_BYTES).next_power_of_two()];
    let mut written = 0;
    loop {
        let (status, consumed, produced) = decompress(
            &mut decoder,
            packed,
            &mut data,
            written,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        written += produced;
        packed = packed
            .get(consumed..)
            .ok_or("Invalid snapshot deflate length")?;
        match status {
            TINFLStatus::Done if packed.is_empty() => {
                data.truncate(written);
                return Ok(data);
            }
            TINFLStatus::Done => return Err("Trailing compressed snapshot data".into()),
            TINFLStatus::HasMoreOutput if data.len() < MAX_INFLATED_BYTES => {
                data.resize(data.len() * 2, 0);
            }
            _ => return Err("Invalid or oversized snapshot deflate data".into()),
        }
    }
}

/// Decodes the surface and places it at `origin`.
pub fn decode(text: &str, origin: Point) -> Result<Primitive, String> {
    if text.len() > MAX_BYTES {
        return Err("Snapshot is too large".into());
    }
    let packed = STANDARD
        .decode(text)
        .map_err(|_| "Invalid snapshot base64")?;
    let data = inflate(&packed)?;
    let mut bytes = data.as_slice();
    if &take::<4>(&mut bytes)? != MAGIC {
        return Err("Invalid snapshot magic".into());
    }
    let outline = match take(&mut bytes)? {
        [0] => None,
        [1] => {
            let outline = Outline {
                color: take(&mut bytes)?,
                alpha: f32::from_le_bytes(take(&mut bytes)?),
                width: f32::from_le_bytes(take(&mut bytes)?),
            };
            if !valid_outline(&outline) {
                return Err("Invalid snapshot outline".into());
            }
            Some(outline)
        }
        _ => return Err("Invalid snapshot outline flag".into()),
    };
    // Each layer needs a color, both counts, and at least one contour.
    let layer_count = read_count(&mut bytes, 3 + 4 + 4 + 8 * MIN_CONTOUR_POINTS)?;
    let mut layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let color = take(&mut bytes)?;
        let contours = read_paths(&mut bytes, origin, MIN_CONTOUR_POINTS)?;
        if contours.is_empty() {
            return Err("Snapshot layer has no contours".into());
        }
        layers.push((contours, color));
    }
    let rims = read_paths(&mut bytes, origin, MIN_RIM_POINTS)?;
    if !bytes.is_empty() {
        return Err("Trailing snapshot data".into());
    }
    Ok(Primitive::Surface {
        layers,
        rims,
        outline,
    })
}

fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], String> {
    let (head, tail) = bytes.split_first_chunk().ok_or("Truncated snapshot")?;
    *bytes = tail;
    Ok(*head)
}

fn read_count(bytes: &mut &[u8], item_bytes: usize) -> Result<usize, String> {
    let count = u32::from_le_bytes(take(bytes)?) as usize;
    if count > bytes.len() / item_bytes {
        return Err("Snapshot count exceeds remaining data".into());
    }
    Ok(count)
}

fn read_paths(
    bytes: &mut &[u8],
    origin: Point,
    min_points: usize,
) -> Result<Vec<Vec<Point>>, String> {
    let path_count = read_count(bytes, 4 + 8 * min_points)?;
    let mut paths = Vec::with_capacity(path_count);
    for _ in 0..path_count {
        let point_count = read_count(bytes, 8)?;
        if point_count < min_points {
            return Err("Snapshot path has too few points".into());
        }
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            let x = f32::from_le_bytes(take(bytes)?);
            let y = f32::from_le_bytes(take(bytes)?);
            let point = origin.offset(x, y);
            if !point.is_finite() {
                return Err("Non-finite snapshot coordinate".into());
            }
            points.push(point);
        }
        paths.push(points);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_bytes(bytes: &[u8]) -> Result<Primitive, String> {
        decode(
            &STANDARD.encode(compress_to_vec(bytes, 9)),
            Point::default(),
        )
    }

    fn small_surface(outline: Option<Outline>) -> Primitive {
        Primitive::Surface {
            layers: vec![
                (
                    vec![vec![
                        Point::new(8., -5.),
                        Point::new(11., -5.),
                        Point::new(11., -3.),
                        Point::new(8., -3.),
                    ]],
                    [120, 180, 220],
                ),
                (
                    vec![vec![
                        Point::new(9., -4.75),
                        Point::new(10.5, -4.75),
                        Point::new(9.25, -4.),
                    ]],
                    [25, 45, 60],
                ),
            ],
            rims: vec![vec![Point::new(8., -3.), Point::new(11., -3.)]],
            outline,
        }
    }

    fn assert_surface_eq(expected: Primitive, actual: Primitive) {
        let fields = |surface| match surface {
            Primitive::Surface {
                layers,
                rims,
                outline,
            } => (layers, rims, outline),
            _ => panic!("Expected a surface"),
        };
        assert_eq!(fields(actual), fields(expected));
    }

    #[test]
    fn round_trip() {
        let origin = Point::new(10., -4.);
        let outline = Outline {
            color: [26, 33, 41],
            alpha: 0.45,
            width: 0.8,
        };
        for outline in [Some(outline), None] {
            let surface = small_surface(outline);
            let text = encode(&surface, origin).unwrap();
            assert_surface_eq(surface, decode(&text, origin).unwrap());
        }
    }

    #[test]
    fn origin_shift() {
        let origin = Point::new(10., -4.);
        let moved_origin = Point::new(-6., 3.5);
        let mut surface = small_surface(None);
        let text = encode(&surface, origin).unwrap();
        let Primitive::Surface { layers, rims, .. } = &mut surface else {
            panic!("Expected a surface");
        };
        for point in layers
            .iter_mut()
            .flat_map(|(contours, _)| contours)
            .chain(rims)
            .flatten()
        {
            *point = point.offset(moved_origin.x - origin.x, moved_origin.y - origin.y);
        }
        assert_surface_eq(surface, decode(&text, moved_origin).unwrap());
    }

    #[test]
    fn thins_large_surface() {
        const POINTS: usize = 400;
        let mut noise = 1u32;
        let layers: Vec<(Vec<Vec<Point>>, [u8; 3])> = (0..40)
            .map(|layer| {
                let contours = (0..20)
                    .map(|contour| {
                        (0..POINTS)
                            .map(|i| {
                                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                let radius = if layer == 0 && contour == 0 {
                                    0.01
                                } else {
                                    11.5 + (noise >> 16) as f32 / 65_536.
                                };
                                let angle = std::f32::consts::TAU * i as f32 / POINTS as f32;
                                let (sin, cos) = angle.sin_cos();
                                Point::new(
                                    30. * contour as f32 + radius * cos,
                                    30. * layer as f32 + radius * sin,
                                )
                            })
                            .collect()
                    })
                    .collect();
                (contours, [layer as u8, 70, 130])
            })
            .collect();
        let mut closed_rim = layers[0].0[0].clone();
        closed_rim.push(closed_rim[0]);
        let rim_start = closed_rim[0];
        let surface = Primitive::Surface {
            layers,
            rims: vec![
                closed_rim,
                (0..100).map(|i| Point::new(i as f32, 5.)).collect(),
            ],
            outline: None,
        };
        let origin = Point::default();
        let text = encode(&surface, origin).unwrap();
        assert!(text.len() <= MAX_BYTES);
        let Primitive::Surface { layers, rims, .. } = decode(&text, origin).unwrap() else {
            panic!("Expected a surface");
        };
        assert_eq!(layers.len(), 40);
        assert_eq!(layers[0].0[0].len(), POINTS);
        let mut total_points = 0;
        for (contours, _) in layers {
            assert_eq!(contours.len(), 20);
            for contour in contours {
                assert!((3..=POINTS).contains(&contour.len()));
                total_points += contour.len();
            }
        }
        assert!(total_points < 40 * 20 * POINTS);
        assert_eq!(rims[0].len(), 4);
        assert_eq!(rims[0].first(), Some(&rim_start));
        assert_eq!(rims[0].last(), Some(&rim_start));
        assert_eq!(rims[1], [Point::new(0., 5.), Point::new(99., 5.)]);
    }

    #[test]
    fn rejects_malformed_snapshots() {
        let origin = Point::default();
        assert!(decode("not base64!", origin).is_err());

        // No outline or layers, with one rim containing two points.
        let mut bytes = b"RSF1\0".to_vec();
        for field in [0u32, 1, 2] {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
        let point_start = bytes.len();
        bytes.extend_from_slice(&[0; 16]);
        assert!(decode_bytes(&bytes).is_ok());

        assert_eq!(decode_bytes(&bytes[..4]).unwrap_err(), "Truncated snapshot");
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            decode_bytes(&bad_magic).unwrap_err(),
            "Invalid snapshot magic"
        );
        let mut bad_count = bytes.clone();
        bad_count[point_start - 4..point_start].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(
            decode_bytes(&bad_count).unwrap_err(),
            "Snapshot count exceeds remaining data"
        );
        bytes[point_start..point_start + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(
            decode_bytes(&bytes).unwrap_err(),
            "Non-finite snapshot coordinate"
        );
    }

    #[test]
    fn rejects_inflate_over_16_mib() {
        // The header is 17 bytes, so this valid rim inflates to 16 MiB plus one byte.
        let point_count = (16 * 1024 * 1024u32 - 17).div_ceil(8);
        let mut bytes = b"RSF1\0".to_vec();
        for field in [0, 1, point_count] {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
        bytes.resize(17 + point_count as usize * 8, 0);
        assert!(decode_bytes(&bytes).is_err());
    }

    #[test]
    fn rejects_short_paths() {
        for (contour, min_points) in [(true, 3u32), (false, 2)] {
            for point_count in 0..min_points {
                let mut bytes = b"RSF1\0".to_vec();
                bytes.extend_from_slice(&u32::from(contour).to_le_bytes());
                if contour {
                    bytes.extend_from_slice(&[0; 3]); // Layer color.
                }
                bytes.extend_from_slice(&2u32.to_le_bytes()); // Path count.
                // A valid second path leaves enough bytes for the path-count bound.
                for count in [point_count, 6] {
                    bytes.extend_from_slice(&count.to_le_bytes());
                    bytes.resize(bytes.len() + count as usize * 8, 0);
                }
                if contour {
                    bytes.extend_from_slice(&[0; 4]); // Rim count.
                }
                assert!(decode_bytes(&bytes).is_err());
            }
        }
    }

    #[test]
    fn drops_short_paths_on_encode() {
        let mut surface = small_surface(None);
        let Primitive::Surface { layers, rims, .. } = &mut surface else {
            panic!("Expected a surface");
        };
        let point = Point::default();
        layers[0].0.extend([vec![], vec![point], vec![point; 2]]);
        rims.extend([vec![], vec![point]]);
        let text = encode(&surface, point).unwrap();
        assert_surface_eq(small_surface(None), decode(&text, point).unwrap());
    }

    #[test]
    fn rejects_empty_layers_on_decode() {
        let mut bytes = b"RSF1\0".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes()); // Layer count.
        bytes.extend_from_slice(&[0; 7]); // Layer color and zero contours.
        bytes.extend_from_slice(&1u32.to_le_bytes()); // Rim count.
        bytes.extend_from_slice(&3u32.to_le_bytes()); // Enough data for the layer-count bound.
        bytes.extend_from_slice(&[0; 24]);
        assert_eq!(
            decode_bytes(&bytes).unwrap_err(),
            "Snapshot layer has no contours"
        );
    }

    #[test]
    fn drops_empty_layers_on_encode() {
        let mut surface = small_surface(None);
        let Primitive::Surface { layers, .. } = &mut surface else {
            panic!("Expected a surface");
        };
        let origin = Point::default();
        layers.insert(0, (vec![], [1, 2, 3]));
        layers.insert(2, (vec![vec![origin; 2]], [4, 5, 6]));
        let text = encode(&surface, origin).unwrap();
        assert_surface_eq(small_surface(None), decode(&text, origin).unwrap());
    }

    #[test]
    fn rejects_invalid_outlines() {
        for (width, alpha) in [
            (f32::INFINITY, 1.),
            (f32::NAN, 1.),
            (-1., 1.),
            (1., f32::NAN),
            (1., f32::INFINITY),
            (1., -0.1),
            (1., 1.1),
        ] {
            let mut bytes = b"RSF1\x01\0\0\0".to_vec();
            bytes.extend_from_slice(&alpha.to_le_bytes());
            bytes.extend_from_slice(&width.to_le_bytes());
            bytes.extend_from_slice(&[0; 8]);
            assert!(
                decode_bytes(&bytes).is_err(),
                "width {width}, alpha {alpha}"
            );
            let outline = Outline {
                color: [0; 3],
                alpha,
                width,
            };
            assert!(encode(&small_surface(Some(outline)), Point::default()).is_none());
        }
    }

    #[test]
    fn keeps_contour_when_thinning_introduces_a_crossing() {
        // The shortcut crosses another edge while changing area by less than 10%.
        let points = [
            [-4., -2.],
            [-4., -4.],
            [5., -4.],
            [-1., -3.],
            [3., 1.],
            [2., 24.],
        ]
        .map(|[x, y]| Point::new(x, y));
        assert_eq!(thin(&points, 2., true).as_ref(), points);
    }

    #[test]
    fn keeps_contour_when_thinning_distorts_signed_area() {
        let loses_area = [[0., 0.], [10., 0.], [10., 5.], [5., 8.], [0., 5.]];
        let reverses = [[3., 3.], [5., 3.], [10., 4.], [4., 0.], [1., 6.]];
        for (points, tolerance) in [(loses_area, 3.), (reverses, 8.)] {
            let points = points.map(|[x, y]| Point::new(x, y));
            assert_eq!(thin(&points, tolerance, true).as_ref(), points);
        }
    }

    #[test]
    fn rejects_trailing_compressed_input() {
        let origin = Point::default();
        let text = encode(&small_surface(None), origin).unwrap();
        let mut packed = STANDARD.decode(text).unwrap();
        packed.push(0);
        assert!(decode(&STANDARD.encode(packed), origin).is_err());
    }

    #[test]
    fn only_encodes_surfaces() {
        assert!(encode(&Primitive::Polygon(Vec::new()), Point::default()).is_none());
    }
}
