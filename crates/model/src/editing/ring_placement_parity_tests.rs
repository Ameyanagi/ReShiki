//! Captured-baseline parity for ring_placement: every ring size, plain and
//! aromatic, at empty, atom and bond targets, plus the input guard and each
//! rejection. Run with RESHIKI_MODEL_PARITY set; see crate::parity_tests.
use super::{RingRejection, ring_placement};
use crate::{
    document::{Document, Point},
    parity_tests::{check_baseline, measured},
    rings::Preset,
};
use std::fmt::Write as _;

type Placed = Result<(Document, Vec<u64>), RingRejection>;

const RADIUS: f32 = 5.;
const LENGTH: f32 = 42.;

/// Record one placement and the heap figures of a warm repeat.
fn record(text: &mut String, case: &str, place: impl Fn() -> Placed) {
    let result = place();
    writeln!(text, "{case}: {result:?}").unwrap();
    // The call above warms the path; measure a second, identical one.
    let (repeat, allocations) = measured(&place);
    drop(repeat);
    writeln!(text, "{case} alloc {allocations}").unwrap();
}

/// A drawing, the clicked point and hit radius, and one drag point on each
/// side of the target.
struct Target {
    name: &'static str,
    doc: Document,
    p: Point,
    radius: f32,
    sides: [Point; 2],
}

fn target(name: &'static str, doc: Document, p: Point, radius: f32) -> Target {
    let sides = [p.offset(12., -60.), p.offset(-9., 55.)];
    Target {
        name,
        doc,
        p,
        radius,
        sides,
    }
}

fn lone() -> Document {
    let mut doc = Document::default();
    doc.add_atom("C", Point::new(0., 0.));
    doc
}

/// A zigzag C–C–C chain; the click targets its first atom.
fn chain() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(36.373, -21.));
    let c = doc.add_atom("C", Point::new(72.746, 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc
}

/// Two atoms joined by one horizontal bond from (0, 0) to (LENGTH, 0).
fn bond(order: u8, display: &str) -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(LENGTH, 0.));
    doc.add_bond(a, b, order, display);
    doc
}

/// A C–C=C–C chain whose horizontal double bond carries E stereochemistry.
fn stereo_bond() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-36.373, 21.));
    let b = doc.add_atom("C", Point::new(0., 0.));
    let c = doc.add_atom("C", Point::new(LENGTH, 0.));
    let d = doc.add_atom("C", Point::new(78.373, 21.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 2, "plain");
    doc.add_bond(c, d, 1, "plain");
    let double = &mut doc.bonds[1];
    double.stereo = Some("E".into());
    double.stereo_atoms = vec![a, d];
    doc
}

/// Neutral N with three single bonds: no valence left for a shared atom.
fn saturated() -> Document {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    for (x, y) in [(0., -42.), (36.373, 21.), (-36.373, 21.)] {
        let c = doc.add_atom("C", Point::new(x, y));
        doc.add_bond(n, c, 1, "plain");
    }
    doc
}

/// Two bonded atoms at the same point, so the attached ring has no length.
fn zero_length() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(0., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc
}

fn midpoint(doc: &Document, index: usize) -> Point {
    let bond = &doc.bonds[index];
    let a = doc.atom(bond.a).unwrap().position;
    let b = doc.atom(bond.b).unwrap().position;
    Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)
}

fn targets() -> Vec<Target> {
    let hexagon = Preset::Regular.document(LENGTH, false);
    let ring_atom = hexagon.atoms[0].position;
    let edge = midpoint(&hexagon, 0);
    let free = Point::new(150., 90.);
    let mut out = vec![
        target("empty radius 0", Document::default(), free, 0.),
        target("empty radius 5", Document::default(), free, RADIUS),
        target("lone atom", lone(), Point::new(0., 0.), RADIUS),
        target("chain terminus", chain(), Point::new(0., 0.), RADIUS),
        target("ring atom", hexagon.clone(), ring_atom, RADIUS),
        target("single bond", bond(1, "plain"), Point::new(21., 0.), RADIUS),
        target("double bond", bond(2, "plain"), Point::new(21., 0.), RADIUS),
        target("wedge bond", bond(1, "wedge"), Point::new(21., 0.), RADIUS),
        target("stereo bond", stereo_bond(), Point::new(21., 0.), RADIUS),
        target(
            "unsupported bond",
            bond(1, "dashed"),
            Point::new(21., 0.),
            RADIUS,
        ),
        target("saturated atom", saturated(), Point::new(0., 0.), RADIUS),
        target(
            "zero-length bond",
            zero_length(),
            Point::new(0., 0.),
            RADIUS,
        ),
    ];
    // The hexagon is centred on the origin: inward overlaps, outward fits.
    out.push(Target {
        name: "overlap",
        doc: hexagon,
        p: edge,
        radius: RADIUS,
        sides: [Point::new(0., 0.), Point::new(edge.x * 2., edge.y * 2.)],
    });
    out
}

fn placements(text: &mut String) {
    for target in targets() {
        let [first, second] = target.sides;
        let directions = [
            ("none", None),
            ("side 1", Some(first)),
            ("side 2", Some(second)),
        ];
        for size in 2..=9 {
            for aromatic in [false, true] {
                for (side, direction) in directions {
                    let case = format!(
                        "{} size={size} aromatic={aromatic} direction={side}",
                        target.name
                    );
                    record(text, &case, || {
                        ring_placement(
                            &target.doc,
                            target.p,
                            size,
                            aromatic,
                            target.radius,
                            direction,
                        )
                    });
                }
            }
        }
    }
}

fn invalid_inputs(text: &mut String) {
    let doc = lone();
    let p = Point::new(0., 0.);
    let cases = [
        ("NaN p.x", Point::new(f32::NAN, 0.), RADIUS, None),
        ("NaN p.y", Point::new(0., f32::NAN), RADIUS, None),
        ("infinite radius", p, f32::INFINITY, None),
        ("radius -1", p, -1., None),
        ("NaN direction", p, RADIUS, Some(Point::new(f32::NAN, 10.))),
    ];
    for (name, p, radius, direction) in cases {
        for aromatic in [false, true] {
            record(text, &format!("invalid {name} aromatic={aromatic}"), || {
                ring_placement(&doc, p, 6, aromatic, radius, direction)
            });
        }
    }
}

#[test]
#[ignore = "Captured-baseline parity; set RESHIKI_MODEL_PARITY"]
fn ring_placement_matches_captured_baseline() {
    let mut text = String::new();
    placements(&mut text);
    invalid_inputs(&mut text);
    check_baseline("ring_placement", &text);
}
