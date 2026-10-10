//! Captured-baseline parity for atom_label_runs: abbreviations in every
//! alignment, internal groups, variables, radicals, mark suppression, hidden
//! charges, colored hydrogens, isotopes and charged hidden carbons.
//! Run with RESHIKI_MODEL_PARITY set; see crate::parity_tests.
use super::{
    Primitive, atom_label_bounds, atom_label_hit, atom_label_ink_boxes, atom_label_runs, primitives,
};
use crate::{
    abbreviations::LabelAlignment,
    document::{Document, Point},
    palette::{Color, Hue, Row},
    parity_tests::{check_baseline, measured},
    scientific::{AtomMark, MarkKind},
    typography::TextStyle,
};
use std::fmt::Write as _;

const RADIUS: f32 = 1.;
/// Opposing horizontal bonds stack an appendage above.
const HORIZONTAL: &[(f32, f32)] = &[(-36., 0.), (36., 0.)];
/// Two upward bonds stack an appendage below.
const UPWARD: &[(f32, f32)] = &[(-36., -21.), (36., -21.)];
/// One bond to the left keeps an appendage on the right.
const LEFT: &[(f32, f32)] = &[(-36., 21.)];
/// One bond to the right moves an appendage to the left.
const RIGHT: &[(f32, f32)] = &[(36., 21.)];
/// Two bonds to the left keep an appendage inline on the right.
const LEFT_PAIR: &[(f32, f32)] = &[(-36., 21.), (-36., -21.)];
/// Two bonds to the right move an appendage inline to the left.
const RIGHT_PAIR: &[(f32, f32)] = &[(36., 21.), (36., -21.)];

/// One cell of a drawing grid, so labels never overlap.
fn cell(column: usize, row: usize) -> Point {
    Point::new(column as f32 * 150., row as f32 * 90.)
}

/// An atom bonded to carbon neighbors at the given offsets.
fn center(doc: &mut Document, element: &str, at: Point, neighbors: &[(f32, f32)]) -> u64 {
    let id = doc.add_atom(element, at);
    for &(x, y) in neighbors {
        let other = doc.add_atom("C", at.offset(x, y));
        doc.add_bond(id, other, 1, "plain");
    }
    id
}

/// One center per column, returning their IDs. Set atom fields only after
/// this: adding a bond clears label_h on every atom.
fn centers(doc: &mut Document, cases: &[(&str, &[(f32, f32)])]) -> Vec<u64> {
    cases
        .iter()
        .enumerate()
        .map(|(column, &(element, neighbors))| center(doc, element, cell(column, 0), neighbors))
        .collect()
}

fn mark(kind: MarkKind) -> AtomMark {
    AtomMark {
        id: None,
        kind,
        offset: Point::default(),
        angle: 0.,
        size_pt: None,
    }
}

/// A formula and a nickname, contracted in every alignment with the anchor
/// bond pointing left and right.
fn abbreviations() -> Document {
    let mut doc = Document::default();
    for (block, (label, reverse)) in [("CO2Me", "MeO2C"), ("Boc", "")].into_iter().enumerate() {
        for (column, alignment) in LabelAlignment::ALL.into_iter().enumerate() {
            for (side, dx) in [-36f32, 36.].into_iter().enumerate() {
                let at = cell(column, block * 2 + side);
                let outside = doc.add_atom("C", at.offset(dx, 21.));
                let anchor = doc.add_atom("C", at);
                let member = doc.add_atom("O", at.offset(-dx, 21.));
                doc.add_bond(outside, anchor, 1, "plain");
                doc.add_bond(anchor, member, 1, "plain");
                doc.contract(&[anchor, member], label, reverse).unwrap();
                doc.abbreviations.last_mut().unwrap().alignment = alignment;
            }
        }
    }
    doc.validate().unwrap();
    doc
}

/// Auto condensed labels on anchors with two visible bonds, so each draws as
/// an internal group with its suffix stacked or inline.
fn internal_groups() -> Document {
    let mut doc = Document::default();
    for (column, (element, label, neighbors, isotope)) in [
        ("N", "NMe", HORIZONTAL, 0),
        ("C", "CCl2", LEFT_PAIR, 13),
        ("C", "CF2", RIGHT_PAIR, 0),
        ("N", "NMe", UPWARD, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let at = cell(column, 0);
        let anchor = center(&mut doc, element, at, neighbors);
        doc.atom_mut(anchor).unwrap().isotope = isotope;
        let member = doc.add_atom("C", at.offset(0., 36.));
        doc.add_bond(anchor, member, 1, "plain");
        doc.contract(&[anchor, member], label, "").unwrap();
    }
    doc.validate().unwrap();
    for group in &doc.abbreviations {
        let anchor = doc.atom(group.anchor).unwrap();
        let runs = atom_label_runs(anchor, &doc);
        assert!(
            matches!(runs.first(), Some(Primitive::Text { text, .. }) if *text == anchor.element),
            "{} must draw as an internal group",
            group.label
        );
    }
    doc
}

/// Wildcard atoms: plain (hidden), a variable label and a condensed one.
fn variables() -> Document {
    let mut doc = Document::default();
    let cases = [None, Some("R1"), Some("CO2Et")];
    let ids = centers(&mut doc, &cases.map(|_| ("*", LEFT)));
    for (id, variable) in ids.into_iter().zip(cases) {
        doc.atom_mut(id).unwrap().display.variable = variable.map(Into::into);
    }
    doc.validate().unwrap();
    doc
}

/// One and two radical electrons on a neutral carbon and on N+, with stacked
/// and inline hydrogens, plus a bare carbon radical.
fn radicals() -> Document {
    let mut doc = Document::default();
    let cases = [
        ("C", 0, 1u8, HORIZONTAL),
        ("C", 0, 2, LEFT),
        ("N", 1, 1, HORIZONTAL),
        ("N", 1, 2, LEFT),
        ("C", 0, 1, &[][..]),
    ];
    let ids = centers(
        &mut doc,
        &cases.map(|(element, .., neighbors)| (element, neighbors)),
    );
    for (id, (_, charge, radicals, neighbors)) in ids.into_iter().zip(cases) {
        let atom = doc.atom_mut(id).unwrap();
        atom.charge = charge;
        atom.radical_electrons = radicals;
        if !neighbors.is_empty() {
            atom.label_h = u32::from(radicals);
        }
    }
    doc.validate().unwrap();
    doc
}

/// Charge and radical marks that do and do not suppress the printed state.
fn marks() -> Document {
    let mut doc = Document::default();
    let cases = [
        ("N", 1, 0, MarkKind::Charge),
        ("N", 1, 0, MarkKind::RadicalIon),
        ("N", 1, 1, MarkKind::RadicalIon),
        ("C", -1, 1, MarkKind::RadicalIon),
        ("C", 0, 1, MarkKind::RadicalIon),
        ("C", 0, 1, MarkKind::Radical),
        ("O", -1, 0, MarkKind::CircledCharge),
        ("N", 1, 2, MarkKind::LonePair),
    ];
    let ids = centers(&mut doc, &cases.map(|(element, ..)| (element, LEFT)));
    for (id, (_, charge, radicals, kind)) in ids.into_iter().zip(cases) {
        let atom = doc.atom_mut(id).unwrap();
        atom.charge = charge;
        atom.radical_electrons = radicals;
        atom.label_h = 1;
        atom.marks.push(mark(kind));
    }
    doc.validate().unwrap();
    doc
}

/// Charges hidden from print on a labeled N+, a bare O- and a skeletal C+.
fn hidden_charges() -> Document {
    let mut doc = Document::default();
    let cases = [("N", 1, LEFT), ("O", -1, &[][..]), ("C", 1, HORIZONTAL)];
    let ids = centers(
        &mut doc,
        &cases.map(|(element, _, neighbors)| (element, neighbors)),
    );
    for (id, (_, charge, _)) in ids.into_iter().zip(cases) {
        let atom = doc.atom_mut(id).unwrap();
        atom.charge = charge;
        atom.label_h = 1;
        atom.display.hide_charge = true;
    }
    doc.validate().unwrap();
    doc
}

/// Recolored hydrogens stacked above and below, on the right and on the left.
fn hydrogen_colors() -> Document {
    let mut doc = Document::default();
    let cases = [
        ("N", 2, 0, HORIZONTAL, Color::Custom([20, 120, 200])),
        ("N", 2, 1, UPWARD, Color::Palette(Hue::Blue, Row::Strong)),
        ("N", 1, 0, LEFT, Color::Custom([200, 30, 30])),
        ("O", 1, 1, RIGHT, Color::Palette(Hue::Teal, Row::Strong)),
    ];
    let ids = centers(
        &mut doc,
        &cases.map(|(element, .., neighbors, _)| (element, neighbors)),
    );
    for (id, (_, hydrogens, charge, _, color)) in ids.into_iter().zip(cases) {
        let atom = doc.atom_mut(id).unwrap();
        atom.label_h = hydrogens;
        atom.charge = charge;
        atom.display.hydrogen_color = Some(color);
    }
    doc.validate().unwrap();
    doc
}

/// Isotopes with charges, hydrogens and a custom label style.
fn isotopes() -> Document {
    let mut doc = Document::default();
    let cases = [
        ("N", 15, 1, 1, LEFT),
        ("C", 13, -1, 0, &[][..]),
        ("O", 18, -2, 0, &[][..]),
        ("H", 2, 1, 1, &[][..]),
        ("N", 15, 2, 2, HORIZONTAL),
    ];
    let ids = centers(
        &mut doc,
        &cases.map(|(element, .., neighbors)| (element, neighbors)),
    );
    for (column, (id, (_, isotope, charge, hydrogens, _))) in ids.into_iter().zip(cases).enumerate()
    {
        let atom = doc.atom_mut(id).unwrap();
        atom.isotope = isotope;
        atom.charge = charge;
        atom.label_h = hydrogens;
        if column == 4 {
            atom.text_style = Some(TextStyle {
                size_pt: 14.,
                bold: true,
                color: Color::Palette(Hue::Purple, Row::Strong),
                ..Default::default()
            });
        }
    }
    doc.validate().unwrap();
    doc
}

/// Skeletal carbons: charged ones print only the charge, a neutral one nothing.
fn hidden_carbons() -> Document {
    let mut doc = Document::default();
    let cases = [(-1, 0), (2, 2), (0, 1)];
    let ids = centers(&mut doc, &cases.map(|_| ("C", HORIZONTAL)));
    for (id, (charge, hydrogens)) in ids.into_iter().zip(cases) {
        let atom = doc.atom_mut(id).unwrap();
        atom.charge = charge;
        atom.label_h = hydrogens;
    }
    doc.validate().unwrap();
    doc
}

/// The first run of the first label, the last run of the last label (an
/// appendage, charge or radical), a point just outside that run's ink but
/// inside the hit radius, and a miss that scans every atom.
fn probes(doc: &Document) -> [Point; 4] {
    let labels: Vec<_> = doc
        .atoms
        .iter()
        .map(|atom| atom_label_ink_boxes(atom, doc))
        .filter(|boxes| !boxes.is_empty())
        .collect();
    let (lo, hi) = labels.first().and_then(|b| b.first()).copied().unwrap();
    let (last_lo, last_hi) = labels.last().and_then(|b| b.last()).copied().unwrap();
    let middle = |lo: Point, hi: Point| Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    [
        middle(lo, hi),
        middle(last_lo, last_hi),
        last_lo.offset(-0.5, -0.5),
        Point::new(-1000., -1000.),
    ]
}

/// Record every label stage of one drawing: runs with their heap figures,
/// ink boxes, bounds, pointer hits and the complete primitives.
fn record(text: &mut String, name: &str, doc: &Document) {
    writeln!(text, "# {name}").unwrap();
    for atom in &doc.atoms {
        // The first call warms lazy state; measure a second, identical one.
        drop(atom_label_runs(atom, doc));
        let (runs, allocations) = measured(|| atom_label_runs(atom, doc));
        writeln!(text, "atom {} alloc {allocations}", atom.id).unwrap();
        writeln!(text, "runs {runs:?}").unwrap();
        writeln!(text, "ink {:?}", atom_label_ink_boxes(atom, doc)).unwrap();
        writeln!(text, "bounds {:?}", atom_label_bounds(atom, doc)).unwrap();
    }
    for point in probes(doc) {
        let _ = atom_label_hit(doc, point, RADIUS);
        let (hit, allocations) = measured(|| atom_label_hit(doc, point, RADIUS));
        writeln!(text, "hit {point:?} {hit:?} alloc {allocations}").unwrap();
    }
    for primitive in primitives(doc) {
        writeln!(text, "{primitive:?}").unwrap();
    }
}

#[test]
#[ignore = "Captured-baseline parity; set RESHIKI_MODEL_PARITY"]
fn atom_label_runs_match_captured_baseline() {
    let mut text = String::new();
    for (name, doc) in [
        ("abbreviations", abbreviations()),
        ("internal_groups", internal_groups()),
        ("variables", variables()),
        ("radicals", radicals()),
        ("marks", marks()),
        ("hidden_charges", hidden_charges()),
        ("hydrogen_colors", hydrogen_colors()),
        ("isotopes", isotopes()),
        ("hidden_carbons", hidden_carbons()),
    ] {
        record(&mut text, name, &doc);
    }
    check_baseline("atom_label_runs", &text);
}
