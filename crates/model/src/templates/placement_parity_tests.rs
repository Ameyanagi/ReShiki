//! Captured-baseline parity for template placement: free placement, atom and
//! bond targets, error precedence, connection modes, source shapes and
//! metadata merges. Run with RESHIKI_MODEL_PARITY set; see crate::parity_tests.
use super::{Anchor, Connection, LIBRARY, Template, place_anchored, place_with_mode};
use crate::{
    document::{Document, Point},
    editing,
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
    highlights,
    palette::{Color, Hue, Row},
    parity_tests::{check_baseline, measured},
    ring_fills,
};
use std::fmt::Write as _;

type Placed = Result<(Document, Vec<u64>), &'static str>;

const RADIUS: f32 = 5.;
const FILL: Color = Color::Palette(Hue::Blue, Row::Tint);
const PAINT: Color = Color::Palette(Hue::Red, Row::Strong);
const PART_PAINT: Color = Color::Palette(Hue::Green, Row::Strong);

/// Record one placement and the heap figures of a warm repeat, returning the
/// first call's error, if any.
fn record(text: &mut String, case: &str, place: impl Fn() -> Placed) -> Option<&'static str> {
    let result = place();
    writeln!(text, "{case}: {result:?}").unwrap();
    // The call above warms the path; measure a second, identical one.
    let (repeat, allocations) = measured(&place);
    drop(repeat);
    writeln!(text, "{case} alloc {allocations}").unwrap();
    result.err()
}

fn template(name: &str) -> &'static Template {
    LIBRARY
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("missing template {name}"))
}

/// The ring, heterocycle, conformer, Haworth and acyclic sources.
fn sources() -> Vec<&'static Template> {
    let mut list: Vec<_> = [
        "Benzene",
        "Cyclohexane",
        "Cyclopentane",
        "Pyridine",
        "Furan",
        "Naphthalene",
    ]
    .into_iter()
    .map(template)
    .collect();
    list.extend(LIBRARY.iter().filter(|t| t.group == "Conformers"));
    list.extend(LIBRARY.iter().find(|t| t.group == "Carbohydrates"));
    list.push(template("Acetic acid"));
    assert_eq!(list.len(), 10);
    list
}

fn anchors(part: &Document) -> Vec<(String, Anchor)> {
    let mut out = vec![("auto".to_string(), Anchor::Auto)];
    let atom = part.atoms[0].id;
    out.push((format!("atom {atom}"), Anchor::Atom(atom)));
    for bond in &part.bonds {
        out.push((
            format!("bond {}-{}", bond.a, bond.b),
            Anchor::Bond(bond.a, bond.b),
        ));
    }
    out
}

fn lone(element: &str) -> (Document, u64) {
    let mut doc = Document::default();
    let id = doc.add_atom(element, Point::new(0., 0.));
    (doc, id)
}

/// A zigzag C–C–C chain whose first bond has the given order and display.
fn chain(order: u8, display: &str, stereo: Option<&str>) -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(36.373, -21.));
    let c = doc.add_atom("C", Point::new(72.746, 0.));
    doc.add_bond(a, b, order, display);
    doc.add_bond(b, c, 1, "plain");
    doc.bonds[0].stereo = stereo.map(Into::into);
    doc.validate().unwrap();
    doc
}

/// Neutral N with three single bonds: no valence left for a shared atom.
fn saturated_nitrogen() -> (Document, u64) {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    for (x, y) in [(0., -42.), (36.373, 21.), (-36.373, 21.)] {
        let c = doc.add_atom("C", Point::new(x, y));
        doc.add_bond(n, c, 1, "plain");
    }
    (doc, n)
}

fn aromatic_benzene() -> Document {
    let mut doc = template("Benzene").document.clone();
    for bond in &mut doc.bonds {
        bond.order = 4;
    }
    for atom in &mut doc.atoms {
        atom.aromatic = true;
    }
    doc.validate().unwrap();
    doc
}

fn filled(mut doc: Document) -> Document {
    let ids = doc.all_ids();
    ring_fills::apply(&mut doc, &ids, Some(FILL));
    assert!(!doc.ring_fills.is_empty());
    doc
}

fn position(doc: &Document, id: u64) -> Point {
    doc.atom(id).unwrap().position
}

fn fixture(json: &str) -> Document {
    let doc: Document = serde_json::from_str(json).unwrap();
    doc.validate().unwrap();
    doc
}

fn free_placement(text: &mut String) {
    let point = Point::new(150., 90.);
    let directions = [("none", None), ("far", Some(Point::new(210., 55.)))];
    let empty = Document::default();
    let mut scaled = Document::default();
    scaled.drawing_style.set_bond_length(18.);
    scaled.validate().unwrap();
    for t in LIBRARY.iter() {
        for (dir, direction) in directions {
            record(text, &format!("free {} {dir}", t.id), || {
                place_anchored(&empty, &t.document, point, direction, RADIUS, t.anchor)
            });
            record(text, &format!("free scaled {} {dir}", t.id), || {
                t.place(
                    &scaled,
                    point,
                    direction,
                    RADIUS,
                    t.anchor,
                    Connection::default(),
                )
            });
        }
    }
}

fn atom_targets(text: &mut String) {
    let (lone_carbon, carbon) = lone("C");
    let chain_end = chain(1, "plain", None);
    let ring = template("Cyclohexane").document.clone();
    let ring_atom = ring.atoms[0].id;
    let (nitrogen, saturated) = saturated_nitrogen();
    let targets = [
        ("lone C", &lone_carbon, carbon),
        ("chain end", &chain_end, chain_end.atoms[0].id),
        ("ring atom", &ring, ring_atom),
        ("saturated N", &nitrogen, saturated),
    ];
    for t in sources() {
        for (target, doc, id) in targets {
            let point = position(doc, id);
            for (dir, direction) in [("none", None), ("far", Some(point.offset(60., -35.)))] {
                for (anchor_label, anchor) in anchors(&t.document).into_iter().take(3) {
                    let case = format!("atom {} on {target} {dir} {anchor_label}", t.id);
                    record(text, &case, || {
                        place_anchored(doc, &t.document, point, direction, RADIUS, anchor)
                    });
                }
            }
        }
    }
}

fn bond_targets(text: &mut String) {
    let targets = [
        ("single", chain(1, "plain", None)),
        ("double", chain(2, "plain", None)),
        ("aromatic", aromatic_benzene()),
        ("bold", chain(1, "bold", None)),
        ("stereo", chain(2, "plain", Some("any"))),
    ];
    for t in sources() {
        for (target, doc) in &targets {
            let bond = &doc.bonds[0];
            let (a, b) = (position(doc, bond.a), position(doc, bond.b));
            let point = super::midpoint(a, b);
            let length = a.distance(b);
            let (nx, ny) = (-(b.y - a.y) / length * 40., (b.x - a.x) / length * 40.);
            for (dir, direction) in [
                ("none", None),
                ("side+", Some(point.offset(nx, ny))),
                ("side-", Some(point.offset(-nx, -ny))),
            ] {
                for (anchor_label, anchor) in anchors(&t.document) {
                    let case = format!("bond {} on {target} {dir} {anchor_label}", t.id);
                    record(text, &case, || {
                        place_anchored(doc, &t.document, point, direction, RADIUS, anchor)
                    });
                }
            }
        }
    }
}

/// Called directly, so place_with_mode's combined validator cannot hide the
/// order: drawing, then geometry, then anchor and empty template.
fn error_precedence(text: &mut String) {
    let (doc, id) = lone("C");
    let mut invalid = doc.clone();
    invalid.version = 0;
    let part = &template("Benzene").document;
    let empty = Document::default();
    let point = position(&doc, id);
    let nan = Point::new(f32::NAN, 0.);
    let missing = Anchor::Atom(9999);
    let auto = Anchor::Auto;
    record(text, "error invalid doc", || {
        place_anchored(&invalid, part, point, None, RADIUS, auto)
    });
    let error = record(text, "error invalid doc + NaN point", || {
        place_anchored(&invalid, part, nan, None, RADIUS, auto)
    });
    assert_eq!(error, Some("The drawing or template is invalid."));
    let error = record(text, "error NaN point + invalid anchor", || {
        place_anchored(&doc, part, nan, None, RADIUS, missing)
    });
    assert_eq!(error, Some("Invalid attachment geometry."));
    record(text, "error radius 0", || {
        place_anchored(&doc, part, point, None, 0., auto)
    });
    record(text, "error radius -1", || {
        place_anchored(&doc, part, point, None, -1., auto)
    });
    record(text, "error NaN direction", || {
        place_anchored(&doc, part, point, Some(nan), RADIUS, auto)
    });
    let error = record(text, "error invalid anchor + empty part", || {
        place_anchored(&doc, &empty, point, None, RADIUS, missing)
    });
    assert_eq!(
        error,
        Some("Choose an attachment point in a nonempty template.")
    );
    record(text, "error empty part", || {
        place_anchored(&doc, &empty, point, None, RADIUS, auto)
    });
}

fn connection_modes(text: &mut String) {
    let doc = chain(1, "plain", None);
    let end = position(&doc, doc.atoms[0].id);
    let bond = super::midpoint(end, position(&doc, doc.atoms[1].id));
    let points = [
        ("atom", end),
        ("bond", bond),
        ("empty", Point::new(200., 150.)),
    ];
    let modes = [
        Connection::Auto,
        Connection::Connect,
        Connection::ShareAtom,
        Connection::FuseBond,
    ];
    for t in [template("Benzene"), template("Acetic acid")] {
        let part = &t.document;
        let choices: Vec<_> = anchors(part).into_iter().take(3).collect();
        for mode in modes {
            for (at, point) in points {
                for (dir, direction) in [("none", None), ("far", Some(point.offset(60., -35.)))] {
                    for (anchor_label, anchor) in &choices {
                        let case = format!("mode {mode:?} {} at {at} {dir} {anchor_label}", t.id);
                        record(text, &case, || {
                            place_with_mode(&doc, part, point, direction, RADIUS, *anchor, mode)
                        });
                    }
                }
            }
        }
    }
    let empty = Document::default();
    record(text, "mode Connect empty part at atom", || {
        place_with_mode(
            &doc,
            &empty,
            end,
            None,
            RADIUS,
            Anchor::Auto,
            Connection::Connect,
        )
    });
    let (nitrogen, saturated) = saturated_nitrogen();
    let point = position(&nitrogen, saturated);
    for t in [template("Benzene"), template("Acetic acid")] {
        record(
            text,
            &format!("mode Connect {} at saturated N", t.id),
            || {
                place_with_mode(
                    &nitrogen,
                    &t.document,
                    point,
                    None,
                    RADIUS,
                    Anchor::Auto,
                    Connection::Connect,
                )
            },
        );
    }
}

fn source_shapes(text: &mut String) {
    let (doc, id) = lone("C");
    let point = position(&doc, id);
    let mut graphic = Document::default();
    graphic.graphics.push(Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(40., 30.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    graphic.validate().unwrap();
    let (oxygen, _) = lone("O");
    let mut methanol = Document::default();
    let c = methanol.add_atom("C", Point::new(0., 0.));
    let o = methanol.add_atom("O", Point::new(42., 0.));
    methanol.add_bond(c, o, 1, "plain");
    let cases = [
        ("graphic on lone C", &graphic, &doc, point),
        (
            "single O on methanol O",
            &oxygen,
            &methanol,
            position(&methanol, o),
        ),
        ("single O on lone C", &oxygen, &doc, point),
    ];
    for (case, part, doc, point) in cases {
        for (dir, direction) in [("none", None), ("far", Some(point.offset(60., -35.)))] {
            record(text, &format!("shape {case} {dir}"), || {
                place_anchored(doc, part, point, direction, RADIUS, Anchor::Auto)
            });
        }
    }
}

fn coincident_neighbors(text: &mut String) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(0., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.validate().unwrap();
    let part = &template("Benzene").document;
    record(text, "coincident neighbors", || {
        place_anchored(&doc, part, Point::new(0., 0.), None, RADIUS, Anchor::Auto)
    });
}

fn metadata_merges(text: &mut String) {
    let benzene = &template("Benzene").document;
    let cyclohexane = &template("Cyclohexane").document;
    let acid = &template("Acetic acid").document;

    let gallery = fixture(include_str!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ));
    // 455 is a centroid member; 3 is a plain terminus, so the merge succeeds
    // and visits every centroid atom.
    for id in [455, 3] {
        let point = position(&gallery, id);
        record(text, &format!("merge centroid gallery atom {id}"), || {
            place_anchored(&gallery, benzene, point, None, RADIUS, Anchor::Auto)
        });
    }

    let coordination = fixture(include_str!(
        "../../../../tests/fixtures/coordination-layout.rsk"
    ));
    let point = position(&coordination, 25);
    // Every imported atom has no_implicit set, which compatible() rejects. The
    // implicit copy lets a merge succeed and rewrite the group members.
    let mut implicit = coordination.clone();
    implicit.atom_mut(25).unwrap().no_implicit = false;
    implicit.validate().unwrap();
    for (label, doc) in [("", &coordination), (" implicit", &implicit)] {
        for (name, part) in [("Benzene", benzene), ("Cyclohexane", cyclohexane)] {
            record(
                text,
                &format!("merge grouped atom 25{label} {name}"),
                || place_anchored(doc, part, point, None, RADIUS, Anchor::Auto),
            );
        }
    }

    let mut reaction = fixture(include_str!(
        "../../../../tests/fixtures/chemdraw-arrows/reaction-source.rsk"
    ));
    let ring = editing::append(&mut reaction, benzene, Point::new(-134., 200.));
    assert_eq!(ring_fills::apply(&mut reaction, &ring, Some(FILL)), 1);
    reaction.contract(&[1], "Me", "Me").unwrap();
    assert!(highlights::apply(&mut reaction, &[2, 3], Some(PAINT)) > 0);
    reaction.validate().unwrap();
    let mut painted = acid.clone();
    let all = painted.all_ids();
    highlights::apply(&mut painted, &all, Some(PART_PAINT));
    let point = position(&reaction, 3);
    for (name, part) in [
        ("Acetic acid", acid),
        ("painted Acetic acid", &painted),
        ("Benzene", benzene),
    ] {
        for (dir, direction) in [("none", None), ("far", Some(point.offset(60., -35.)))] {
            record(text, &format!("merge reaction atom 3 {name} {dir}"), || {
                place_anchored(&reaction, part, point, direction, RADIUS, Anchor::Auto)
            });
        }
    }

    let ring = filled(benzene.clone());
    let edge = &ring.bonds[0];
    let point = super::midpoint(position(&ring, edge.a), position(&ring, edge.b));
    let filled_part = filled(benzene.clone());
    for (name, part) in [("Benzene", benzene), ("filled Benzene", &filled_part)] {
        record(text, &format!("merge filled ring edge {name}"), || {
            place_anchored(&ring, part, point, None, RADIUS, Anchor::Auto)
        });
    }
}

#[test]
#[ignore = "Captured-baseline parity; set RESHIKI_MODEL_PARITY"]
fn template_placement_matches_captured_baseline() {
    let mut text = String::new();
    free_placement(&mut text);
    atom_targets(&mut text);
    bond_targets(&mut text);
    error_precedence(&mut text);
    connection_modes(&mut text);
    source_shapes(&mut text);
    coincident_neighbors(&mut text);
    metadata_merges(&mut text);
    check_baseline("template_placement", &text);
}
