//! Captured-baseline parity for scene::primitives: every layer stage over the
//! fixture corpus, a bond display x order matrix and a decorations drawing.
//! Run with RESHIKI_MODEL_PARITY set; see crate::parity_tests.
use super::{atom_label_bounds, atom_label_ink_boxes, bounds, primitives, svg_with_background};
use crate::{
    bonds::DoublePosition,
    document::{Annotation, Document, Point},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
    highlights,
    palette::{Color, Hue, Row},
    parity_tests::{check_baseline, measured},
    pictures::Picture,
    ring_fills,
};
use std::fmt::Write as _;

macro_rules! fixture {
    ($path:literal) => {
        (
            $path,
            include_bytes!(concat!("../../../../", $path)).as_slice(),
        )
    };
}

const FIXTURES: [(&str, &[u8]); 17] = [
    fixture!("assets/examples/shortcut-examples.rsk"),
    fixture!("tests/fixtures/adjustable-arcs.rsk"),
    fixture!("tests/fixtures/adjustable-arc-desktop.rsk"),
    fixture!("tests/fixtures/bond-join-regression.rsk"),
    fixture!("tests/fixtures/tilted-fused-double-bonds.rsk"),
    fixture!("tests/fixtures/coordination-layout.rsk"),
    fixture!("tests/fixtures/pyrrole-rotation-drift.rsk"),
    fixture!("tests/fixtures/palette/legacy-dark.rsk"),
    fixture!("tests/fixtures/palette/legacy-light.rsk"),
    fixture!("tests/fixtures/geometry/adamantane-projection.rsk"),
    fixture!("tests/fixtures/geometry/adamantane-frozen.rsk"),
    fixture!("tests/fixtures/chemdraw-arrows/reaction-source.rsk"),
    fixture!("tests/fixtures/chemdraw-arrows/numeric-source.rsk"),
    fixture!("tests/fixtures/chemdraw-captions/source.rsk"),
    fixture!("tests/fixtures/ui-declutter/mixed-arrow-width.rsk"),
    fixture!("tests/fixtures/ui-expanded.reshiki"),
    fixture!("tests/fixtures/ui-drawn-ethanol.reshiki"),
];

const DISPLAYS: [&str; 9] = [
    "plain",
    "bold",
    "wedge",
    "hollow_wedge",
    "hash",
    "hashed",
    "wavy",
    "dashed",
    "dotted",
];
const RED: Color = Color::Custom([200, 30, 30]);

/// Record one drawing: the Joins constructions and heap figures of a warm
/// primitives() call, every primitive, bounds, the SVG and label boxes.
fn record(text: &mut String, name: &str, doc: &Document) {
    // The first call warms lazy state; measure a second, identical one.
    drop(primitives(doc));
    let before = crate::bond_joins::construction_count();
    let (drawing, allocations) = measured(|| primitives(doc));
    let joins = crate::bond_joins::construction_count() - before;
    writeln!(text, "# {name}").unwrap();
    writeln!(text, "joins={joins}").unwrap();
    writeln!(text, "alloc {allocations}").unwrap();
    for primitive in &drawing {
        writeln!(text, "{primitive:?}").unwrap();
    }
    writeln!(text, "bounds {:?}", bounds(&drawing)).unwrap();
    text.push_str(&svg_with_background(doc));
    for atom in &doc.atoms {
        writeln!(
            text,
            "atom {} ink {:?} bounds {:?}",
            atom.id,
            atom_label_ink_boxes(atom, doc),
            atom_label_bounds(atom, doc)
        )
        .unwrap();
    }
}

/// An isolated two-atom fragment, returning its bond for further styling.
fn fragment(doc: &mut Document, x: f32, y: f32, order: u8, display: &str) -> usize {
    let a = doc.add_atom("C", Point::new(x, y));
    let b = doc.add_atom("C", Point::new(x + 36., y + 21.));
    doc.add_bond(a, b, order, display);
    doc.bonds.len() - 1
}

fn ring(doc: &mut Document, center: Point, order: u8) -> Vec<u64> {
    let (r, h) = (30., 15. * 3f32.sqrt());
    let ids: Vec<_> = [
        (r, 0.),
        (r / 2., h),
        (-r / 2., h),
        (-r, 0.),
        (-r / 2., -h),
        (r / 2., -h),
    ]
    .into_iter()
    .map(|(x, y)| doc.add_atom("C", center.offset(x, y)))
    .collect();
    for (index, id) in ids.iter().enumerate() {
        doc.add_bond(*id, ids[(index + 1) % ids.len()], order, "plain");
    }
    ids
}

/// Bonds from a carbon center to each tip, colored as given.
fn star(doc: &mut Document, center: Point, tips: [(f32, f32, &str, Color); 3]) {
    let hub = doc.add_atom("C", center);
    for (x, y, display, color) in tips {
        let tip = doc.add_atom("C", center.offset(x, y));
        doc.add_bond(hub, tip, 1, display);
        doc.bonds.last_mut().unwrap().color = color;
    }
}

/// Every bond display and order, plus aromatic, positioned, secondary,
/// colored, crossing and junction cases.
fn bond_matrix() -> Document {
    let mut doc = Document::default();
    for (row, display) in DISPLAYS.into_iter().enumerate() {
        for order in 1..=7u8 {
            let x = f32::from(order - 1) * 60.;
            fragment(&mut doc, x, row as f32 * 60., order, display);
        }
    }
    let y = DISPLAYS.len() as f32 * 60.;
    for id in ring(&mut doc, Point::new(30., y + 30.), 4) {
        doc.atom_mut(id).unwrap().aromatic = true;
    }
    let chain: Vec<_> = (0..4)
        .map(|i| {
            doc.add_atom(
                "C",
                Point::new(120. + i as f32 * 36., y + (i % 2) as f32 * 21.),
            )
        })
        .collect();
    for (index, (order, position)) in [
        (2, DoublePosition::Left),
        (1, DoublePosition::Auto),
        (2, DoublePosition::Right),
    ]
    .into_iter()
    .enumerate()
    {
        doc.add_bond(chain[index], chain[index + 1], order, "plain");
        doc.bonds.last_mut().unwrap().double_position = position;
    }
    let secondary = fragment(&mut doc, 300., y, 2, "plain");
    doc.bonds[secondary].secondary_display = Some("dashed".into());
    for (index, (order, display)) in [(1, "plain"), (1, "bold"), (2, "plain")]
        .into_iter()
        .enumerate()
    {
        let colored = fragment(&mut doc, 360. + index as f32 * 60., y, order, display);
        doc.bonds[colored].color = RED;
    }
    let y = y + 90.;
    let corners = [(0., y), (40., y + 40.), (0., y + 40.), (40., y)]
        .map(|(x, y)| doc.add_atom("C", Point::new(x, y)));
    doc.add_bond(corners[0], corners[1], 1, "plain");
    doc.add_bond(corners[2], corners[3], 1, "plain");
    // A mixed-color junction whose first branch is crossed by a plain bond.
    star(
        &mut doc,
        Point::new(120., y + 20.),
        [
            (36., 0., "plain", Color::default()),
            (-18., -31., "plain", RED),
            (-18., 31., "bold", Color::default()),
        ],
    );
    let across = [(140., y), (140., y + 40.)].map(|(x, y)| doc.add_atom("C", Point::new(x, y)));
    doc.add_bond(across[0], across[1], 1, "plain");
    star(
        &mut doc,
        Point::new(240., y + 20.),
        [
            (36., 0., "plain", Color::default()),
            (-18., -31., "plain", Color::default()),
            (-18., 31., "hollow_wedge", Color::default()),
        ],
    );
    doc
}

/// The PNG used by the canvas pixel baseline: four quadrants, one transparent.
fn picture() -> Picture {
    let pixels = image::RgbaImage::from_fn(12, 8, |x, y| {
        image::Rgba(match (x < 6, y < 4) {
            (true, true) => [255, 0, 0, 255],
            (false, true) => [0, 255, 0, 255],
            (true, false) => [0, 0, 255, 255],
            (false, false) => [0, 0, 0, 0],
        })
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    Picture::import(bytes.get_ref()).unwrap()
}

/// A hexagon and chain with back and front pictures and rectangles, a ring
/// fill, two-color highlights, an annotation and a charged isotope label.
fn decorations() -> Document {
    let mut doc = Document::default();
    let hexagon = ring(&mut doc, Point::new(0., 0.), 1);
    let mut chain = vec![hexagon[0]];
    for (x, y) in [(66., -21.), (102., 0.), (138., -21.)] {
        chain.push(doc.add_atom("C", Point::new(x, y)));
    }
    for (index, display) in ["plain", "bold", "plain"].into_iter().enumerate() {
        doc.add_bond(chain[index], chain[index + 1], 1, display);
    }
    let nitrogen = doc.atom_mut(chain[3]).unwrap();
    nitrogen.element = "N".into();
    nitrogen.charge = 1;
    nitrogen.isotope = 15;
    let fill = Some(Color::Palette(Hue::Blue, Row::Tint));
    assert_eq!(ring_fills::apply(&mut doc, &hexagon, fill), 1);
    let ring_paint = Some(Color::Palette(Hue::Red, Row::Tint));
    assert!(highlights::apply(&mut doc, &hexagon[..3], ring_paint) > 0);
    let chain_paint = Some(Color::Custom([250, 220, 120]));
    assert!(highlights::apply(&mut doc, &chain[1..], chain_paint) > 0);
    let picture = picture();
    for (center, layer) in [(Point::new(-10., 5.), -1), (Point::new(100., 40.), 1)] {
        let mut graphic = picture.graphic(doc.next_id(), center);
        graphic.layer = layer;
        doc.graphics.push(graphic);
    }
    for (start, end, stroke, layer) in [
        (
            Point::new(-50., -45.),
            Point::new(150., 45.),
            Color::default(),
            -1,
        ),
        (Point::new(80., 20.), Point::new(130., 60.), RED, 1),
    ] {
        let style = GraphicStyle {
            stroke,
            ..Default::default()
        };
        let kind = GraphicKind::Rectangle;
        let id = doc.next_id();
        let mut graphic = Graphic::dragged(id, kind, start, end, style, BracketSides::Both, false);
        graphic.layer = layer;
        doc.graphics.push(graphic);
    }
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: Point::new(-30., 50.),
        text: "Decorated ring".into(),
        format: Default::default(),
    });
    doc.validate().unwrap();
    doc
}

#[test]
#[ignore = "Captured-baseline parity; set RESHIKI_MODEL_PARITY"]
fn scene_primitives_match_captured_baseline() {
    let mut text = String::new();
    for (path, bytes) in FIXTURES {
        let doc = Document::from_json(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        record(&mut text, path, &doc);
    }
    record(&mut text, "bond_matrix", &bond_matrix());
    record(&mut text, "decorations", &decorations());
    check_baseline("scene_primitives", &text);
}
