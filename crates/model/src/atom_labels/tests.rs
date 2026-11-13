use super::test_support::with_fonts;
use super::*;

fn stereo_document() -> Document {
    let mut doc = Document::default();
    doc.drawing_style.font_family = "Liberation Sans".into();
    doc.atom_labels.stereo = true;
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 2, "plain");
    doc.bonds[0].indicator.style.family = "Liberation Sans".into();
    doc
}

#[test]
fn stereo_descriptors_use_display_names_without_changing_cached_codes_or_styles() {
    with_fonts(|| {
        let mut doc = stereo_document();
        for (code, display) in [
            ("E", "(E)"),
            ("Z", "(Z)"),
            ("e", "(seqTrans)"),
            ("z", "(seqCis)"),
            ("M", "(M)"),
            ("P", "(P)"),
            ("m", "(m)"),
            ("p", "(p)"),
        ] {
            doc.bonds[0].cip_label = Some(code.into());
            let before = doc.clone();
            let [indicator] = indicators(&doc).try_into().unwrap();
            assert_eq!(indicator.text, display, "{code}");
            assert!(indicator.style.italic);
            assert_eq!(indicator.style, doc.bonds[0].indicator.style);
            assert_eq!(doc, before);
        }
    });
}

#[test]
fn stereo_descriptors_axial_choice_changes_only_displayed_helicity() {
    with_fonts(|| {
        let mut doc = stereo_document();
        doc.atom_labels = serde_json::from_value(serde_json::json!({
            "stereo": true,
            "axial_descriptors": "ra"
        }))
        .unwrap();
        for (code, display) in [
            ("M", "(Ra)"),
            ("P", "(Sa)"),
            ("m", "(ra)"),
            ("p", "(sa)"),
            ("e", "(seqTrans)"),
            ("z", "(seqCis)"),
        ] {
            doc.bonds[0].cip_label = Some(code.into());
            let [indicator] = indicators(&doc).try_into().unwrap();
            assert_eq!(indicator.text, display, "{code}");
            assert_eq!(doc.bonds[0].cip_label.as_deref(), Some(code));
            assert!(indicator.style.italic);
        }
    });
}

#[test]
fn stereo_descriptors_validate_native_bond_codes_without_accepting_display_strings() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 2, "plain");
    for code in ["E", "Z", "e", "z", "M", "P", "m", "p"] {
        doc.bonds[0].cip_label = Some(code.into());
        assert_eq!(doc.validate(), Ok(()), "{code}");
    }
    for display in ["seqTrans", "seqCis", "Ra", "Sa", "ra", "sa", "R"] {
        doc.bonds[0].cip_label = Some(display.into());
        assert_eq!(doc.validate().unwrap_err(), "Unsupported bond CIP label");
    }
}

#[test]
fn stereo_descriptors_serde_preserves_old_defaults_and_saves_axial_choice() {
    let old = serde_json::json!({"carbons": "skeletal", "hydrogens": true, "stereo": false});
    let settings: Settings = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.axial_descriptors, AxialDescriptors::Helicity);
    assert_eq!(
        AxialDescriptors::Helicity.to_string(),
        "M/P (IUPAC preferred)"
    );
    assert_eq!(AxialDescriptors::Ra.to_string(), "Ra/Sa");
    assert_eq!(serde_json::to_value(settings).unwrap(), old);
    let chosen = serde_json::json!({
        "carbons": "skeletal", "hydrogens": true, "stereo": false,
        "axial_descriptors": "ra"
    });
    let settings: Settings = serde_json::from_value(chosen.clone()).unwrap();
    assert_eq!(serde_json::to_value(settings).unwrap(), chosen);
}

#[test]
fn roman_stereo_parentheses_are_opt_in_for_saved_documents() {
    let document = Document::default();
    let saved = serde_json::to_value(&document).unwrap();
    assert!(
        saved["atom_labels"]
            .get("roman_stereo_parentheses")
            .is_none()
    );
    let loaded: Document = serde_json::from_value(saved).unwrap();
    assert!(!loaded.atom_labels.roman_stereo_parentheses);
    assert_eq!(loaded, document);

    let mut document = document;
    document.atom_labels.roman_stereo_parentheses = true;
    let saved = serde_json::to_value(&document).unwrap();
    assert_eq!(saved["atom_labels"]["roman_stereo_parentheses"], true);
    let loaded: Document = serde_json::from_value(saved).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn roman_stereo_parentheses_use_three_runs_for_atom_and_bond_indicators() {
    use crate::scene::Primitive;
    for (atom_cip, bond_cip) in [("R", "E"), ("S", "Z"), ("r", "E"), ("s", "Z")] {
        for roman in [false, true] {
            let mut doc = Document::default();
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(42., 0.));
            doc.add_bond(a, b, 2, "plain");
            doc.atom_mut(a).unwrap().cip_label = Some(atom_cip.into());
            doc.bonds[0].cip_label = Some(bond_cip.into());
            doc.atom_labels.stereo = true;
            doc.atom_labels.roman_stereo_parentheses = roman;

            let indicators = indicators(&doc);
            assert_eq!(indicators.len(), 2);
            let mut expected = Vec::new();
            for (indicator, cip) in indicators.iter().zip([atom_cip, bond_cip]) {
                let runs = if roman {
                    vec![
                        ("(".to_owned(), false),
                        (cip.into(), true),
                        (")".into(), false),
                    ]
                } else {
                    vec![(format!("({cip})"), true)]
                };
                expected.extend(runs.clone());
                let primitives = indicator.primitives();
                let mut right = indicator.origin.x;
                for (primitive, (expected_text, italic)) in primitives.iter().zip(&runs) {
                    let Primitive::Text {
                        position,
                        text,
                        style,
                        size,
                        ..
                    } = primitive
                    else {
                        panic!("indicator run is not text");
                    };
                    assert_eq!(text, expected_text);
                    assert_eq!(style.italic, *italic);
                    assert!((position.x - right).abs() < 0.001);
                    assert_eq!(position.y, indicator.origin.y);
                    right += crate::style::styled_text_width(text, *size, style);
                }
                assert_eq!(primitives.len(), runs.len());
                assert!((right - indicator.origin.x - indicator.width).abs() < 0.001);
            }
            let rendered: Vec<_> = crate::scene::primitives(&doc)
                .into_iter()
                .filter_map(|primitive| match primitive {
                    Primitive::Text { text, style, .. } => Some((text, style.italic)),
                    _ => None,
                })
                .collect();
            assert_eq!(rendered, expected);
            let svg = crate::scene::svg(&doc);
            let tree = roxmltree::Document::parse(&svg).unwrap();
            let exported: Vec<_> = tree
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .map(|node| {
                    (
                        node.text().unwrap().to_owned(),
                        node.attribute("font-style") == Some("italic"),
                    )
                })
                .collect();
            assert_eq!(exported, expected);
        }
    }
}

#[test]
fn roman_stereo_parentheses_preserve_number_and_map_styles() {
    let mut doc = Document::default();
    let id = doc.add_atom("C", Point::default());
    let atom = doc.atom_mut(id).unwrap();
    atom.map_num = 3;
    atom.display.number = Some(Number {
        text: "(R)".into(),
        offset: None,
        style: stereo_style(),
    });
    doc.atom_labels.roman_stereo_parentheses = true;
    for indicator in indicators(&doc) {
        assert!(!indicator.roman_parentheses);
        let primitives = indicator.primitives();
        let [crate::scene::Primitive::Text { style, .. }] = primitives.as_slice() else {
            panic!("number or map was split into styled runs");
        };
        assert_eq!(style, &indicator.style);
    }
}

#[test]
fn undefined_marker_setting_defaults_off_and_round_trips_only_when_enabled() {
    let legacy: Settings = serde_json::from_value(serde_json::json!({ "stereo": true })).unwrap();
    assert!(!legacy.mark_undefined_stereocentres);
    assert!(!legacy.undefined_xi);
    let saved = serde_json::to_value(&legacy).unwrap();
    assert!(saved.get("mark_undefined_stereocentres").is_none());
    assert!(saved.get("undefined_xi").is_none());
    let enabled = Settings {
        mark_undefined_stereocentres: true,
        undefined_xi: true,
        ..legacy
    };
    let saved = serde_json::to_value(&enabled).unwrap();
    assert_eq!(saved["mark_undefined_stereocentres"], true);
    assert_eq!(saved["undefined_xi"], true);
    assert_eq!(serde_json::from_value::<Settings>(saved).unwrap(), enabled);
}

#[test]
fn existing_native_document_round_trips_without_marker_fields() {
    let document = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap()
    .current();
    assert!(!document.atom_labels.mark_undefined_stereocentres);
    assert!(!document.atom_labels.undefined_xi);
    let saved = document.file_json().unwrap();
    let text = std::str::from_utf8(&saved).unwrap();
    assert!(!text.contains("mark_undefined_stereocentres"));
    assert!(!text.contains("undefined_xi"));
    assert!(!text.contains("undefined_stereo"));
    let reopened = Document::from_json(&saved).unwrap();
    assert_eq!(reopened, document);
    assert_eq!(reopened.file_json().unwrap(), saved);
}

#[test]
fn undefined_markers_follow_stereo_visibility_and_keep_cip_labels_separate() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 2, "plain");
    doc.atoms[0].undefined_stereo = true;
    doc.atoms[0].display.stereo.offset = Some(Point::new(-10., -20.));
    doc.bonds[0].undefined_stereo = true;
    doc.bonds[0].indicator.offset = Some(Point::new(0., 20.));
    for stereo in [false, true] {
        for mark in [false, true] {
            for show in [None, Some(false), Some(true)] {
                doc.atom_labels.stereo = stereo;
                doc.atom_labels.mark_undefined_stereocentres = mark;
                doc.atoms[0].display.stereo.show = show;
                doc.bonds[0].indicator.show = show;
                let labels = indicators(&doc);
                assert_eq!(
                    labels.len(),
                    if stereo && mark && show != Some(false) {
                        2
                    } else {
                        0
                    }
                );
                assert!(
                    labels.iter().all(|i| i.text == "(?)"
                        && i.style.color == crate::palette::Color::Custom([128; 3]))
                );
                assert_eq!(
                    serde_json::to_value(&labels).unwrap(),
                    serde_json::to_value(layout_indicators(&doc, &doc.abbreviation_render_copy()))
                        .unwrap()
                );
                assert!(doc.atoms[0].cip_label.is_none() && doc.bonds[0].cip_label.is_none());
            }
        }
    }
    let labels = indicators(&doc);
    assert_eq!(labels[0].owner, Owner::AtomStereo(a));
    assert_eq!(labels[0].center, Point::new(-10., -20.));
    assert_eq!(labels[1].owner, Owner::BondStereo(a, b));
    assert_eq!(labels[1].center, Point::new(21., 20.));

    doc.atoms[0].cip_label = Some("R".into());
    doc.bonds[0].cip_label = Some("E".into());
    let labels = indicators(&doc);
    assert_eq!(labels[0].text, "(R)");
    assert_eq!(labels[1].text, "(E)");
    assert_eq!(labels[0].style, doc.atoms[0].display.stereo.style);
    assert_eq!(labels[1].style, doc.bonds[0].indicator.style);
}

#[test]
fn computed_undefined_flags_are_not_persisted_and_clear_with_other_stereo_labels() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 2, "plain");
    doc.atom_labels.mark_undefined_stereocentres = true;
    let plain = doc.clone();
    doc.atoms[0].undefined_stereo = true;
    doc.atoms[0].undefined_pseudoasymmetric = true;
    doc.bonds[0].undefined_stereo = true;
    assert!(!crate::transaction::chemistry_changed(&plain, &doc));
    let json = serde_json::to_value(&doc).unwrap();
    assert!(json["atoms"][0].get("undefined_stereo").is_none());
    assert!(json["atoms"][0].get("undefined_pseudoasymmetric").is_none());
    assert!(json["bonds"][0].get("undefined_stereo").is_none());
    let opened: Document = serde_json::from_value(json).unwrap();
    assert!(!opened.atoms[0].undefined_stereo && !opened.bonds[0].undefined_stereo);
    assert!(!opened.atoms[0].undefined_pseudoasymmetric);
    assert!(opened.atom_labels.mark_undefined_stereocentres);
    assert!(!crate::transaction::chemistry_changed(&opened, &doc));
    clear_computed(&mut doc);
    assert!(!doc.atoms[0].undefined_stereo && !doc.bonds[0].undefined_stereo);
    assert!(!doc.atoms[0].undefined_pseudoasymmetric);
}

#[test]
fn xi_markers_share_muted_stereo_layout_visibility_and_italic_formatting() {
    with_fonts(xi_marker_formatting);
}

fn xi_marker_formatting() {
    let mut doc = stereo_document();
    doc.atoms[0].display.stereo.style.family = "Liberation Sans".into();
    let atom = doc.atoms[0].id;
    doc.atoms[0].undefined_stereo = true;
    doc.bonds[0].undefined_stereo = true;
    doc.atom_labels.mark_undefined_stereocentres = true;
    doc.atom_labels.undefined_xi = true;
    for pseudo in [false, true] {
        doc.atoms[0].undefined_pseudoasymmetric = pseudo;
        let atom_code = if pseudo { "ξ" } else { "Ξ" };
        for roman in [false, true] {
            doc.atom_labels.roman_stereo_parentheses = roman;
            let labels = indicators(&doc);
            assert_eq!(labels.len(), 2);
            for (indicator, code) in labels.iter().zip([atom_code, "Ξ"]) {
                assert_eq!(indicator.text, format!("({code})"));
                assert_eq!(
                    indicator.style.color,
                    crate::palette::Color::Custom([128; 3])
                );
                let format = owned_indicator_format(
                    indicator.owner,
                    &indicator.text,
                    &indicator.style,
                    roman,
                );
                let runs: Vec<_> = indicator_runs(&indicator.text, &format)
                    .map(|(text, style)| (text, style.italic, style.color))
                    .collect();
                let muted = crate::palette::Color::Custom([128; 3]);
                let expected = if roman {
                    vec![
                        ("(", false, muted),
                        (code, true, muted),
                        (")", false, muted),
                    ]
                } else {
                    vec![(indicator.text.as_str(), true, muted)]
                };
                assert_eq!(runs, expected);
            }
        }
        for stereo in [false, true] {
            for mark in [false, true] {
                for show in [None, Some(false), Some(true)] {
                    doc.atom_labels.stereo = stereo;
                    doc.atom_labels.mark_undefined_stereocentres = mark;
                    doc.atoms[0].display.stereo.show = show;
                    doc.bonds[0].indicator.show = show;
                    assert_eq!(
                        indicators(&doc).len(),
                        if stereo && mark && show != Some(false) {
                            2
                        } else {
                            0
                        }
                    );
                }
            }
        }
        assert_eq!(
            atom_stereo_code(doc.atom(atom).unwrap(), &doc),
            Some(atom_code)
        );
    }
    assert!(doc.atoms[0].cip_label.is_none() && doc.bonds[0].cip_label.is_none());
}

#[test]
fn isolated_hydrogen_halides_read_hydrogen_first_and_respect_overrides() {
    for element in ["F", "Cl", "Br", "I"] {
        let mut doc = Document::default();
        let id = doc.add_atom(element, Point::default());
        doc.atom_mut(id).unwrap().label_h = 1;
        assert_eq!(
            appendage_position(doc.atom(id).unwrap(), &doc),
            HydrogenPosition::Left
        );
        let mut text: Vec<_> = crate::scene::primitives(&doc)
            .into_iter()
            .filter_map(|p| match p {
                crate::scene::Primitive::Text { position, text, .. } => Some((position.x, text)),
                _ => None,
            })
            .collect();
        text.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(
            text.into_iter().map(|(_, text)| text).collect::<String>(),
            format!("H{element}")
        );

        doc.atom_mut(id).unwrap().display.hydrogen_position = HydrogenPosition::Right;
        assert_eq!(
            appendage_position(doc.atom(id).unwrap(), &doc),
            HydrogenPosition::Right
        );
        doc.atom_mut(id).unwrap().display.hydrogen_position = HydrogenPosition::Auto;
        doc.atom_mut(id).unwrap().label_h = 2;
        assert_eq!(
            appendage_position(doc.atom(id).unwrap(), &doc),
            HydrogenPosition::Right
        );
        doc.atom_mut(id).unwrap().label_h = 1;
        let carbon = doc.add_atom("C", Point::new(-42., 0.));
        doc.add_bond(id, carbon, 1, "plain");
        assert_eq!(
            appendage_position(doc.atom(id).unwrap(), &doc),
            HydrogenPosition::Right
        );
    }
}

/// A mapped atom at the origin with plain bonds to carbons at `neighbors`.
fn mapped(element: &str, neighbors: &[Point]) -> (Document, Indicator) {
    let mut doc = Document::default();
    doc.atom_labels.maps = true;
    let atom = doc.add_atom(element, Point::new(0., 0.));
    for &p in neighbors {
        let c = doc.add_atom("C", p);
        doc.add_bond(atom, c, 1, "plain");
    }
    doc.atom_mut(atom).unwrap().map_num = 3;
    let map = indicators(&doc)
        .into_iter()
        .find(|i| i.owner == Owner::Mapping(atom))
        .unwrap();
    (doc, map)
}

/// Issue #314: a map number beside its element label reads as part of it
/// (O 3 → O₃), whichever way the bond points after Align rotates a product.
#[test]
fn map_numbers_stack_on_their_element_label() {
    for element in ["O", "I"] {
        for step in 0..48 {
            let angle = (step as f32 * 7.5).to_radians();
            let (doc, map) = mapped(element, &[Point::new(42. * angle.cos(), 42. * angle.sin())]);
            let (lo, hi) = crate::scene::atom_label_ink_bounds(&doc.atoms[0], &doc).unwrap();
            let (left, right) = (map.origin.x, map.origin.x + map.width);
            let (top, bottom) = (map.origin.y, map.origin.y + map.height);
            assert!(
                (bottom <= lo.y || top >= hi.y) && left < hi.x && right > lo.x,
                "{element} bond at {}°: map {:?} not stacked on label {lo:?}..{hi:?}",
                step as f32 * 7.5,
                map.center,
            );
        }
    }
}

#[test]
fn stacking_never_moves_a_map_number_nearer_another_atom() {
    let carbons = [Point::new(0., -42.), Point::new(0., 42.)];
    let (_, map) = mapped("O", &carbons);
    let distance = |p: Point| (p.x - map.center.x).hypot(p.y - map.center.y);
    let own = distance(Point::new(0., 0.));
    assert!(
        carbons.iter().all(|&c| distance(c) > own),
        "map {:?} reads as a carbon's",
        map.center
    );
}

/// A fully mapped chain; the first bond has `first_order`.
fn mapped_chain(atoms: &[(&str, f32, f32)], first_order: u8) -> Document {
    let mut doc = Document::default();
    doc.atom_labels.maps = true;
    let ids: Vec<_> = atoms
        .iter()
        .map(|&(e, x, y)| doc.add_atom(e, Point::new(x, y)))
        .collect();
    for (n, pair) in ids.windows(2).enumerate() {
        doc.add_bond(
            pair[0],
            pair[1],
            if n == 0 { first_order } else { 1 },
            "plain",
        );
    }
    for (n, id) in ids.iter().enumerate() {
        doc.atom_mut(*id).unwrap().map_num = n as u32 + 1;
    }
    doc
}

fn assert_maps_nearest_their_atoms(doc: &Document) {
    for map in indicators(doc) {
        let Owner::Mapping(owner) = map.owner else {
            continue;
        };
        let distance = |p: Point| (p.x - map.center.x).hypot(p.y - map.center.y);
        let own = distance(doc.atom(owner).unwrap().position);
        assert!(
            doc.atoms.iter().all(|a| distance(a.position) >= own),
            "map {} at {:?} is nearer another atom",
            map.text,
            map.center
        );
    }
}

#[test]
fn a_stacked_label_map_does_not_push_a_carbon_map_beside_the_label() {
    // Fully mapped O=C–C–C–C–C: map 6 must not end up beside the O.
    assert_maps_nearest_their_atoms(&mapped_chain(
        &[
            ("O", 0., 0.),
            ("C", 0., -42.),
            ("C", 36.373_07, -63.),
            ("C", 72.746_13, -42.),
            ("C", 72.746_13, 0.),
            ("C", 36.373_07, 21.),
        ],
        2,
    ));
}

#[test]
fn ownership_outranks_stacking_when_both_cannot_hold() {
    // The O's map cannot both stack on the label and stay nearest the O:
    // staying nearest wins.
    assert_maps_nearest_their_atoms(&mapped_chain(
        &[
            ("C", 0., 0.),
            ("C", 21., 36.373_067),
            ("O", 42., 72.746_13),
            ("C", 63., 109.119_2),
            ("C", 84., 72.746_13),
            ("C", 63., 36.373_067),
            ("C", 84., 0.),
        ],
        1,
    ));
}

#[test]
fn atoms_hidden_in_a_collapsed_group_do_not_claim_map_numbers() {
    let (mut doc, _) = mapped("O", &[Point::new(42., 0.)]);
    let anchor = doc.add_atom("C", Point::new(84., 0.));
    let mut members = vec![anchor];
    for y in [-60., 60.] {
        let near = doc.add_atom("C", Point::new(42., y));
        let far = doc.add_atom("C", Point::new(0., y));
        doc.add_bond(anchor, near, 1, "plain");
        doc.add_bond(near, far, 1, "plain");
        members.extend([near, far]);
    }
    doc.abbreviations.push(crate::abbreviations::Abbreviation {
        label_style: None,
        label_color_override: false,
        highlight: None,
        alignment: Default::default(),
        label: "R".into(),
        reverse_label: "R".into(),
        anchor,
        exit: None,
        members,
    });
    let oxygen = &doc.atoms[0];
    let map = indicators(&doc)
        .into_iter()
        .find(|i| i.owner == Owner::Mapping(oxygen.id))
        .unwrap();
    let (lo, hi) = crate::scene::atom_label_ink_bounds(oxygen, &doc).unwrap();
    let (top, bottom) = (map.origin.y, map.origin.y + map.height);
    assert!(
        bottom <= lo.y || top >= hi.y,
        "map {:?} not stacked on label {lo:?}..{hi:?}",
        map.center
    );
}

/// Issue #458: the Style bar alignment reads and sets each kind of label the
/// way ChemDraw names it, and Automatic returns a forced label to the bonds.
#[test]
fn label_alignment_round_trips_for_elements_and_free_text() {
    use crate::abbreviations::LabelAlignment as A;
    let mut doc = Document::default();
    let o = doc.add_atom("O", Point::new(0., 0.));
    let c = doc.add_atom("C", Point::new(20., 0.));
    doc.add_bond(o, c, 1, "plain");
    let text = doc.add_atom("*", Point::new(40., 0.));
    doc.atom_mut(text).unwrap().display.variable = Some("MeO".into());
    for id in [o, text] {
        assert_eq!(alignment(&doc, id), Some(A::Auto));
        for value in [A::Left, A::Right, A::Auto] {
            set_alignment(&mut doc, id, value);
            assert_eq!(alignment(&doc, id), Some(value));
        }
    }
    set_alignment(&mut doc, o, A::Left);
    assert_eq!(
        doc.atom(o).unwrap().display.hydrogen_position,
        HydrogenPosition::Right,
        "Flush left reads OH"
    );
    set_alignment(&mut doc, o, A::Center);
    assert_eq!(alignment(&doc, o), Some(A::Left), "no centered element");
    set_alignment(&mut doc, text, A::Center);
    set_alignment(&mut doc, text, A::Above);
    assert_eq!(alignment(&doc, text), Some(A::Center), "no stacked text");
}

#[test]
fn absent_indicator_fast_path_matches_full_layout_for_visibility_and_display_overrides() {
    for maps in [false, true] {
        for stereo in [false, true] {
            for override_show in [None, Some(false), Some(true)] {
                for has_cip in [false, true] {
                    for numbered in [false, true] {
                        let mut doc = Document::default();
                        let a = doc.add_atom("N", Point::default());
                        let b = doc.add_atom("O", Point::new(42., 0.));
                        doc.add_bond(a, b, 2, "plain");
                        doc.atom_labels.maps = maps;
                        doc.atom_labels.stereo = stereo;
                        for atom in &mut doc.atoms {
                            atom.map_num = 12;
                            atom.cip_label = has_cip.then(|| "R".into());
                            atom.display.mapping.show = override_show;
                            atom.display.stereo.show = override_show;
                            atom.display.number = numbered.then(|| Number {
                                text: "12".into(),
                                offset: None,
                                style: number_style(),
                            });
                        }
                        doc.bonds[0].cip_label = has_cip.then(|| "E".into());
                        doc.bonds[0].indicator.show = override_show;
                        let original = doc.clone();
                        let compare = |doc: &Document| {
                            assert_eq!(
                                serde_json::to_value(indicators(doc)).unwrap(),
                                serde_json::to_value(layout_indicators(
                                    doc,
                                    &doc.abbreviation_render_copy()
                                ))
                                .unwrap()
                            );
                        };
                        compare(&doc);
                        doc.abbreviations.push(crate::abbreviations::Abbreviation {
                            label: "NO".into(),
                            reverse_label: String::new(),
                            anchor: a,
                            exit: None,
                            members: vec![a, b],
                            alignment: Default::default(),
                            label_style: None,
                            label_color_override: false,
                            highlight: None,
                        });
                        compare(&doc);
                        doc.abbreviations.clear();
                        assert_eq!(doc, original);
                    }
                }
            }
        }
    }
}

/// #766: grouped centres show their group and the molecule's other
/// stereocentres show `abs`; molecules without groups and the hidden setting
/// show nothing.
#[test]
fn stereo_group_labels_mark_grouped_and_absolute_centres() {
    use crate::stereo_groups::{Kind, StereoGroup};
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    let other = doc.add_atom("C", Point::new(300., 0.));
    doc.add_bond(a, b, 1, "plain");
    for id in [a, b, other] {
        doc.atom_mut(id).unwrap().cip_label = Some("S".into());
    }
    let labels = |doc: &Document| {
        indicators(doc)
            .into_iter()
            .filter_map(|i| match i.owner {
                Owner::StereoGroup(id) => Some((id, i.text)),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>()
    };
    assert!(labels(&doc).is_empty());
    doc.atom_mut(a).unwrap().stereo_group = Some(StereoGroup {
        kind: Kind::And,
        number: 1,
    });
    assert_eq!(
        labels(&doc),
        [(a, "and1".to_string()), (b, "abs".to_string())].into()
    );
    doc.atom_mut(b).unwrap().cip_label = None;
    assert_eq!(labels(&doc), [(a, "and1".to_string())].into());
    doc.atom_labels.stereo_groups = false;
    assert!(labels(&doc).is_empty());
}

/// #766: with two adjacent centres (`C[C@H](O)[C@H](F)Cl |a:1,&1:3|`), each
/// group label sits on its own centre's open side, not between the centres.
#[test]
fn stereo_group_labels_sit_on_their_own_centres_open_side() {
    use crate::stereo_groups::{Kind, StereoGroup};
    for stereo in [false, true] {
        let mut doc = Document::default();
        doc.atom_labels.stereo = stereo;
        let left = doc.add_atom("C", Point::new(0., 0.));
        let right = doc.add_atom("C", Point::new(42., 0.));
        let others = [
            (left, "Cl", Point::new(-21., -36.4)),
            (left, "F", Point::new(-21., 36.4)),
            (right, "C", Point::new(63., -36.4)),
            (right, "O", Point::new(63., 36.4)),
        ];
        doc.add_bond(left, right, 1, "plain");
        for (centre, element, position) in others {
            let id = doc.add_atom(element, position);
            doc.add_bond(centre, id, 1, "plain");
        }
        for id in [left, right] {
            doc.atom_mut(id).unwrap().cip_label = Some("S".into());
        }
        doc.atom_mut(left).unwrap().stereo_group = Some(StereoGroup {
            kind: Kind::And,
            number: 1,
        });
        let groups: Vec<_> = indicators(&doc)
            .into_iter()
            .filter_map(|i| match i.owner {
                Owner::StereoGroup(id) => Some((id, i.center)),
                _ => None,
            })
            .collect();
        assert_eq!(groups.len(), 2);
        for (id, center) in groups {
            let own = doc.atom(id).unwrap().position;
            let other = doc
                .atom(if id == left { right } else { left })
                .unwrap()
                .position;
            assert!(
                other.distance(center) > own.distance(center) * 1.3,
                "{stereo} {id}"
            );
            if !stereo {
                // The open side faces away from the other centre.
                assert!((center.x - own.x) * (own.x - other.x) > 0., "{id}");
            }
        }
    }
}
