use super::*;
use crate::generic::drawing_test_support::{chain, preview, raw, same_except_positions};
use crate::{
    atom_labels::test_support::with_fonts,
    atom_text::{self, Mode},
    generic::{self, Limits},
};

#[test]
fn dl_descriptors_fit_the_generic_ink_envelope() {
    for roman in [false, true] {
        for inverted in [false, true] {
            let mut doc =
                crate::fischer::document(crate::fischer::Compound::LAlanine, 42.).unwrap();
            let center = crate::fischer::recognize(&doc).crosses[0].center;
            if inverted {
                let stereo = doc.atom_mut(center).unwrap().stereo.as_mut().unwrap();
                stereo.winding = if stereo.winding == "ccw" { "cw" } else { "ccw" }.into();
            }
            doc.atom_labels.stereo = true;
            doc.atom_labels.dl_descriptors = true;
            doc.atom_labels.roman_stereo_parentheses = roman;
            let refresh =
                crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default()).unwrap();
            assert!(refresh.notice.is_none());
            refresh.apply(&mut doc);
            let before = doc.clone();
            let (lo, hi) = atom_bounds(doc.atom(center).unwrap(), &doc)
                .unwrap()
                .unwrap();
            let indicator = crate::atom_labels::indicators(&doc)
                .into_iter()
                .find(|i| i.owner == crate::atom_labels::Owner::AtomStereo(center))
                .unwrap();
            assert_eq!(indicator.text, if inverted { "D (R)" } else { "L (S)" });
            assert!(indicator.origin.x >= lo.x && indicator.origin.y >= lo.y);
            assert!(indicator.origin.x + indicator.width <= hi.x);
            assert!(indicator.origin.y + indicator.height <= hi.y);
            assert_eq!(doc, before);
        }
    }
}

#[test]
fn optional_unknown_indicators_fit_the_generic_ink_envelope() {
    with_fonts(optional_unknown_ink_envelope);
}

fn optional_unknown_ink_envelope() {
    for xi in [false, true] {
        for pseudo in [false, true] {
            for roman in [false, true] {
                let mut doc = chain(&["C", "C"]);
                doc.atoms[0].display.stereo.style.family = "Liberation Sans".into();
                doc.bonds[0].indicator.style.family = "Liberation Sans".into();
                doc.atom_labels.stereo = true;
                doc.atom_labels.mark_undefined_stereocentres = true;
                doc.atom_labels.undefined_xi = xi;
                doc.atom_labels.roman_stereo_parentheses = roman;
                doc.atoms[0].display.stereo.offset = Some(Point::new(-70., -60.));
                doc.bonds[0].indicator.offset = Some(Point::new(60., 80.));
                // Bound the ink before the optional refresh supplies its labels.
                let atom = atom_bounds(&doc.atoms[0], &doc).unwrap().unwrap();
                let bond = object_bounds(&doc).unwrap().unwrap();
                doc.atoms[0].undefined_stereo = true;
                doc.atoms[0].undefined_pseudoasymmetric = pseudo;
                doc.bonds[0].undefined_stereo = true;
                for indicator in crate::atom_labels::indicators(&doc) {
                    let (lo, hi, code) = match indicator.owner {
                        crate::atom_labels::Owner::AtomStereo(_) => (
                            atom.0,
                            atom.1,
                            if xi && pseudo {
                                "ξ"
                            } else if xi {
                                "Ξ"
                            } else {
                                "?"
                            },
                        ),
                        crate::atom_labels::Owner::BondStereo(_, _) => {
                            (bond.0, bond.1, if xi { "Ξ" } else { "?" })
                        }
                        _ => continue,
                    };
                    assert_eq!(indicator.text, format!("({code})"));
                    assert!(indicator.origin.x >= lo.x && indicator.origin.y >= lo.y);
                    assert!(indicator.origin.x + indicator.width <= hi.x);
                    assert!(indicator.origin.y + indicator.height <= hi.y);
                }
            }
        }
    }
}

#[test]
fn stereo_descriptors_future_bounds_cover_displayed_sequence_and_axial_ink() {
    with_fonts(|| {
        for axial in ["helicity", "ra"] {
            let mut doc = Document::default();
            doc.drawing_style.font_family = "Liberation Sans".into();
            doc.atom_labels = serde_json::from_value(serde_json::json!({
                "stereo": true,
                "axial_descriptors": axial
            }))
            .unwrap();
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(42., 0.));
            doc.add_bond(a, b, 2, "plain");
            doc.bonds[0].indicator.offset = Some(Point::new(1000., 1000.));
            doc.bonds[0].indicator.style.family = "Liberation Sans".into();
            let before = doc.clone();
            let (lo, hi) = object_bounds(&doc).unwrap().unwrap();
            for text in [
                "(seqTrans)",
                "(seqCis)",
                if axial == "ra" { "(Ra)" } else { "(M)" },
                if axial == "ra" { "(sa)" } else { "(p)" },
            ] {
                let style = &doc.bonds[0].indicator.style;
                let height = crate::style::DEFAULT.world(style.size_pt);
                let width = crate::style::styled_text_width(text, height, style);
                let center = Point::new(1021., 1000.);
                let origin = center.offset(-width / 2., -height / 2.);
                let ink = crate::style::text_ink_boxes(text, height, style);
                assert!(!ink.is_empty());
                for (ink_lo, ink_hi) in ink {
                    let ink_lo = origin.offset(ink_lo.x, ink_lo.y);
                    let ink_hi = origin.offset(ink_hi.x, ink_hi.y);
                    assert!(
                        lo.x <= ink_lo.x
                            && lo.y <= ink_lo.y
                            && ink_hi.x <= hi.x
                            && ink_hi.y <= hi.y,
                        "{axial} {text}: {ink_lo:?}..{ink_hi:?} outside {lo:?}..{hi:?}"
                    );
                }
            }
            assert_eq!(doc, before);
        }
    });
}

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

#[test]
fn steroid_alpha_beta_indicators_and_future_ink_fit_generic_bounds() {
    with_fonts(|| {
        for roman in [false, true] {
            let mut doc = Document::default();
            doc.drawing_style.font_family = "Liberation Sans".into();
            doc.atom_labels.stereo = true;
            doc.atom_labels.steroid_alpha_beta = true;
            doc.atom_labels.roman_stereo_parentheses = roman;
            let id = doc.add_atom("C", Point::default());
            let atom = doc.atom_mut(id).unwrap();
            atom.display.stereo.style.family = "Liberation Sans".into();
            atom.display.stereo.style.size_pt = 36.;
            atom.display.stereo.offset = Some(Point::new(-80., 70.));
            // Pay and measure the envelope before background perception adds
            // a locant label, as generic expansion does for fixed components.
            let future = atom_bounds(doc.atom(id).unwrap(), &doc).unwrap().unwrap();
            let atom = doc.atom_mut(id).unwrap();
            atom.cip_label = Some("R".into());
            atom.steroid_label = Some("17β".into());
            let actual = atom_bounds(doc.atom(id).unwrap(), &doc).unwrap().unwrap();
            let [indicator] = crate::atom_labels::indicators(&doc).try_into().unwrap();
            assert_eq!(indicator.text, "17β");
            assert!(indicator.style.italic);
            assert!(
                crate::atom_labels::indicator_format(&indicator.text, &indicator.style, roman)
                    .spans
                    .is_empty()
            );
            let ink = crate::scene::label_ink_boxes(&indicator.primitives());
            assert!(!ink.is_empty());
            for bounds in ink {
                crate::generic::drawing_test_support::contained(actual, bounds);
                crate::generic::drawing_test_support::contained(future, bounds);
            }
        }
    });
}
