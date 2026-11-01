use super::*;

#[test]
fn undefined_marker_setting_defaults_off_and_round_trips_only_when_enabled() {
    let legacy: Settings = serde_json::from_value(serde_json::json!({ "stereo": true })).unwrap();
    assert!(!legacy.mark_undefined_stereocentres);
    let saved = serde_json::to_value(&legacy).unwrap();
    assert!(saved.get("mark_undefined_stereocentres").is_none());
    let enabled = Settings {
        mark_undefined_stereocentres: true,
        ..legacy
    };
    let saved = serde_json::to_value(&enabled).unwrap();
    assert_eq!(saved["mark_undefined_stereocentres"], true);
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
    let saved = document.file_json().unwrap();
    let text = std::str::from_utf8(&saved).unwrap();
    assert!(!text.contains("mark_undefined_stereocentres"));
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
                    serde_json::to_value(layout_indicators(&doc)).unwrap()
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
    doc.bonds[0].undefined_stereo = true;
    assert!(!crate::transaction::chemistry_changed(&plain, &doc));
    let json = serde_json::to_value(&doc).unwrap();
    assert!(json["atoms"][0].get("undefined_stereo").is_none());
    assert!(json["bonds"][0].get("undefined_stereo").is_none());
    let opened: Document = serde_json::from_value(json).unwrap();
    assert!(!opened.atoms[0].undefined_stereo && !opened.bonds[0].undefined_stereo);
    assert!(opened.atom_labels.mark_undefined_stereocentres);
    assert!(!crate::transaction::chemistry_changed(&opened, &doc));
    clear_computed(&mut doc);
    assert!(!doc.atoms[0].undefined_stereo && !doc.bonds[0].undefined_stereo);
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
                                serde_json::to_value(layout_indicators(doc)).unwrap()
                            );
                        };
                        compare(&doc);
                        doc.abbreviations.push(crate::abbreviations::Abbreviation {
                            label: "NO".into(),
                            reverse_label: String::new(),
                            anchor: a,
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
