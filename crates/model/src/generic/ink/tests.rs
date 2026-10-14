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
