//! Shared drawing fixtures and ink assertions for generic module tests.
use super::ink::{atom_bounds, reach};
use crate::{
    atom_text::{self, Mode},
    document::{Document, Point},
    generic,
};

pub(super) fn chain(elements: &[&str]) -> Document {
    let mut doc = Document::default();
    for (i, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(i as f32 * 42., 0.));
        if i > 0 {
            doc.add_bond(id - 1, id, 1, "plain");
        }
    }
    doc
}

pub(super) fn formula(doc: &Document) -> String {
    let molecule = crate::chemistry::document::prepare(doc).unwrap();
    let facts: Vec<_> = molecule
        .state
        .graph
        .atoms
        .iter()
        .zip(&molecule.state.valences)
        .map(|(a, v)| crate::chemistry::AtomFacts {
            atomic_number: a.atomic_number,
            isotope: a.isotope,
            charge: a.charge,
            hydrogens: (u32::from(a.explicit_hydrogens) + v.implicit_hydrogens) as u8,
            radical_electrons: a.radical_electrons,
        })
        .collect();
    crate::chemistry::properties(&facts).unwrap().formula
}

pub(super) fn neighbors(doc: &Document, id: u64) -> Vec<u64> {
    doc.bonds
        .iter()
        .filter_map(|b| {
            if b.a == id {
                Some(b.b)
            } else if b.b == id {
                Some(b.a)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn preview(doc: &Document) -> Document {
    let molecule = crate::chemistry::document::prepare(doc).unwrap();
    let mut result = doc.clone();
    for (i, a) in result.atoms.iter_mut().enumerate() {
        a.label_h = u32::from(molecule.state.graph.atoms[i].explicit_hydrogens)
            + molecule.state.valences[i].implicit_hydrogens;
    }
    result
}

pub(super) fn legible(doc: &Document) {
    let shown = preview(doc);
    let boxes: Vec<_> = shown
        .atoms
        .iter()
        .map(|a| atom_bounds(a, &shown).unwrap())
        .collect();
    for (i, a) in shown.atoms.iter().enumerate() {
        for (j, z) in shown.atoms.iter().enumerate().skip(i + 1) {
            if let (Some((lo, hi)), Some((zl, zh))) = (boxes[i], boxes[j]) {
                assert!(
                    hi.x <= zl.x || zh.x <= lo.x || hi.y <= zl.y || zh.y <= lo.y,
                    "labels {} and {} overlap",
                    a.id,
                    z.id
                );
            }
        }
    }
    for b in &shown.bonds {
        let i = shown.atoms.iter().position(|a| a.id == b.a).unwrap();
        let j = shown.atoms.iter().position(|a| a.id == b.b).unwrap();
        let a = &shown.atoms[i];
        let z = &shown.atoms[j];
        let distance = a.position.distance(z.position);
        let ux = (z.position.x - a.position.x) / distance;
        let uy = (z.position.y - a.position.y) / distance;
        let available =
            distance - reach(a.position, boxes[i], ux, uy) - reach(z.position, boxes[j], -ux, -uy);
        assert!(
            available >= doc.drawing_style.bond_length_world * 0.39,
            "bond {}-{} has only {available} world units of clear ink",
            b.a,
            b.b
        );
    }
}

pub(super) fn same_except_positions(mut actual: Document, expected: &Document) {
    for (a, b) in actual.atoms.iter_mut().zip(&expected.atoms) {
        assert_eq!(a.id, b.id);
        a.position = b.position;
    }
    assert_eq!(&actual, expected);
}

pub(super) fn raw(source: &Document, count: u32) -> Document {
    let mut seed = source.clone();
    seed.generic = Default::default();
    let mut result = if let Some(link) = source.generic.links.first() {
        generic::link(&seed, link, count).unwrap()
    } else {
        generic::repeat(&seed, &source.generic.repeats[0], count).unwrap()
    };
    crate::atom_labels::clear_computed(&mut result);
    result
}

// Compatibility proposals below require the composed import/drawing/names/maps
// model. These are private authored tests; no execution is claimed.
pub(super) fn union_source() -> Document {
    use crate::{
        arrow_anchors::{Anchor, Target},
        atom_labels::{MappingDisplay, Number},
        scientific::{AtomMark, MarkKind},
    };
    let mut source = atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
    for (id, map) in [(1, 17), (3, 23)] {
        let atom = source.atom_mut(id).unwrap();
        atom.map_num = map;
        atom.display.mapping = MappingDisplay {
            show: Some(true),
            offset: (id == 1).then_some(Point::new(-110., -120.)),
            style: crate::typography::TextStyle {
                size_pt: 32.,
                italic: true,
                underline: true,
                family: "Times New Roman".into(),
                ..Default::default()
            },
        };
    }
    source.atom_mut(2).unwrap().mark_serial = 11;
    source.atom_mut(3).unwrap().display.number = Some(Number {
        text: "tail".into(),
        offset: Some(Point::new(0., 130.)),
        style: crate::typography::TextStyle {
            size_pt: 24.,
            italic: true,
            ..Default::default()
        },
    });
    let fixed = source.add_atom("O", Point::new(-250., 80.));
    let atom = source.atom_mut(fixed).unwrap();
    atom.mark_serial = 11;
    atom.marks.push(AtomMark {
        id: Some(7),
        kind: MarkKind::LonePair,
        offset: Point::new(-16., -16.),
        angle: 0.,
        size_pt: Some(10.),
    });
    source
        .depth_appearance
        .push(crate::depth_appearance::Scope {
            atoms: vec![fixed],
            automatic: false,
            strength: 0.62,
            rear_opacity: 0.3,
            weights: [(fixed, 0.7)].into(),
            overrides: [(fixed, 0.2)].into(),
            rear_weights: [(fixed, 1.)].into(),
            rear_overrides: [(fixed, 0.4)].into(),
        });
    let annotation = source.next_id();
    source.annotations.push(crate::document::Annotation {
        id: annotation,
        position: Point::new(-380., 180.),
        text: "oxidane".into(),
        format: crate::typography::TextFormat {
            width_pt: Some(55.),
            style: crate::typography::TextStyle {
                size_pt: 30.,
                italic: true,
                underline: true,
                family: "Times New Roman".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    });
    source
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation,
            atoms: vec![fixed],
            smiles: "O".into(),
            text: "oxidane".into(),
        });
    let target = Target::LonePair {
        atom: fixed,
        mark: 7,
    };
    let center = target.center(&source).unwrap();
    source.arrows.push(crate::document::Arrow {
        id: source.next_id(),
        start: center.offset(-100., 0.),
        end: Point::new(-600., 80.),
        start_anchor: Some(Anchor {
            target,
            offset: Point::new(-100., 0.),
            direction: Point::new(-1., 0.),
            gap: 4.,
        }),
        end_anchor: None,
        kind: "equilibrium".into(),
        control: None,
        cubic: Some([Point::new(-500., -190.), Point::new(-500., 300.)]),
        style: None,
    });
    source.validate().unwrap();
    source
}

pub(super) fn contained((lo, hi): (Point, Point), (a, z): (Point, Point)) {
    assert!(
        a.x >= lo.x - 0.01 && a.y >= lo.y - 0.01 && z.x <= hi.x + 0.01 && z.y <= hi.y + 0.01,
        "actual ink {a:?}..{z:?} lies outside {lo:?}..{hi:?}"
    );
}

pub(super) fn union_actual_ink_contained(doc: &Document, bounds: (Point, Point)) {
    let shown = preview(doc);
    for state in [doc, &shown] {
        for atom in &state.atoms {
            for ink in crate::scene::atom_label_ink_boxes(atom, state) {
                contained(bounds, ink);
            }
        }
    }
    for indicator in crate::atom_labels::indicators(&shown) {
        for (lo, hi) in
            crate::style::text_ink_boxes(&indicator.text, indicator.height, &indicator.style)
        {
            contained(
                bounds,
                (
                    indicator.origin.offset(lo.x, lo.y),
                    indicator.origin.offset(hi.x, hi.y),
                ),
            );
        }
    }
    for a in &doc.annotations {
        for f in crate::typography::layout(&a.text, &a.format).fragments {
            let origin = a.position.offset(f.position.x, f.position.y);
            for (lo, hi) in crate::style::text_ink_boxes(&f.text, f.style.size(), &f.style) {
                contained(
                    bounds,
                    (origin.offset(lo.x, lo.y), origin.offset(hi.x, hi.y)),
                );
            }
        }
    }
    for arrow in &doc.arrows {
        for part in arrow.paths() {
            let half = part.style.width() / 2.;
            for point in part
                .commands
                .iter()
                .flat_map(crate::graphics::PathCommand::iter_points)
            {
                contained(
                    bounds,
                    (point.offset(-half, -half), point.offset(half, half)),
                );
            }
        }
    }
}
