//! Inspect the actual PDF font programs against independent static fixtures.
use flate2::read::ZlibDecoder;
use resvg::tiny_skia;
use std::{collections::HashMap, io::Read};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

// This fixture is emitted by pdf-writer's classic xref writer. Follow its
// offsets and stream lengths so compressed bytes cannot masquerade as syntax.
fn objects(pdf: &[u8]) -> HashMap<u32, &[u8]> {
    let marker = b"startxref\n";
    let start = pdf
        .windows(marker.len())
        .rposition(|b| b == marker)
        .unwrap();
    let offset: usize = std::str::from_utf8(&pdf[start + marker.len()..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[offset..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let mut header = lines.next().unwrap().split_whitespace();
    assert_eq!(header.next(), Some("0"));
    let count: u32 = header.next().unwrap().parse().unwrap();
    let mut offsets = Vec::new();
    for id in 0..count {
        let mut entry = lines.next().unwrap().split_whitespace();
        let position: usize = entry.next().unwrap().parse().unwrap();
        assert_eq!(entry.next(), Some(if id == 0 { "65535" } else { "00000" }));
        if entry.next() == Some("n") {
            offsets.push((id, position));
        }
    }
    offsets.sort_unstable_by_key(|&(_, position)| position);
    offsets
        .iter()
        .enumerate()
        .map(|(i, &(id, position))| {
            let end = offsets.get(i + 1).map_or(offset, |&(_, next)| next);
            (id, &pdf[position..end])
        })
        .collect()
}

fn dictionary(object: &[u8]) -> &str {
    let end = object
        .windows(7)
        .position(|b| b == b"stream\n")
        .unwrap_or(object.len());
    std::str::from_utf8(&object[..end]).unwrap()
}

fn integer(dictionary: &str, property: &str) -> u32 {
    dictionary
        .split_once(property)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn font_program(object: &[u8]) -> Vec<u8> {
    let dict = dictionary(object);
    assert!(dict.contains("/Filter /FlateDecode"));
    let length = integer(dict, "/Length") as usize;
    let start = object.windows(7).position(|b| b == b"stream\n").unwrap() + 7;
    let mut data = Vec::new();
    ZlibDecoder::new(&object[start..start + length])
        .read_to_end(&mut data)
        .unwrap();
    data
}

fn widths(dict: &str) -> Vec<f32> {
    // PDF /W permits both CID ranges and arrays of consecutive CID widths.
    let after = dict.split_once("/W ").unwrap().1;
    let start = after.find('[').unwrap();
    let mut depth = 0;
    let end = after
        .char_indices()
        .skip(start)
        .find_map(|(i, c)| {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(i)
        })
        .unwrap();
    let values = after[start + 1..end]
        .replace('[', " [ ")
        .replace(']', " ] ");
    let mut tokens = values.split_whitespace();
    let mut widths = HashMap::new();
    while let Some(first) = tokens.next() {
        let mut first: u16 = first.parse().unwrap();
        let second = tokens.next().unwrap();
        if second == "[" {
            for width in tokens.by_ref().take_while(|&value| value != "]") {
                widths.insert(first, width.parse::<f32>().unwrap());
                first += 1;
            }
        } else {
            let last: u16 = second.parse().unwrap();
            let width: f32 = tokens.next().unwrap().parse().unwrap();
            for cid in first..=last {
                widths.insert(cid, width);
            }
        }
    }
    (0..4).map(|cid| widths[&cid]).collect()
}

struct Path(tiny_skia::PathBuilder);
impl OutlineBuilder for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

fn glyph_pixels(face: &Face<'_>, id: GlyphId) -> Vec<u8> {
    let mut path = Path(tiny_skia::PathBuilder::new());
    face.outline_glyph(id, &mut path).unwrap();
    path_pixels(path.0, face.units_per_em())
}

fn path_pixels(path: tiny_skia::PathBuilder, units_per_em: u16) -> Vec<u8> {
    let mut image = tiny_skia::Pixmap::new(220, 200).unwrap();
    let scale = 140. / f32::from(units_per_em);
    image.fill_path(
        &path.finish().unwrap(),
        &tiny_skia::Paint::default(),
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::from_row(scale, 0., 0., -scale, 20., 160.),
        None,
    );
    image.take()
}

fn cff2_control_pixels(weight: u16, character: char) -> Vec<u8> {
    // FontTools' independent unrounded cubic oracle avoids cumulative integer
    // rounding in the static CFF1 controls, especially along the N diagonal.
    let mut path = tiny_skia::PathBuilder::new();
    let mut begun = false;
    for line in include_str!("../fixtures/font-export-103/cff2-outline-controls.tsv").lines() {
        let mut fields = line.split_whitespace();
        let w: u16 = fields.next().unwrap().parse().unwrap();
        let c = fields.next().unwrap().chars().next().unwrap();
        if w != weight || c != character {
            continue;
        }
        let command = fields.next().unwrap();
        let points: Vec<f32> = fields.map(|s| s.parse().unwrap()).collect();
        match (command, points.as_slice()) {
            ("0", &[x, y]) => {
                if begun {
                    path.close();
                }
                path.move_to(x, y);
                begun = true;
            }
            ("1", &[x, y]) => path.line_to(x, y),
            ("3", &[x1, y1, x2, y2, x, y]) => path.cubic_to(x1, y1, x2, y2, x, y),
            _ => panic!("Invalid independent outline command"),
        }
    }
    assert!(begun);
    path.close();
    path_pixels(path, 1000)
}

pub(super) fn assert_embedded_weights(pdf: &[u8], controls: [&[u8]; 2]) {
    let objects = objects(pdf);
    let controls = controls.map(|bytes| Face::parse(bytes, 0).unwrap());
    let mut found = Vec::new();
    let mut programs = Vec::new();
    for descendant in objects
        .values()
        .filter(|object| dictionary(object).contains("/Subtype /CIDFontType2"))
    {
        let dict = dictionary(descendant);
        assert!(dict.contains("/CIDToGIDMap /Identity"));
        let actual_widths = widths(dict);
        let matches: Vec<_> = controls
            .iter()
            .enumerate()
            .filter(|(_, face)| {
                "HNO".chars().enumerate().all(|(i, c)| {
                    let expected = f32::from(
                        face.glyph_hor_advance(face.glyph_index(c).unwrap())
                            .unwrap(),
                    ) * 1000.
                        / f32::from(face.units_per_em());
                    (actual_widths[i + 1] - expected).abs() < 0.001
                })
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "PDF widths must identify one requested weight: {actual_widths:?}"
        );
        let &(weight, control) = matches.first().unwrap();
        found.push(weight);
        let descriptor = dictionary(objects[&integer(dict, "/FontDescriptor")]);
        let program = font_program(objects[&integer(descriptor, "/FontFile2")]);
        let embedded = Face::parse(&program, 0).unwrap();
        assert!(
            embedded.variation_axes().is_empty(),
            "PDF must embed a resolved instance"
        );
        for (i, c) in "HNO".chars().enumerate() {
            let id = GlyphId((i + 1) as u16);
            let advance = f32::from(embedded.glyph_hor_advance(id).unwrap()) * 1000.
                / f32::from(embedded.units_per_em());
            assert!((advance - actual_widths[i + 1]).abs() < 0.001);
            let actual = glyph_pixels(&embedded, id);
            let expected = if control
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"CFF "))
                .is_some()
            {
                cff2_control_pixels([400, 700][weight], c)
            } else {
                glyph_pixels(control, control.glyph_index(c).unwrap())
            };
            let coverage: u64 = expected
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| u64::from(p[3]))
                .sum();
            let difference: u64 = actual
                .iter()
                .zip(&expected)
                .map(|(&a, &b)| u64::from(a.abs_diff(b)))
                .sum();
            assert!(coverage > 1000);
            // CFF2 is converted to quadratics and TTF coordinates are rounded.
            // Thin outlines are far outside this antialiasing tolerance.
            let ratio = difference as f64 / coverage as f64;
            assert!(
                ratio < 0.02,
                "embedded weight {weight}, glyph {c} differs from its static control: {ratio:.6}; embedded {:?}, control {:?}",
                embedded.glyph_bounding_box(id),
                control.glyph_bounding_box(control.glyph_index(c).unwrap())
            );
        }
        programs.push(program);
    }
    found.sort_unstable();
    assert_eq!(found, [0, 1]);
    assert_ne!(
        programs[0], programs[1],
        "The PDF must contain distinct font instances"
    );
}
