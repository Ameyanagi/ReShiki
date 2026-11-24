//! Compact snapshots of the merged 2D shapes of an orbital surface.

use std::borrow::Cow;

use base64::{Engine, engine::general_purpose::STANDARD};
use miniz_oxide::{deflate::compress_to_vec, inflate::decompress_to_vec_with_limit};

use crate::document::Point;
use crate::scene::{Outline, Primitive};

const MAGIC: &[u8; 4] = b"RSF1";
const MAX_INFLATED_BYTES: usize = 64 * 1024 * 1024;

/// Largest encoded snapshot, in bytes.
pub const MAX_BYTES: usize = 300 * 1024;

/// Encodes a surface with points relative to `origin`, the model frame's centre.
/// If needed, thins contours and rims with Douglas–Peucker, doubling the tolerance
/// from 0.1 to 12.8 drawing units. Returns `None` if the primitive is not a surface
/// or it cannot fit in the snapshot.
pub fn encode(surface: &Primitive, origin: Point) -> Option<String> {
    let Primitive::Surface {
        layers,
        rims,
        outline,
    } = surface
    else {
        return None;
    };

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
        bytes.extend_from_slice(&u32::try_from(layers.len()).ok()?.to_le_bytes());
        for (contours, color) in layers {
            bytes.extend_from_slice(color);
            write_paths(
                &mut bytes,
                contours.iter().map(|path| thin(path, tolerance, true)),
                origin,
            )?;
        }
        write_paths(
            &mut bytes,
            rims.iter().map(|path| thin(path, tolerance, false)),
            origin,
        )?;

        if bytes.len() <= MAX_INFLATED_BYTES {
            let packed = compress_to_vec(&bytes, 9);
            if packed.len().div_ceil(3) * 4 <= MAX_BYTES {
                return Some(STANDARD.encode(packed));
            }
        }
    }
    None
}

fn write_paths<'a>(
    bytes: &mut Vec<u8>,
    paths: impl ExactSizeIterator<Item = Cow<'a, [Point]>>,
    origin: Point,
) -> Option<()> {
    bytes.extend_from_slice(&u32::try_from(paths.len()).ok()?.to_le_bytes());
    for points in paths {
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

fn thin(points: &[Point], tolerance: f64, closed: bool) -> Cow<'_, [Point]> {
    let repeated_end = points.len() > 1 && points.first() == points.last();
    let closed = closed || repeated_end;
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
    Cow::Owned(simplified)
}

/// Decodes the surface and places it at `origin`.
pub fn decode(text: &str, origin: Point) -> Result<Primitive, String> {
    if text.len() > MAX_BYTES {
        return Err("Snapshot is too large".into());
    }
    let packed = STANDARD
        .decode(text)
        .map_err(|_| "Invalid snapshot base64")?;
    let data = decompress_to_vec_with_limit(&packed, MAX_INFLATED_BYTES)
        .map_err(|_| "Invalid or oversized snapshot deflate data")?;
    let mut bytes = data.as_slice();
    if &take::<4>(&mut bytes)? != MAGIC {
        return Err("Invalid snapshot magic".into());
    }
    let outline = match take(&mut bytes)? {
        [0] => None,
        [1] => Some(Outline {
            color: take(&mut bytes)?,
            alpha: f32::from_le_bytes(take(&mut bytes)?),
            width: f32::from_le_bytes(take(&mut bytes)?),
        }),
        _ => return Err("Invalid snapshot outline flag".into()),
    };
    let layer_count = read_count(&mut bytes, 7)?;
    let mut layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let color = take(&mut bytes)?;
        layers.push((read_paths(&mut bytes, origin)?, color));
    }
    let rims = read_paths(&mut bytes, origin)?;
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

fn read_paths(bytes: &mut &[u8], origin: Point) -> Result<Vec<Vec<Point>>, String> {
    let path_count = read_count(bytes, 4)?;
    let mut paths = Vec::with_capacity(path_count);
    for _ in 0..path_count {
        let point_count = read_count(bytes, 8)?;
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
        for (contours, _) in layers {
            assert_eq!(contours.len(), 20);
            for contour in contours {
                assert!((3..POINTS).contains(&contour.len()));
            }
        }
        assert_eq!(rims[0].len(), 4);
        assert_eq!(rims[0].first(), Some(&rim_start));
        assert_eq!(rims[0].last(), Some(&rim_start));
        assert_eq!(rims[1], [Point::new(0., 5.), Point::new(99., 5.)]);
    }

    #[test]
    fn rejects_malformed_snapshots() {
        let origin = Point::default();
        assert!(decode("not base64!", origin).is_err());

        // No outline or layers, with one rim containing a single point.
        let mut bytes = b"RSF1\0".to_vec();
        for field in [0u32, 1, 1] {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
        let point_start = bytes.len();
        bytes.extend_from_slice(&1f32.to_le_bytes());
        bytes.extend_from_slice(&2f32.to_le_bytes());
        let decode_bytes =
            |bytes: &[u8]| decode(&STANDARD.encode(compress_to_vec(bytes, 9)), origin);
        assert!(decode_bytes(&bytes).is_ok());

        assert_eq!(decode_bytes(&bytes[..4]).unwrap_err(), "Truncated snapshot");
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            decode_bytes(&bad_magic).unwrap_err(),
            "Invalid snapshot magic"
        );
        let mut bad_count = bytes.clone();
        bad_count[point_start - 4..point_start].copy_from_slice(&2u32.to_le_bytes());
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
    fn only_encodes_surfaces() {
        assert!(encode(&Primitive::Polygon(Vec::new()), Point::default()).is_none());
    }
}
