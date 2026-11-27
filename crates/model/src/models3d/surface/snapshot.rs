//! Compact snapshots of the merged 2D shapes of an orbital surface.

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
use crate::graphics::PathCommand;
use crate::scene::{Outline, Primitive, SurfaceFill, SurfaceGroup, SurfaceStroke};

const MAGIC: &[u8; 4] = b"RSF2";
const MAX_INFLATED_BYTES: usize = 16 * 1024 * 1024;
const MAX_PATH_COMMANDS: usize = 1_000_000;
const MIN_PATH_BYTES: usize = 4 + 1 + 8;

/// Largest encoded snapshot, in bytes.
pub const MAX_BYTES: usize = 300 * 1024;

/// Encodes a surface with points relative to `origin`, the model frame's centre.
/// If the snapshot exceeds a size or path command limit, drops every other
/// tone in each fill and retries until it fits or no tones remain. Regions
/// and strokes are always retained exactly. Returns `None` for invalid data,
/// other primitives, or a surface that cannot fit even without tones.
pub fn encode(surface: &Primitive, origin: Point) -> Option<String> {
    let Primitive::Surface { groups } = surface else {
        return None;
    };
    if !origin.is_finite() {
        return None;
    }
    let mut tone_stride = 1;
    loop {
        let (bytes, command_count) = write_snapshot(groups, origin, tone_stride)?;
        if command_count <= MAX_PATH_COMMANDS && bytes.len() <= MAX_INFLATED_BYTES {
            let packed = compress_to_vec(&bytes, 9);
            if packed.len().div_ceil(3) * 4 <= MAX_BYTES {
                return Some(STANDARD.encode(packed));
            }
        }
        if !groups
            .iter()
            .flat_map(|group| &group.fills)
            .any(|fill| fill.tones.len() >= tone_stride)
        {
            return None;
        }
        tone_stride = tone_stride.checked_mul(2)?;
    }
}

fn write_snapshot(
    groups: &[SurfaceGroup],
    origin: Point,
    tone_stride: usize,
) -> Option<(Vec<u8>, usize)> {
    let mut bytes = MAGIC.to_vec();
    let mut command_count = 0;
    write_count(&mut bytes, groups.len())?;
    for group in groups {
        if !valid_alpha(group.alpha) {
            return None;
        }
        bytes.extend_from_slice(&group.alpha.to_le_bytes());
        write_count(&mut bytes, group.fills.len())?;
        for fill in &group.fills {
            bytes.extend_from_slice(&fill.color);
            command_count += write_path(&mut bytes, &fill.region, origin)?;
            write_count(&mut bytes, fill.tones.len() / tone_stride)?;
            // Retain the lighter member of each pair. A final single tone is
            // dropped too, so retries eventually reach the base regions.
            for (path, color) in fill.tones.iter().skip(tone_stride - 1).step_by(tone_stride) {
                bytes.extend_from_slice(color);
                command_count += write_path(&mut bytes, path, origin)?;
            }
        }
        write_count(&mut bytes, group.strokes.len())?;
        for stroke in &group.strokes {
            if !valid_outline(&stroke.outline) {
                return None;
            }
            bytes.extend_from_slice(&stroke.outline.color);
            bytes.extend_from_slice(&stroke.outline.alpha.to_le_bytes());
            bytes.extend_from_slice(&stroke.outline.width.to_le_bytes());
            command_count += write_path(&mut bytes, &stroke.paths, origin)?;
        }
    }
    Some((bytes, command_count))
}

fn valid_alpha(alpha: f32) -> bool {
    alpha > 0. && alpha <= 1.
}

fn valid_outline(outline: &Outline) -> bool {
    valid_alpha(outline.alpha) && outline.width.is_finite() && outline.width > 0.
}

fn write_count(bytes: &mut Vec<u8>, count: usize) -> Option<()> {
    bytes.extend_from_slice(&u32::try_from(count).ok()?.to_le_bytes());
    Some(())
}

fn write_path(bytes: &mut Vec<u8>, path: &[PathCommand], origin: Point) -> Option<usize> {
    if !matches!(path.first(), Some(PathCommand::Move(_))) {
        return None;
    }
    write_count(bytes, path.len())?;
    for command in path {
        bytes.push(match command {
            PathCommand::Move(_) => 0,
            PathCommand::Line(_) => 1,
            PathCommand::Cubic(..) => 2,
            PathCommand::Close => 3,
        });
        for point in command.iter_points() {
            let relative = point.offset(-origin.x, -origin.y);
            if !relative.is_finite() {
                return None;
            }
            bytes.extend_from_slice(&relative.x.to_le_bytes());
            bytes.extend_from_slice(&relative.y.to_le_bytes());
        }
    }
    Some(path.len())
}

fn inflate(mut packed: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = Box::<DecompressorOxide>::default();
    let mut data = vec![
        0;
        packed
            .len()
            .clamp(1, MAX_INFLATED_BYTES)
            .next_power_of_two()
    ];
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
                data.resize((data.len() * 2).min(MAX_INFLATED_BYTES), 0);
            }
            _ => return Err("Invalid or oversized snapshot deflate data".into()),
        }
    }
}

/// Decodes an RSF2 surface and places it at `origin`.
pub fn decode(text: &str, origin: Point) -> Result<Primitive, String> {
    decode_scaled(text, origin, 1.)
}

pub(in crate::models3d) fn decode_scaled(
    text: &str,
    origin: Point,
    scale: f32,
) -> Result<Primitive, String> {
    if text.len() > MAX_BYTES {
        return Err("Snapshot is too large".into());
    }
    if !origin.is_finite() {
        return Err("Non-finite snapshot origin".into());
    }
    let packed = STANDARD
        .decode(text)
        .map_err(|_| "Invalid snapshot base64")?;
    let data = inflate(&packed)?;
    let mut bytes = data.as_slice();
    if &take::<4>(&mut bytes)? != MAGIC {
        return Err("Invalid snapshot magic".into());
    }
    // A group always contains its alpha and both collection counts.
    let group_count = read_count(&mut bytes, 4 + 4 + 4)?;
    let mut groups = Vec::with_capacity(group_count);
    let mut total_path_commands = 0;
    for _ in 0..group_count {
        let alpha = f32::from_le_bytes(take(&mut bytes)?);
        if !valid_alpha(alpha) {
            return Err("Invalid snapshot group alpha".into());
        }
        let fill_count = read_count(&mut bytes, 3 + MIN_PATH_BYTES + 4)?;
        let mut fills = Vec::with_capacity(fill_count);
        for _ in 0..fill_count {
            let color = take(&mut bytes)?;
            let region = read_path(&mut bytes, origin, scale, &mut total_path_commands)?;
            let tone_count = read_count(&mut bytes, 3 + MIN_PATH_BYTES)?;
            let mut tones = Vec::with_capacity(tone_count);
            for _ in 0..tone_count {
                let color = take(&mut bytes)?;
                let path = read_path(&mut bytes, origin, scale, &mut total_path_commands)?;
                tones.push((path, color));
            }
            fills.push(SurfaceFill {
                region,
                color,
                tones,
            });
        }
        let stroke_count = read_count(&mut bytes, 3 + 4 + 4 + MIN_PATH_BYTES)?;
        let mut strokes = Vec::with_capacity(stroke_count);
        for _ in 0..stroke_count {
            let outline = Outline {
                color: take(&mut bytes)?,
                alpha: f32::from_le_bytes(take(&mut bytes)?),
                width: f32::from_le_bytes(take(&mut bytes)?) * scale,
            };
            if !valid_outline(&outline) {
                return Err("Invalid snapshot outline".into());
            }
            let paths = read_path(&mut bytes, origin, scale, &mut total_path_commands)?;
            strokes.push(SurfaceStroke { paths, outline });
        }
        groups.push(SurfaceGroup {
            alpha,
            fills,
            strokes,
        });
    }
    if !bytes.is_empty() {
        return Err("Trailing snapshot data".into());
    }
    Ok(Primitive::Surface { groups })
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

fn read_path(
    bytes: &mut &[u8],
    origin: Point,
    scale: f32,
    total_path_commands: &mut usize,
) -> Result<Vec<PathCommand>, String> {
    // Close is the smallest command, occupying just its tag byte.
    let command_count = read_count(bytes, 1)?;
    if command_count == 0 {
        return Err("Snapshot path is empty".into());
    }
    // Validate each command before growing the decoded path.
    let mut path = Vec::new();
    let mut subpath_open = false;
    for index in 0..command_count {
        if *total_path_commands == MAX_PATH_COMMANDS {
            return Err("Snapshot has too many path commands".into());
        }
        let [tag] = take(bytes)?;
        if index == 0 && tag != 0 {
            return Err("Snapshot path does not start with Move".into());
        }
        let command = match tag {
            0 => {
                subpath_open = true;
                PathCommand::Move(read_point(bytes, origin, scale)?)
            }
            1 => PathCommand::Line(read_point(bytes, origin, scale)?),
            2 => PathCommand::Cubic(
                read_point(bytes, origin, scale)?,
                read_point(bytes, origin, scale)?,
                read_point(bytes, origin, scale)?,
            ),
            3 => {
                if !subpath_open
                    || !matches!(
                        path.last(),
                        Some(PathCommand::Line(_) | PathCommand::Cubic(..))
                    )
                {
                    return Err("Snapshot Close does not follow a segment in its subpath".into());
                }
                subpath_open = false;
                PathCommand::Close
            }
            _ => return Err("Invalid snapshot path command".into()),
        };
        *total_path_commands += 1;
        path.push(command);
    }
    Ok(path)
}

fn read_point(bytes: &mut &[u8], origin: Point, scale: f32) -> Result<Point, String> {
    let x = f32::from_le_bytes(take(bytes)?);
    let y = f32::from_le_bytes(take(bytes)?);
    let point = origin.offset(x * scale, y * scale);
    if !point.is_finite() {
        return Err("Non-finite snapshot coordinate".into());
    }
    Ok(point)
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

    fn groups(surface: &Primitive) -> &[SurfaceGroup] {
        let Primitive::Surface { groups } = surface else {
            panic!("Expected a surface");
        };
        groups
    }

    fn groups_mut(surface: &mut Primitive) -> &mut Vec<SurfaceGroup> {
        let Primitive::Surface { groups } = surface else {
            panic!("Expected a surface");
        };
        groups
    }

    fn small_surface() -> Primitive {
        Primitive::Surface {
            groups: vec![
                SurfaceGroup {
                    alpha: 0.6,
                    fills: vec![SurfaceFill {
                        region: vec![
                            PathCommand::Move(Point::new(8., -5.)),
                            PathCommand::Line(Point::new(11., -5.)),
                            PathCommand::Cubic(
                                Point::new(12., -4.5),
                                Point::new(12., -3.5),
                                Point::new(11., -3.),
                            ),
                            PathCommand::Line(Point::new(8., -3.)),
                            PathCommand::Close,
                            PathCommand::Move(Point::new(9., -4.5)),
                            PathCommand::Line(Point::new(10., -4.)),
                            PathCommand::Line(Point::new(9., -3.5)),
                            PathCommand::Close,
                        ],
                        color: [120, 180, 220],
                        tones: vec![
                            (
                                vec![
                                    PathCommand::Move(Point::new(9., -4.75)),
                                    PathCommand::Cubic(
                                        Point::new(10., -4.75),
                                        Point::new(10.5, -4.25),
                                        Point::new(9.25, -4.),
                                    ),
                                    PathCommand::Close,
                                ],
                                [25, 45, 60],
                            ),
                            (vec![PathCommand::Move(Point::new(10., -4.))], [255; 3]),
                        ],
                    }],
                    strokes: vec![SurfaceStroke {
                        paths: vec![
                            PathCommand::Move(Point::new(8., -3.)),
                            PathCommand::Line(Point::new(11., -3.)),
                            PathCommand::Move(Point::new(8., -5.)),
                            PathCommand::Cubic(
                                Point::new(9., -5.5),
                                Point::new(10., -5.5),
                                Point::new(11., -5.),
                            ),
                        ],
                        outline: Outline {
                            color: [26, 33, 41],
                            alpha: 0.45,
                            width: 0.8,
                        },
                    }],
                },
                SurfaceGroup {
                    alpha: 1.,
                    fills: vec![],
                    strokes: vec![],
                },
            ],
        }
    }

    // A complete independent wire fixture, including every required color.
    fn wire_fixture() -> Vec<u8> {
        let mut bytes = b"RSF2".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3]);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&2f32.to_le_bytes());
        bytes.extend_from_slice(&3f32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[4, 5, 6]);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&4f32.to_le_bytes());
        bytes.extend_from_slice(&5f32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[7, 8, 9]);
        bytes.extend_from_slice(&0.75f32.to_le_bytes());
        bytes.extend_from_slice(&1.25f32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&6f32.to_le_bytes());
        bytes.extend_from_slice(&7f32.to_le_bytes());
        bytes
    }

    #[test]
    fn round_trip_all_commands_and_groups() {
        let origin = Point::new(10., -4.);
        let surface = small_surface();
        let text = encode(&surface, origin).unwrap();
        assert_eq!(groups(&surface), groups(&decode(&text, origin).unwrap()));
        let empty = Primitive::Surface { groups: vec![] };
        let text = encode(&empty, origin).unwrap();
        assert!(groups(&decode(&text, origin).unwrap()).is_empty());
    }

    #[test]
    fn exact_rsf2_layout() {
        let surface = Primitive::Surface {
            groups: vec![SurfaceGroup {
                alpha: 0.5,
                fills: vec![SurfaceFill {
                    region: vec![PathCommand::Move(Point::new(12., -1.))],
                    color: [1, 2, 3],
                    tones: vec![(vec![PathCommand::Move(Point::new(14., 1.))], [4, 5, 6])],
                }],
                strokes: vec![SurfaceStroke {
                    paths: vec![PathCommand::Move(Point::new(16., 3.))],
                    outline: Outline {
                        color: [7, 8, 9],
                        alpha: 0.75,
                        width: 1.25,
                    },
                }],
            }],
        };
        let text = encode(&surface, Point::new(10., -4.)).unwrap();
        assert_eq!(
            inflate(&STANDARD.decode(text).unwrap()).unwrap(),
            wire_fixture()
        );
    }

    #[test]
    fn origin_shift() {
        let origin = Point::new(10., -4.);
        let moved_origin = Point::new(-6., 3.5);
        let mut surface = small_surface();
        let text = encode(&surface, origin).unwrap();
        for group in groups_mut(&mut surface) {
            for path in group
                .fills
                .iter_mut()
                .flat_map(|fill| {
                    std::iter::once(&mut fill.region)
                        .chain(fill.tones.iter_mut().map(|(path, _)| path))
                })
                .chain(group.strokes.iter_mut().map(|stroke| &mut stroke.paths))
            {
                for command in path {
                    *command = command.map(|point| {
                        point.offset(moved_origin.x - origin.x, moved_origin.y - origin.y)
                    });
                }
            }
        }
        assert_eq!(
            groups(&surface),
            groups(&decode(&text, moved_origin).unwrap())
        );
    }

    fn noisy_path(noise: &mut u32, points: usize) -> Vec<PathCommand> {
        let mut coordinate = || {
            *noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f32::from_bits(0x3f80_0000 | (*noise >> 9))
        };
        (0..points)
            .map(|index| {
                let point = Point::new(coordinate(), coordinate());
                if index == 0 {
                    PathCommand::Move(point)
                } else {
                    PathCommand::Line(point)
                }
            })
            .collect()
    }

    #[test]
    fn drops_alternate_tones_without_changing_regions_or_strokes() {
        let mut surface = small_surface();
        let group = &mut groups_mut(&mut surface)[0];
        group.fills.push(group.fills[0].clone());
        let mut noise = 1;
        for (fill_index, fill) in group.fills.iter_mut().enumerate() {
            fill.tones = (0..9)
                .map(|index| (noisy_path(&mut noise, 6000), [fill_index as u8, index, 60]))
                .collect();
        }
        let (original, _) = write_snapshot(groups(&surface), Point::default(), 1).unwrap();
        assert!(STANDARD.encode(compress_to_vec(&original, 9)).len() > MAX_BYTES);
        let text = encode(&surface, Point::default()).unwrap();
        assert!(text.len() <= MAX_BYTES);
        let decoded = decode(&text, Point::default()).unwrap();
        let before = groups(&surface);
        let after = groups(&decoded);
        assert_eq!(before.len(), after.len());
        let retained = after[0].fills[0].tones.len();
        assert!(retained < before[0].fills[0].tones.len());
        let stride = [2, 4, 8, 16]
            .into_iter()
            .find(|stride| before[0].fills[0].tones.len() / stride == retained)
            .unwrap();
        for (before, after) in before.iter().zip(after) {
            assert_eq!(before.alpha, after.alpha);
            assert_eq!(before.strokes, after.strokes);
            assert_eq!(before.fills.len(), after.fills.len());
            for (before, after) in before.fills.iter().zip(&after.fills) {
                assert_eq!(before.region, after.region);
                assert_eq!(before.color, after.color);
                let expected: Vec<_> = before
                    .tones
                    .iter()
                    .skip(stride - 1)
                    .step_by(stride)
                    .cloned()
                    .collect();
                assert_eq!(after.tones, expected);
            }
        }
    }

    #[test]
    fn can_drop_the_last_tone_but_never_required_paths() {
        let mut noise = 1;
        let path = noisy_path(&mut noise, 90_000);
        let mut surface = small_surface();
        groups_mut(&mut surface)[0].fills[0].tones = vec![(path.clone(), [1, 2, 3])];
        let text = encode(&surface, Point::default()).unwrap();
        let decoded = decode(&text, Point::default()).unwrap();
        groups_mut(&mut surface)[0].fills[0].tones.clear();
        assert_eq!(groups(&surface), groups(&decoded));

        let mut oversized_region = surface.clone();
        groups_mut(&mut oversized_region)[0].fills[0].region = path.clone();
        assert!(encode(&oversized_region, Point::default()).is_none());
        let mut oversized_stroke = surface;
        groups_mut(&mut oversized_stroke)[0].strokes[0].paths = path;
        assert!(encode(&oversized_stroke, Point::default()).is_none());
    }

    #[test]
    fn encode_rejects_over_budget_and_round_trips_at_the_limit() {
        let origin = Point::default();
        let mut surface = small_surface();
        let group = &mut groups_mut(&mut surface)[0];
        let fill = &mut group.fills[0];
        fill.tones.clear();
        fill.region.resize(
            MAX_PATH_COMMANDS + 1 - group.strokes[0].paths.len(),
            PathCommand::Line(origin),
        );
        assert!(encode(&surface, origin).is_none());

        groups_mut(&mut surface)[0].fills[0].region.pop();
        let text = encode(&surface, origin).unwrap();
        let decoded = decode(&text, origin).unwrap();
        assert!(groups(&decoded) == groups(&surface));
    }

    #[test]
    fn encode_drops_alternate_tones_to_fit_the_command_budget() {
        let origin = Point::default();
        let mut surface = small_surface();
        let group = &mut groups_mut(&mut surface)[0];
        let fill = &mut group.fills[0];
        fill.region.resize(
            MAX_PATH_COMMANDS - group.strokes[0].paths.len() - fill.tones[1].0.len(),
            PathCommand::Line(origin),
        );

        let text = encode(&surface, origin).unwrap();
        let decoded = decode(&text, origin).unwrap();
        groups_mut(&mut surface)[0].fills[0].tones.remove(0);
        assert!(groups(&decoded) == groups(&surface));
    }

    #[test]
    fn rejects_bad_magic_truncation_and_counts() {
        let fixture = wire_fixture();
        assert!(decode_bytes(&fixture).is_ok());
        for length in 0..fixture.len() {
            assert!(decode_bytes(&fixture[..length]).is_err(), "length {length}");
        }
        for magic in [b"RSF1", b"XSF2"] {
            let mut bytes = fixture.clone();
            bytes[..4].copy_from_slice(magic);
            assert_eq!(decode_bytes(&bytes).unwrap_err(), "Invalid snapshot magic");
        }
        for offset in [4, 12, 19, 32, 39, 52, 67] {
            let mut bytes = fixture.clone();
            bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert_eq!(
                decode_bytes(&bytes).unwrap_err(),
                "Snapshot count exceeds remaining data",
                "count at {offset}"
            );
        }
    }

    #[test]
    fn rejects_empty_paths_and_paths_without_move() {
        for (count_offset, tag_offset) in [(19, 23), (39, 43), (67, 71)] {
            let mut bytes = wire_fixture();
            bytes[count_offset..count_offset + 4].copy_from_slice(&0u32.to_le_bytes());
            assert_eq!(decode_bytes(&bytes).unwrap_err(), "Snapshot path is empty");
            for tag in [1, 2, 3, 255] {
                let mut bytes = wire_fixture();
                bytes[tag_offset] = tag;
                assert_eq!(
                    decode_bytes(&bytes).unwrap_err(),
                    "Snapshot path does not start with Move"
                );
            }
        }
        let mut surface = small_surface();
        for path in [
            vec![],
            vec![PathCommand::Close],
            vec![PathCommand::Line(Point::default())],
        ] {
            groups_mut(&mut surface)[0].fills[0].region = path;
            assert!(encode(&surface, Point::default()).is_none());
        }
        let mut bytes = wire_fixture();
        bytes[19..23].copy_from_slice(&2u32.to_le_bytes());
        bytes.insert(32, 255);
        assert_eq!(
            decode_bytes(&bytes).unwrap_err(),
            "Invalid snapshot path command"
        );
    }

    #[test]
    fn rejects_more_than_one_million_total_path_commands() {
        fn path(bytes: &mut Vec<u8>, commands: usize) {
            write_count(bytes, commands).unwrap();
            for index in 0..commands {
                bytes.push(u8::from(index != 0));
                bytes.extend_from_slice(&[0; 8]);
            }
        }

        fn fixture(last_path_commands: usize) -> Vec<u8> {
            let mut bytes = MAGIC.to_vec();
            write_count(&mut bytes, 2).unwrap();
            bytes.extend_from_slice(&1f32.to_le_bytes());
            write_count(&mut bytes, 1).unwrap();
            bytes.extend_from_slice(&[1, 2, 3]);
            path(&mut bytes, 250_000);
            write_count(&mut bytes, 1).unwrap();
            bytes.extend_from_slice(&[4, 5, 6]);
            path(&mut bytes, 250_000);
            write_count(&mut bytes, 1).unwrap();
            bytes.extend_from_slice(&[7, 8, 9]);
            bytes.extend_from_slice(&1f32.to_le_bytes());
            bytes.extend_from_slice(&1f32.to_le_bytes());
            path(&mut bytes, 250_000);
            bytes.extend_from_slice(&1f32.to_le_bytes());
            write_count(&mut bytes, 1).unwrap();
            bytes.extend_from_slice(&[10, 11, 12]);
            path(&mut bytes, last_path_commands);
            write_count(&mut bytes, 0).unwrap();
            write_count(&mut bytes, 0).unwrap();
            bytes
        }

        // The budget spans regions, tones, strokes, and groups.
        assert!(decode_bytes(&fixture(250_000)).is_ok());
        assert_eq!(
            decode_bytes(&fixture(250_001)).err().as_deref(),
            Some("Snapshot has too many path commands")
        );
    }

    #[test]
    fn rejects_close_without_preceding_segment_in_subpath() {
        let point = Point::default();
        for path in [
            vec![PathCommand::Move(point), PathCommand::Close],
            vec![
                PathCommand::Move(point),
                PathCommand::Line(point),
                PathCommand::Close,
                PathCommand::Close,
            ],
            vec![
                PathCommand::Move(point),
                PathCommand::Line(point),
                PathCommand::Move(point),
                PathCommand::Close,
            ],
            vec![
                PathCommand::Move(point),
                PathCommand::Line(point),
                PathCommand::Close,
                PathCommand::Line(point),
                PathCommand::Close,
            ],
        ] {
            let mut surface = small_surface();
            groups_mut(&mut surface)[0].fills[0].region = path;
            let (bytes, _) = write_snapshot(groups(&surface), point, 1).unwrap();
            assert_eq!(
                decode_bytes(&bytes).unwrap_err(),
                "Snapshot Close does not follow a segment in its subpath"
            );
        }
    }

    #[test]
    fn rejects_invalid_alpha_and_width() {
        for alpha in [0., -0.1, 1.1, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for offset in [8, 59] {
                let mut bytes = wire_fixture();
                bytes[offset..offset + 4].copy_from_slice(&alpha.to_le_bytes());
                assert!(decode_bytes(&bytes).is_err(), "alpha {alpha} at {offset}");
            }
            let mut surface = small_surface();
            groups_mut(&mut surface)[0].alpha = alpha;
            assert!(encode(&surface, Point::default()).is_none());
            groups_mut(&mut surface)[0].alpha = 1.;
            groups_mut(&mut surface)[0].strokes[0].outline.alpha = alpha;
            assert!(encode(&surface, Point::default()).is_none());
        }
        for width in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bytes = wire_fixture();
            bytes[63..67].copy_from_slice(&width.to_le_bytes());
            assert!(decode_bytes(&bytes).is_err(), "width {width}");
            let mut surface = small_surface();
            groups_mut(&mut surface)[0].strokes[0].outline.width = width;
            assert!(encode(&surface, Point::default()).is_none());
        }
    }

    #[test]
    fn rejects_non_finite_coordinates_and_origins() {
        for coordinate in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for offset in [24, 28, 44, 48, 72, 76] {
                let mut bytes = wire_fixture();
                bytes[offset..offset + 4].copy_from_slice(&coordinate.to_le_bytes());
                assert_eq!(
                    decode_bytes(&bytes).unwrap_err(),
                    "Non-finite snapshot coordinate"
                );
            }
            let origin = Point::new(coordinate, 0.);
            let surface = small_surface();
            assert!(encode(&surface, origin).is_none());
            assert!(decode(&encode(&surface, Point::default()).unwrap(), origin).is_err());
        }
        let mut bytes = wire_fixture();
        bytes[24..28].copy_from_slice(&f32::MAX.to_le_bytes());
        let text = STANDARD.encode(compress_to_vec(&bytes, 9));
        assert!(decode(&text, Point::new(f32::MAX, 0.)).is_err());
        let mut surface = small_surface();
        groups_mut(&mut surface)[0].fills[0].region[0] =
            PathCommand::Move(Point::new(f32::MAX, 0.));
        assert!(encode(&surface, Point::new(-f32::MAX, 0.)).is_none());
    }

    #[test]
    fn rejects_trailing_encoded_compressed_and_inflated_data() {
        let text = encode(&small_surface(), Point::default()).unwrap();
        for suffix in [" ", "=", "AAAA", "\n"] {
            assert!(decode(&(text.clone() + suffix), Point::default()).is_err());
        }
        let mut packed = STANDARD.decode(&text).unwrap();
        packed.push(0);
        assert_eq!(
            decode(&STANDARD.encode(packed), Point::default()).unwrap_err(),
            "Trailing compressed snapshot data"
        );
        let mut bytes = wire_fixture();
        bytes.push(0);
        assert_eq!(decode_bytes(&bytes).unwrap_err(), "Trailing snapshot data");
    }

    #[test]
    fn rejects_invalid_or_oversized_base64_and_deflate() {
        for text in ["", "not base64!", "AA=="] {
            assert!(decode(text, Point::default()).is_err());
        }
        assert_eq!(
            decode(&"A".repeat(MAX_BYTES + 1), Point::default()).unwrap_err(),
            "Snapshot is too large"
        );
        let bytes = vec![0; MAX_INFLATED_BYTES + 1];
        assert_eq!(
            decode_bytes(&bytes).unwrap_err(),
            "Invalid or oversized snapshot deflate data"
        );
        let packed = compress_to_vec(&bytes[..MAX_INFLATED_BYTES], 1);
        assert_eq!(inflate(&packed).unwrap().len(), MAX_INFLATED_BYTES);
    }

    #[test]
    fn only_encodes_surfaces() {
        assert!(encode(&Primitive::Polygon(Vec::new()), Point::default()).is_none());
    }
}
