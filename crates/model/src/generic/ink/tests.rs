use super::*;
use crate::generic::drawing_test_support::{chain, preview, raw, same_except_positions};
use crate::{
    atom_text::{self, Mode},
    generic::{self, Limits},
};

#[test]
fn generated_paint_bounds_contain_extreme_highlights_and_joined_bonds() {
    let paint = crate::palette::Color::Custom([240, 210, 30]);
    for (length, spacing, bold) in [(100., 0.4, 2.), (5., 0.05, 12.)] {
        let mut source =
            atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
        source.drawing_style.set_bond_length(length);
        source.drawing_style.bond_spacing_ratio = spacing;
        source.drawing_style.line_width_pt = 0.1;
        source.drawing_style.bold_width_pt = bold;
        source.atom_mut(2).unwrap().display.carbons = Some(crate::atom_labels::Carbons::All);
        source.atom_mut(2).unwrap().text_style = Some(crate::typography::TextStyle {
            size_pt: 144.,
            ..Default::default()
        });
        // A fixed, valid tert-butyl graph with acute projected bold junctions
        // exercises the corner clamp without moving arbitrary manual geometry.
        let center = source.add_atom("C", Point::new(-900., 0.));
        for (i, p) in [
            Point::new(-610., 0.),
            Point::new(-615., 60.),
            Point::new(-615., -60.),
        ]
        .into_iter()
        .enumerate()
        {
            let leaf = source.add_atom("C", p);
            source.add_bond(center, leaf, 1, "plain");
            let b = source.bonds.last_mut().unwrap();
            b.display = "bold".into();
            b.projection = true;
            b.color = crate::palette::Color::Custom([60 + i as u8 * 60, 30, 90]);
        }
        for a in &mut source.atoms {
            a.display.highlight = Some(paint);
        }
        for b in &mut source.bonds {
            b.highlight = Some(paint);
        }
        source.validate().unwrap();
        let before = source.clone();
        for (i, product) in generic::expand_drawings(&source, Limits::default())
            .unwrap()
            .iter()
            .enumerate()
        {
            let shown = preview(&product.document);
            let joins = crate::bond_joins::Joins::new(&shown);
            let (lo, hi) = product.bounds;
            let contains = |p: Point| {
                assert!(
                    p.x >= lo.x - 0.01
                        && p.y >= lo.y - 0.01
                        && p.x <= hi.x + 0.01
                        && p.y <= hi.y + 0.01,
                    "paint {p:?} lies outside returned bounds {lo:?}..{hi:?} for {length}/{spacing}/{bold}"
                );
            };
            for a in &shown.atoms {
                if let Some((a, z)) = crate::highlights::atom_bounds(&shown, a) {
                    contains(a);
                    contains(z);
                }
            }
            for b in &shown.bonds {
                if let Some((a, z)) = crate::highlights::bond_bounds(&shown, b, &joins) {
                    contains(a);
                    contains(z);
                }
                if joins.needed(b) {
                    for p in joins.polygon(
                        b,
                        shown.atom(b.a).unwrap().position,
                        shown.atom(b.b).unwrap().position,
                    ) {
                        contains(p);
                    }
                }
            }
            let junctions = joins.junctions();
            assert!(!junctions.is_empty());
            for p in junctions
                .iter()
                .flat_map(|j| &j.parts)
                .flat_map(|(_, points)| points)
            {
                contains(*p);
            }
            same_except_positions(product.document.clone(), &raw(&source, i as u32 + 1));
            for a in before.atoms.iter().filter(|a| a.id >= center) {
                assert_eq!(product.document.atom(a.id).unwrap(), a);
            }
        }
        assert_eq!(source, before);
    }
}

#[test]
fn atom_bounds_cover_long_automatic_numbers_on_either_side() {
    for open_x in [1., -1.] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        for y in [-15., 15.] {
            let b = doc.add_atom("C", Point::new(-26. * open_x, y));
            doc.add_bond(a, b, 1, "plain");
        }
        doc.atom_mut(a).unwrap().display.number = Some(crate::atom_labels::Number {
            text: "attached caption text".into(),
            offset: None,
            style: crate::atom_labels::number_style(),
        });
        let (lo, hi) = atom_bounds(doc.atom(a).unwrap(), &doc).unwrap().unwrap();
        let [label] = crate::atom_labels::indicators(&doc).try_into().unwrap();
        for (ink_lo, ink_hi) in
            crate::style::text_ink_boxes(&label.text, label.height, &label.style)
        {
            let ink_lo = label.origin.offset(ink_lo.x, ink_lo.y);
            let ink_hi = label.origin.offset(ink_hi.x, ink_hi.y);
            assert!(
                lo.x <= ink_lo.x && lo.y <= ink_lo.y && ink_hi.x <= hi.x && ink_hi.y <= hi.y,
                "{open_x}: {ink_lo:?}..{ink_hi:?} outside {lo:?}..{hi:?}"
            );
        }
    }
}

#[test]
fn cip_bounds_cover_roman_parentheses_on_atom_and_bond_indicators() {
    use crate::atom_labels::Owner;
    for roman in [false, true] {
        for offset in [None, Some(Point::new(-30., 40.))] {
            let mut doc = Document::default();
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(42., 0.));
            doc.add_bond(a, b, 2, "plain");
            doc.atom_labels.stereo = true;
            doc.atom_labels.roman_stereo_parentheses = roman;
            let atom = doc.atom_mut(a).unwrap();
            atom.cip_label = Some("R".into());
            atom.display.stereo.offset = offset;
            atom.display.stereo.style.size_pt = 36.;
            doc.bonds[0].cip_label = Some("E".into());
            doc.bonds[0].indicator.offset = offset;
            doc.bonds[0].indicator.style.size_pt = 36.;
            for indicator in crate::atom_labels::indicators(&doc) {
                let bounds = match indicator.owner {
                    Owner::AtomStereo(id) => atom_bounds(doc.atom(id).unwrap(), &doc).unwrap(),
                    Owner::BondStereo(..) => object_bounds(&doc).unwrap(),
                    _ => panic!("unexpected indicator"),
                }
                .unwrap();
                for ink in crate::scene::label_ink_boxes(&indicator.primitives()) {
                    crate::generic::drawing_test_support::contained(bounds, ink);
                }
            }
        }
    }
}
