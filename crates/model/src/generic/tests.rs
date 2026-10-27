use super::*;
use crate::{
    atom_text::{self, Mode},
    document::Point,
};

#[derive(Clone, Copy)]
enum HeteroRepeat {
    Polyester,
    Pet,
    Polyether,
}

fn hetero_repeat(kind: HeteroRepeat, imported_3d: bool) -> Document {
    use crate::graphics::{BracketSides, Graphic, GraphicKind};
    let mut source = match kind {
        HeteroRepeat::Polyester => {
            let mut source = chain(&["C", "O", "C", "C", "C"]);
            let oxygen = source.add_atom("O", Point::new(84., -42.));
            source.add_bond(3, oxygen, 2, "plain");
            source
        }
        HeteroRepeat::Polyether => chain(&["C", "O", "C", "C", "C"]),
        HeteroRepeat::Pet => {
            let mut source = chain(&["C", "O", "C", "C", "O", "C"]);
            let ring: Vec<_> = [
                (252., 0.),
                (273., -36.),
                (315., -36.),
                (336., 0.),
                (315., 36.),
                (273., 36.),
            ]
            .into_iter()
            .map(|(x, y)| source.add_atom("C", Point::new(x, y)))
            .collect();
            source.add_bond(6, ring[0], 1, "plain");
            for i in 0..6 {
                source.add_bond(
                    ring[i],
                    ring[(i + 1) % 6],
                    if i % 2 == 0 { 2 } else { 1 },
                    "plain",
                );
            }
            let carbonyl = source.add_atom("C", Point::new(378., 0.));
            source.add_bond(ring[3], carbonyl, 1, "plain");
            let end = source.add_atom("C", Point::new(420., 0.));
            source.add_bond(carbonyl, end, 1, "plain");
            for (carbon, x) in [(6, 210.), (carbonyl, 378.)] {
                let oxygen = source.add_atom("O", Point::new(x, -42.));
                source.add_bond(carbon, oxygen, 2, "plain");
            }
            source
        }
    };
    if imported_3d {
        // crates/io/src/chemistry/molfile/import.rs marks every 3D order-2 bond.
        for bond in source.bonds.iter_mut().filter(|b| b.order == 2) {
            bond.stereo_authoritative = true;
            bond.projection = true;
        }
    }
    let bracket = source.next_id();
    let right = if matches!(kind, HeteroRepeat::Pet) {
        398.
    } else {
        146.
    };
    source.graphics.push(Graphic::dragged(
        bracket,
        GraphicKind::Brackets,
        Point::new(20., -10.),
        Point::new(right, 10.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    define_bracket(&source, bracket, "3", RepeatPattern::HeadToTail).unwrap()
}

fn canonical_smiles(doc: &Document) -> String {
    let molecule = crate::chemistry::document::prepare(doc).unwrap();
    crate::chemistry::smiles::write::write(&molecule.state, Default::default())
        .unwrap()
        .text
}

fn assert_hetero_product(product: &Document, kind: HeteroRepeat) {
    use super::drawing_test_support::formula;
    let (expected_formula, atoms, bonds, expected_smiles) = match kind {
        HeteroRepeat::Polyester => ("C8H12O6", 14, 13, "COC(=O)COC(=O)COC(=O)CC"),
        HeteroRepeat::Pet => {
            const PET: &str =
                "COCCOC(=O)c1ccc(C(=O)OCCOC(=O)c2ccc(C(=O)OCCOC(=O)c3ccc(C(=O)C)cc3)cc2)cc1";
            ("C32H30O12", 44, 46, PET)
        }
        HeteroRepeat::Polyether => ("C8H18O3", 11, 10, "COCCOCCOCCC"),
    };
    assert_eq!(formula(product), expected_formula);
    assert_eq!(product.atoms.len(), atoms);
    assert_eq!(product.bonds.len(), bonds);
    let expected = crate::chemistry::smiles::read(expected_smiles).unwrap();
    let expected =
        crate::chemistry::smiles::write::write(&expected.prepared.state, Default::default())
            .unwrap()
            .text;
    assert_eq!(canonical_smiles(product), expected);
    assert!(!present(product));
}

fn check_hetero_expansion(kind: HeteroRepeat, drawing: bool, imported_3d: bool) {
    let source = hetero_repeat(kind, imported_3d);
    let before = source.clone();
    let products = if drawing {
        expand_drawings(&source, Limits::default())
            .unwrap()
            .into_iter()
            .map(|p| p.document)
            .collect::<Vec<_>>()
    } else {
        expand(&source, Limits::default()).unwrap()
    };
    assert_eq!(source, before);
    assert_eq!(products.len(), 1);
    assert_hetero_product(&products[0], kind);
}

#[test]
fn polyester_tight_bracket_expands_chemistry() {
    check_hetero_expansion(HeteroRepeat::Polyester, false, false);
}

#[test]
fn polyester_tight_bracket_expands_drawings() {
    check_hetero_expansion(HeteroRepeat::Polyester, true, false);
}

#[test]
fn pet_tight_bracket_expands_chemistry() {
    check_hetero_expansion(HeteroRepeat::Pet, false, false);
}

#[test]
fn pet_tight_bracket_expands_drawings() {
    check_hetero_expansion(HeteroRepeat::Pet, true, false);
}

#[test]
fn pet_ring_bond_selection_requires_whole_molecules() {
    let source = hetero_repeat(HeteroRepeat::Pet, false);
    let before = source.clone();
    let bond = source.bonds.iter().find(|b| b.a == 7 && b.b == 8).unwrap();
    let result = prepare_expansion_input(&source, &[bond.a, bond.b], Limits::default())
        .and_then(expand_prepared);
    assert_eq!(
        result.unwrap_err(),
        "Select whole molecules, or clear the selection to expand the whole drawing"
    );
    assert_eq!(source, before);
}

#[test]
fn pet_whole_molecule_selection_expands_prepared_drawing() {
    let source = hetero_repeat(HeteroRepeat::Pet, false);
    let selected = source.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    let input = prepare_expansion_input(&source, &selected, Limits::default()).unwrap();
    let prepared = expand_prepared(input).unwrap();
    assert_eq!(prepared.count(), 1);
    assert_hetero_product(&prepared.take().unwrap(), HeteroRepeat::Pet);
}

#[test]
fn pet_clicked_component_expands_without_unrelated_molecules() {
    let mut source = hetero_repeat(HeteroRepeat::Pet, false);
    let a = source.add_atom("C", Point::new(700., 0.));
    let b = source.add_atom("O", Point::new(742., 0.));
    source.add_bond(a, b, 1, "plain");
    let before = source.clone();
    let input = prepare_component_expansion_input(&source, &[7, 8], Limits::default()).unwrap();
    assert_eq!(input.source, hetero_repeat(HeteroRepeat::Pet, false));
    let prepared = expand_prepared(input).unwrap();
    assert_eq!(prepared.count(), 1);
    assert_hetero_product(&prepared.take().unwrap(), HeteroRepeat::Pet);
    assert_eq!(source, before);
}

#[test]
fn carbon_repeat_complete_selection_avoids_redundant_work_reservation() {
    let mut source = chain(&["C"; 14]);
    source.generic.repeats.push(unit(
        (2..14).collect(),
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 13,
            outside: 14,
        },
        3,
        3,
    ));
    let whole = prepare_expansion_input(&source, &source.all_ids(), Limits::default()).unwrap();
    let clicked = prepare_component_expansion_input(&source, &[2, 3], Limits::default()).unwrap();
    let drawing = prepare_expansion_input(&source, &[], Limits::default()).unwrap();
    for input in [whole, clicked, drawing] {
        let prepared = expand_prepared(input).unwrap();
        assert_eq!(prepared.count(), 1);
        assert_eq!(prepared.take().unwrap().atoms.len(), 38);
    }
    assert_eq!(
        prepare_expansion_input(&source, &[2, 3], Limits::default())
            .err()
            .unwrap(),
        "Select whole molecules, or clear the selection to expand the whole drawing"
    );
}

#[test]
fn clicked_repeat_scope_includes_grouped_caption_brackets_and_attachments() {
    let mut source = hetero_repeat(HeteroRepeat::Pet, false);
    let bracket = source.generic.repeats[0].bracket.unwrap();
    let caption = source.next_id();
    source.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(150., 100.),
        text: "PET".into(),
        format: Default::default(),
    });
    let left = source.add_atom("C", Point::new(700., 0.));
    let right = source.add_atom("C", Point::new(742., 0.));
    source.add_bond(left, right, 1, "plain");
    let attachment = crate::attachments::add(
        &mut source,
        &[left, right],
        crate::attachments::Kind::MultiCenter,
    )
    .unwrap();
    let ligand = source.add_atom("O", Point::new(721., -80.));
    source.add_bond(attachment, ligand, 1, "plain");
    source
        .group_selection(&[7, bracket, caption, attachment])
        .unwrap();
    let expected = source.clone();
    let unrelated = source.add_atom("C", Point::new(1000., 0.));
    let before = source.clone();
    let input = prepare_component_expansion_input(&source, &[7, 8], Limits::default()).unwrap();
    assert_eq!(input.source, expected);
    assert!(input.source.atom(unrelated).is_none());
    assert_eq!(source, before);
}

#[test]
fn huge_repeat_still_exceeds_shared_work_limit() {
    let mut source = chain(&["C"; 4]);
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1000,
        1000,
    ));
    let before = source.clone();
    for input in [
        prepare_expansion_input(&source, &[], Limits::default()),
        prepare_component_expansion_input(&source, &[2, 3], Limits::default()),
    ] {
        assert_eq!(
            input.and_then(expand_prepared).unwrap_err(),
            "Generic expansion exceeds the work limit"
        );
    }
    assert_eq!(source, before);
}

#[test]
fn generic_native_group_validation_is_prepaid_for_every_expansion_scope() {
    use super::probe::{self, Stage};
    use crate::{document::Annotation, grouping::Group};

    let mut source = atom_text::apply(&chain(&["*", "C"]), 1, "[C,N]", Mode::Auto).unwrap();
    // Enough two-member groups that the squared member count exceeds the
    // default allowance on its own.
    let groups = (Limits::default().max_work as f64).sqrt() as u64 / 2 + 1;
    for i in 0..groups {
        let first = 3 + i * 3;
        for id in [first, first + 1] {
            source.annotations.push(Annotation {
                id,
                position: Point::new(0., 100.),
                text: "a".into(),
                format: Default::default(),
            });
        }
        source.groups.push(Group {
            id: first + 2,
            members: vec![first, first + 1],
            integral: false,
        });
    }
    // The source is valid, and this probe observes the actual native pair scan.
    probe::reset();
    source.validate().unwrap();
    assert_eq!(probe::count(Stage::GroupPairs), 1);
    let before = source.clone();
    let full = source.all_ids();
    for selected in [&[][..], full.as_slice(), &[1, 2][..]] {
        probe::reset();
        let result =
            prepare_expansion_input(&source, selected, Limits::default()).and_then(expand_prepared);
        assert_eq!(
            result.unwrap_err(),
            "Generic expansion exceeds the work limit"
        );
        assert_eq!(probe::count(Stage::Validate), 0, "scope: {selected:?}");
        assert_eq!(probe::count(Stage::GroupPairs), 0, "scope: {selected:?}");
    }
    probe::reset();
    assert_eq!(
        prepare_component_expansion_input(&source, &[1], Limits::default())
            .and_then(expand_prepared)
            .unwrap_err(),
        "Generic expansion exceeds the work limit"
    );
    assert_eq!(probe::count(Stage::Validate), 0);
    assert_eq!(probe::count(Stage::GroupPairs), 0);
    // The direct chemistry and drawing entry points share the same admission.
    for drawing in [false, true] {
        probe::reset();
        let error = if drawing {
            expand_drawings(&source, Limits::default()).unwrap_err()
        } else {
            expand(&source, Limits::default()).unwrap_err()
        };
        assert_eq!(error, "Generic expansion exceeds the work limit");
        assert_eq!(probe::count(Stage::Validate), 0);
        assert_eq!(probe::count(Stage::GroupPairs), 0);
    }
    assert_eq!(source, before);
}

#[test]
fn polyether_tight_bracket_expands_chemistry() {
    check_hetero_expansion(HeteroRepeat::Polyether, false, false);
}

#[test]
fn polyether_tight_bracket_expands_drawings() {
    check_hetero_expansion(HeteroRepeat::Polyether, true, false);
}

#[test]
fn imported_3d_hetero_units_expand_chemistry() {
    for kind in [HeteroRepeat::Polyester, HeteroRepeat::Pet] {
        check_hetero_expansion(kind, false, true);
    }
}

#[test]
fn imported_3d_hetero_units_expand_drawings() {
    for kind in [HeteroRepeat::Polyester, HeteroRepeat::Pet] {
        check_hetero_expansion(kind, true, true);
    }
}

#[test]
fn condensed_hetero_repeat_text_labels_explain_how_to_expand() {
    for label in ["OCH2CH2", "CO2", "COO", "OCO"] {
        let source = chain(&["C", "*", "C"]);
        let mut source = atom_text::apply(&source, 2, label, Mode::Auto).unwrap();
        source.generic.repeats.push(unit(
            vec![2],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 2,
                outside: 3,
            },
            3,
            3,
        ));
        let expected = format!(
            "{label} is a text label, not atoms. Draw the repeat unit's atoms (for example O, C=O, CH2) to expand it."
        );
        assert_eq!(expand(&source, Limits::default()).unwrap_err(), expected);
        assert_eq!(
            expand_drawings(&source, Limits::default()).unwrap_err(),
            expected
        );
    }
}

#[test]
fn condensed_hetero_link_text_label_names_the_base_label() {
    let source = atom_text::apply(&chain(&["C", "*", "C"]), 2, "(OCH2CH2)2-4", Mode::Auto).unwrap();
    const LINK: &str = "OCH2CH2 is a text label, not atoms. Draw the repeat unit's atoms (for example O, C=O, CH2) to expand it.";
    assert_eq!(expand(&source, Limits::default()).unwrap_err(), LINK);
    assert_eq!(
        expand_drawings(&source, Limits::default()).unwrap_err(),
        LINK
    );
}

#[test]
fn condensed_hetero_end_group_text_label_explains_how_to_expand() {
    let source =
        atom_text::apply(&chain(&["C", "C", "C", "*"]), 2, "[CH2]2-2", Mode::Auto).unwrap();
    let source = atom_text::apply(&source, 4, "OCH2CH2", Mode::Auto).unwrap();
    const END: &str = "OCH2CH2 is a text label, not atoms. Draw its atoms to expand it.";
    assert_eq!(expand(&source, Limits::default()).unwrap_err(), END);
    assert_eq!(
        expand_drawings(&source, Limits::default()).unwrap_err(),
        END
    );
}

#[test]
fn real_repeat_alkene_stereo_stays_refused_in_both_modes() {
    for authority_only in [false, true] {
        let mut source = chain(&["C", "C", "C", "C", "C", "C"]);
        source.bonds[2].order = 2;
        if authority_only {
            source.bonds[2].stereo_authoritative = true;
        } else {
            source.bonds[2].stereo = Some("e".into());
            source.bonds[2].stereo_atoms = vec![2, 5];
        }
        source.generic.repeats.push(unit(
            vec![2, 3, 4, 5],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 5,
                outside: 6,
            },
            3,
            3,
        ));
        const STEREO: &str = "Generic expansion involving specified atom or bond stereochemistry is unsupported; the source drawing is retained";
        assert_eq!(expand(&source, Limits::default()).unwrap_err(), STEREO);
        assert_eq!(
            expand_drawings(&source, Limits::default()).unwrap_err(),
            STEREO
        );
    }
}

#[test]
fn isotope_and_coordination_alkene_stereo_in_a_repeat_unit_stays_refused() {
    const STEREO: &str = "Generic expansion involving specified atom or bond stereochemistry is unsupported; the source drawing is retained";
    for authority_only in [false, true] {
        // A pendant -CH=CHD ([2H]/C=C/C keeps its E/Z) on a C-C unit.
        let mut pendant = chain(&["C", "C", "C", "C"]);
        let alkene = pendant.add_atom("C", Point::new(42., 42.));
        let end = pendant.add_atom("C", Point::new(84., 84.));
        let deuterium = pendant.add_atom("H", Point::new(126., 84.));
        pendant.atom_mut(deuterium).unwrap().isotope = 2;
        pendant.add_bond(2, alkene, 1, "plain");
        pendant.add_bond(alkene, end, 2, "plain");
        pendant.add_bond(end, deuterium, 1, "plain");
        pendant.generic.repeats.push(unit(
            vec![2, 3, alkene, end, deuterium],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
            3,
            3,
        ));
        // N1/C=C/N->[Zn]<-1 is acyclic for ring perception.
        let mut chelate = chain(&["C", "N", "C", "C", "N", "C"]);
        let zinc = chelate.add_atom("Zn", Point::new(126., 42.));
        chelate.add_bond(2, zinc, crate::bonds::DATIVE, "plain");
        chelate.add_bond(5, zinc, crate::bonds::DATIVE, "plain");
        chelate.bonds[2].order = 2;
        chelate.generic.repeats.push(unit(
            vec![2, 3, 4, 5, zinc],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 5,
                outside: 6,
            },
            3,
            3,
        ));
        for (mut source, bond, controls) in
            [(pendant, 4, vec![2, deuterium]), (chelate, 2, vec![2, 5])]
        {
            let bond = &mut source.bonds[bond];
            assert_eq!(bond.order, 2);
            if authority_only {
                bond.stereo_authoritative = true;
            } else {
                bond.stereo = Some("e".into());
                bond.stereo_atoms = controls;
            }
            assert_eq!(expand(&source, Limits::default()).unwrap_err(), STEREO);
            assert_eq!(
                expand_drawings(&source, Limits::default()).unwrap_err(),
                STEREO
            );
        }
    }
}

fn chain(elements: &[&str]) -> Document {
    let mut doc = Document::default();
    let mut last = None;
    for (i, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(i as f32 * 42., 0.));
        if let Some(previous) = last {
            doc.add_bond(previous, id, 1, "plain");
        }
        last = Some(id);
    }
    doc
}

#[test]
fn shared_alternative_labels_keep_cdxml_import_isotope_spelling() {
    let mut site = AlternativeSite {
        atom: 1,
        choices: vec![
            Alternative::Element {
                symbol: "H".into(),
                isotope: 2,
            },
            Alternative::Element {
                symbol: "H".into(),
                isotope: 3,
            },
            Alternative::Element {
                symbol: "H".into(),
                isotope: 4,
            },
            Alternative::Element {
                symbol: "C".into(),
                isotope: 13,
            },
            Alternative::Group {
                label: "OMe".into(),
            },
            Alternative::Unresolved { label: "R1".into() },
        ],
        excluded: true,
    };
    assert_eq!(site.label(), "[NOT D,T,H,C,OMe,R1]");
    assert_eq!(
        AlternativeSite::label_without_isotope_aliases(site.excluded, &site.choices),
        "[NOT H,H,H,C,OMe,R1]"
    );
    site.excluded = false;
    assert_eq!(site.label(), "[D,T,H,C,OMe,R1]");
    assert_eq!(
        AlternativeSite::label_without_isotope_aliases(site.excluded, &site.choices),
        "[H,H,H,C,OMe,R1]"
    );
}

#[test]
fn shared_crossing_bond_position_uses_first_undirected_match() {
    let mut doc = chain(&["C", "C", "C"]);
    let mut duplicate = doc.bonds[0].clone();
    std::mem::swap(&mut duplicate.a, &mut duplicate.b);
    doc.bonds.push(duplicate);
    assert_eq!(doc.bond_position(1, 2), Some(0));
    assert_eq!(doc.bond_position(2, 1), Some(0));
    assert_eq!(doc.bond_position(1, 3), None);
}

#[test]
fn touches_is_intersects_or_component_intersects() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let connected = source.add_atom("C", Point::new(-42., 0.));
    source.add_bond(1, connected, 1, "plain");
    let unrelated = source.add_atom("C", Point::new(200., 100.));
    let bracket = source.next_id();
    source.graphics.push(crate::graphics::Graphic::dragged(
        bracket,
        crate::graphics::GraphicKind::Brackets,
        Point::new(20., -30.),
        Point::new(104., 30.),
        Default::default(),
        crate::graphics::BracketSides::Both,
        false,
    ));
    let source = define_bracket(&source, bracket, "1-3", RepeatPattern::HeadToTail).unwrap();
    assert!(intersects(&source, &[bracket]));
    assert!(!component_intersects(&source, &[bracket]));
    assert!(!intersects(&source, &[connected]));
    assert!(component_intersects(&source, &[connected]));
    assert!(!touches(&source, &[unrelated]));
    for ids in [
        vec![],
        vec![bracket],
        vec![connected],
        vec![2],
        vec![unrelated],
    ] {
        assert_eq!(
            touches(&source, &ids),
            intersects(&source, &ids) || component_intersects(&source, &ids)
        );
    }
}

#[test]
fn owns_label_reports_alternative_and_link_sites() {
    let source = chain(&["C", "C", "C", "C"]);
    assert!(!source.generic.owns_label(1));
    let source = atom_text::apply(&source, 1, "[Cl,Br]", Mode::Auto).unwrap();
    let source = atom_text::apply(&source, 2, "[CH2]1-3", Mode::Auto).unwrap();
    assert!(source.generic.owns_label(1));
    assert!(source.generic.owns_label(2));
    assert!(!source.generic.owns_label(3));
    assert!(!source.generic.owns_label(u64::MAX));
}

#[test]
fn two_mixed_anonymous_sites_are_independent_real_fragments() {
    let source = chain(&["*", "C", "C", "*"]);
    let source = atom_text::apply(&source, 1, "[OMe,OPh]", Mode::Auto).unwrap();
    let source = atom_text::apply(&source, 4, "[Cl,Br]", Mode::Auto).unwrap();
    let before = source.clone();
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(source, before);
    assert_eq!(products.len(), 4);
    let mut counts: Vec<_> = products.iter().map(|d| d.atoms.len()).collect();
    counts.sort();
    assert_eq!(counts, vec![5, 5, 10, 10]);
    for product in products {
        assert!(!present(&product));
        assert_eq!(product.atom(1).unwrap().element, "O");
        let neighbors: Vec<_> = product
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == 1 {
                    Some(b.b)
                } else if b.b == 1 {
                    Some(b.a)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&2));
        let group_root = *neighbors.iter().find(|id| **id != 2).unwrap();
        assert_eq!(product.atom(group_root).unwrap().element, "C");
        assert_eq!(
            bonds_at(&product, group_root).len(),
            if product.atoms.len() == 5 { 1 } else { 3 }
        );
        assert!(crate::chemistry::document::prepare(&product).is_ok());
    }
}
#[test]
fn finite_link_nodes_expand_chain_and_ring() {
    for ring in [false, true] {
        let mut source = chain(&["C", "C", "C", "C"]);
        if ring {
            source.add_bond(1, 4, 1, "plain");
        }
        let source = atom_text::apply(&source, 2, "[CH2]1-3", Mode::Auto).unwrap();
        let products = expand(&source, Limits::default()).unwrap();
        assert_eq!(
            products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
            vec![4, 5, 6]
        );
        assert_eq!(
            products.iter().map(|d| d.bonds.len()).collect::<Vec<_>>(),
            if ring { vec![4, 5, 6] } else { vec![3, 4, 5] }
        );
    }
}
#[test]
fn variable_attachment_chooses_one_real_target() {
    let mut source = chain(&["C", "C", "C"]);
    let point =
        crate::attachments::add(&mut source, &[1, 2, 3], crate::attachments::Kind::Variable)
            .unwrap();
    let cl = source.add_atom("Cl", Point::new(42., 42.));
    source.add_bond(point, cl, 1, "plain");
    let products = expand(&source, Limits::default()).unwrap();
    // Propane's terminal targets 1 and 3 both produce 1-chloropropane.
    // Keep the first terminal representative, followed by 2-chloropropane.
    assert_eq!(products.len(), 2);
    for (product, target) in products.iter().zip([1, 2]) {
        assert!(product.atom(point).is_none());
        assert!(
            product
                .bonds
                .iter()
                .any(|b| pair(b.a, b.b) == pair(target, cl))
        );
    }
}
#[test]
fn finite_polymer_keeps_both_end_groups() {
    let mut source = chain(&["O", "C", "C", "N"]);
    set_repeat_unit(
        &mut source,
        RepeatUnit {
            bracket: None,
            bracket_objects: vec![],
            members: vec![2, 3],
            crossings: vec![
                Crossing {
                    inside: 2,
                    outside: 1,
                },
                Crossing {
                    inside: 3,
                    outside: 4,
                },
            ],
            usage: "SRU".into(),
            crossing_brackets: vec![],
            head_count: Some(1),
            pattern: RepeatPattern::HeadToTail,
            count: RepeatCount::Finite { min: 1, max: 3 },
            label: "1-3".into(),
            flip: "NoFlip".into(),
        },
    )
    .unwrap();
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(
        products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
        vec![4, 6, 8]
    );
    for p in products {
        assert_eq!(p.atoms.iter().filter(|a| a.element == "O").count(), 1);
        assert_eq!(p.atoms.iter().filter(|a| a.element == "N").count(), 1);
        assert_eq!(p.bonds.len(), p.atoms.len() - 1);
    }
}
#[test]
fn malformed_or_indefinite_choices_are_atomic_and_bounded() {
    let source = chain(&["C", "C", "C"]);
    for label in ["[CH2]0-3", "[CH2]3-1", "[Cl,,Br]", "[OMe,?]"] {
        assert!(atom_text::apply(&source, 2, label, Mode::Auto).is_err());
    }
    let source = atom_text::apply(&source, 1, "[Cl,Br]", Mode::Auto).unwrap();
    assert!(
        expand(
            &source,
            Limits {
                max_outputs: 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    assert!(crate::chemistry::document::prepare(&source).is_err());
    let bytes = source.file_json().unwrap();
    assert_eq!(Document::from_json(&bytes).unwrap().generic, source.generic);
}

#[test]
fn copy_remaps_typed_sites_and_native_reopen_keeps_finite_and_indefinite_units() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let bracket = source.next_id();
    source.graphics.push(crate::graphics::Graphic::dragged(
        bracket,
        crate::graphics::GraphicKind::Brackets,
        Point::new(20., -30.),
        Point::new(104., 30.),
        Default::default(),
        crate::graphics::BracketSides::Both,
        false,
    ));
    source = define_bracket(&source, bracket, "n", RepeatPattern::HeadToTail).unwrap();
    let selected = crate::editing::selection(&source, &[bracket]);
    assert_eq!(selected.generic.repeats.len(), 1);
    let mut target = chain(&["F"]);
    let inserted = crate::editing::append(&mut target, &selected, Point::new(0., 100.));
    assert_eq!(inserted.len(), 5);
    let unit = target.generic.repeats.first().unwrap();
    assert_eq!(unit.members, vec![3, 4]);
    assert_eq!(
        unit.crossings,
        vec![
            Crossing {
                inside: 3,
                outside: 2
            },
            Crossing {
                inside: 4,
                outside: 5
            }
        ]
    );
    assert_eq!(unit.bracket, Some(6));
    assert_eq!(
        Document::from_native_file(&target.file_json().unwrap())
            .unwrap()
            .generic,
        target.generic
    );
    assert!(
        expand(&target, Limits::default())
            .unwrap_err()
            .contains("indefinite")
    );
    let candidate = define_bracket(&target, 6, "1-3", RepeatPattern::HeadToTail).unwrap();
    assert_eq!(expand(&candidate, Limits::default()).unwrap().len(), 3);
}
#[test]
fn invalid_valence_rejects_the_entire_enumeration_without_changing_source() {
    let source = chain(&["C", "C", "C"]);
    let source = atom_text::apply(&source, 2, "[C,Cl]", Mode::Auto).unwrap();
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("invalid chemistry")
    );
    assert_eq!(source, before);
}
#[test]
fn isotope_choices_materialize_and_multiple_variable_sites_are_independent() {
    let source = chain(&["C", "*"]);
    let source = atom_text::apply(&source, 2, "[H,D,T]", Mode::Auto).unwrap();
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(
        products
            .iter()
            .map(|d| d.atom(2).unwrap().isotope)
            .collect::<Vec<_>>(),
        vec![0, 2, 3]
    );
    let mut source = chain(&["C", "C", "C", "C"]);
    for (members, element, x) in [([1, 2], "F", 0.), ([3, 4], "Cl", 126.)] {
        let point =
            crate::attachments::add(&mut source, &members, crate::attachments::Kind::Variable)
                .unwrap();
        let atom = source.add_atom(element, Point::new(x, 42.));
        source.add_bond(point, atom, 1, "plain");
    }
    assert_eq!(expand(&source, Limits::default()).unwrap().len(), 4);
}
#[test]
fn unsupported_polymer_topology_is_retained_and_explicitly_rejected() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let unit = RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members: vec![2, 3],
        crossings: vec![
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
        ],
        usage: "SRU".into(),
        crossing_brackets: vec![],
        head_count: Some(1),
        pattern: RepeatPattern::HeadToHead,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "NoFlip".into(),
    };
    set_repeat_unit(&mut source, unit).unwrap();
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("repeat pattern")
    );
    assert_eq!(source, before);
    assert_eq!(
        Document::from_json(&source.file_json().unwrap())
            .unwrap()
            .generic,
        source.generic
    );
}
#[test]
fn atom_and_work_limits_are_checked_before_large_replication() {
    let source = chain(&["C", "C", "C"]);
    let source = atom_text::apply(&source, 2, "[CH2]1000-1000", Mode::Auto).unwrap();
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 10,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom limit")
    );
    assert!(
        expand(
            &source,
            Limits {
                max_work: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(products[0].atoms.len(), 1002);
}

fn unit(members: Vec<u64>, head: Crossing, tail: Crossing, min: u32, max: u32) -> RepeatUnit {
    RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members,
        crossings: vec![head, tail],
        usage: "SRU".into(),
        crossing_brackets: vec![],
        head_count: Some(1),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min, max },
        label: if min == max {
            min.to_string()
        } else {
            format!("{min}-{max}")
        },
        flip: "NoFlip".into(),
    }
}

#[test]
fn authoritative_alkene_stereo_is_rejected_atomically() {
    let mut source = chain(&["Cl", "C", "C", "N"]);
    source.bonds[1].order = 2;
    source.bonds[1].stereo_authoritative = true;
    source.bonds[1].stereo = Some("Z".into());
    source.bonds[1].stereo_atoms = vec![1, 4];
    set_repeat_unit(
        &mut source,
        unit(
            vec![2, 3],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
            2,
            2,
        ),
    )
    .unwrap();
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("stereochemistry")
    );
    assert_eq!(source, before);
}

#[test]
fn link_chiral_tail_and_element_choice_neighbor_stereo_are_rejected_atomically() {
    let mut source = chain(&["C", "C", "C", "F"]);
    let cl = source.add_atom("Cl", Point::new(84., 42.));
    let br = source.add_atom("Br", Point::new(84., -42.));
    source.add_bond(3, cl, 1, "plain");
    source.add_bond(3, br, 1, "plain");
    source.generic.links.push(LinkNode {
        atom: 2,
        min: 2,
        max: 2,
    });
    source.atom_mut(3).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![2, 4, cl, br],
    });
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("stereochemistry")
    );
    assert_eq!(source, before);
    source.generic.links.clear();
    source.atom_mut(2).unwrap().element = "*".into();
    source.atom_mut(2).unwrap().no_implicit = true;
    source.generic.alternatives.push(AlternativeSite {
        atom: 2,
        choices: vec![Alternative::Element {
            symbol: "C".into(),
            isotope: 0,
        }],
        excluded: false,
    });
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("stereochemistry")
    );
    assert_eq!(source, before);
}

#[test]
fn directly_adjacent_units_reject_in_both_definition_orders() {
    let mut source = chain(&["O", "C", "C", "N"]);
    source.generic.repeats = vec![
        unit(
            vec![2],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 2,
                outside: 3,
            },
            2,
            2,
        ),
        unit(
            vec![3],
            Crossing {
                inside: 3,
                outside: 2,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
            2,
            2,
        ),
    ];
    for _ in 0..2 {
        let before = source.clone();
        assert!(
            expand(&source, Limits::default())
                .unwrap_err()
                .contains("Directly adjacent")
        );
        assert_eq!(source, before);
        source.generic.repeats.reverse();
    }
    source.generic.repeats.remove(1);
    source.generic.links.push(LinkNode {
        atom: 3,
        min: 1,
        max: 2,
    });
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("Directly adjacent")
    );
}

#[test]
fn mapped_repeat_and_link_atoms_are_retained_and_rejected() {
    let mut source = chain(&["O", "C", "C", "N"]);
    source.atom_mut(2).unwrap().map_num = 7;
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        2,
        2,
    ));
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("Mapped")
    );
    assert_eq!(source, before);
    source.generic.repeats.clear();
    source.generic.links.push(LinkNode {
        atom: 2,
        min: 2,
        max: 2,
    });
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("Mapped")
    );
}

#[test]
fn ring_repeat_closes_on_the_existing_external_component() {
    let mut source = chain(&["C", "C", "C", "C"]);
    source.add_bond(1, 4, 1, "plain");
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1,
        3,
    ));
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(
        products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
        vec![4, 6, 8]
    );
    for p in products {
        assert_eq!(p.atoms.len(), p.bonds.len());
        assert!(p.atoms.iter().all(|a| bonds_at(&p, a.id).len() == 2));
        assert_eq!(
            p.atom(1).unwrap().position,
            source.atom(1).unwrap().position
        );
        assert_eq!(
            p.atom(4).unwrap().position,
            source.atom(4).unwrap().position
        );
    }
}

#[test]
fn fragment_catalog_work_and_exact_output_atom_boundaries_are_checked() {
    let source = atom_text::apply(&chain(&["*", "C"]), 1, "[OMe,OPh]", Mode::Auto).unwrap();
    assert!(
        expand(
            &source,
            Limits {
                max_work: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    let source = atom_text::apply(&chain(&["*", "C", "*"]), 1, "[Cl,Br]", Mode::Auto).unwrap();
    let source = atom_text::apply(&source, 3, "[F,I]", Mode::Auto).unwrap();
    assert_eq!(
        expand(
            &source,
            Limits {
                max_outputs: 4,
                max_total_atoms: 12,
                ..Limits::default()
            }
        )
        .unwrap()
        .len(),
        4
    );
    assert!(
        expand(
            &source,
            Limits {
                max_outputs: 3,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("output limit")
    );
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 11,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom limit")
    );
    let mut source = chain(&["O", "C", "C", "N"]);
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1,
        3,
    ));
    assert_eq!(
        expand(
            &source,
            Limits {
                max_total_atoms: 18,
                ..Limits::default()
            }
        )
        .unwrap()
        .len(),
        3
    );
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 17,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom limit")
    );
}

#[test]
fn large_repeats_check_work_and_source_size_before_graph_construction() {
    let mut source = chain(&vec!["C"; 101]);
    source.generic.repeats.push(unit(
        (2..=100).collect(),
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 100,
            outside: 101,
        },
        1000,
        1000,
    ));
    let before = source.clone();
    assert!(
        expand(
            &source,
            Limits {
                max_work: 1_000_000,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    assert_eq!(source, before);
    // Source validation itself uses a prebuilt ID index, including a large
    // member set. Cheap bounds precede even that validation in expansion.
    source.generic.repeats[0].members = (1..=101).collect();
    source.generic.repeats[0].crossings.clear();
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 100,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom or bond limit")
    );
}

#[test]
fn entering_alternative_and_link_labels_preserves_neighbor_stereochemistry() {
    let mut chiral = chain(&["C", "C", "C", "C"]);
    let f = chiral.add_atom("F", Point::new(84., 42.));
    let cl = chiral.add_atom("Cl", Point::new(84., -42.));
    chiral.add_bond(3, f, 1, "plain");
    chiral.add_bond(3, cl, 1, "plain");
    chiral.atom_mut(3).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![2, 4, f, cl],
    });
    let mut alkene = chain(&["C", "C", "C", "C", "N"]);
    alkene.bonds[2].order = 2;
    alkene.bonds[2].stereo_authoritative = true;
    alkene.bonds[2].stereo = Some("Z".into());
    alkene.bonds[2].stereo_atoms = vec![2, 5];
    let mut marker = chiral.clone();
    marker.atom_mut(3).unwrap().stereo = None;
    marker
        .bonds
        .iter_mut()
        .find(|b| pair(b.a, b.b) == pair(3, f))
        .unwrap()
        .display = "wedge".into();
    for source in [chiral, alkene, marker] {
        source.validate().unwrap();
        let before = source.clone();
        for label in ["[C,N]", "[CH2]1-3"] {
            assert!(
                atom_text::apply(&source, 2, label, Mode::Auto)
                    .unwrap_err()
                    .contains("stereochemistry")
            );
            assert_eq!(source, before);
        }
    }
}

#[test]
fn native_current_version_preserves_generic_units_and_still_reads_version19() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let mut repeat = unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1,
        3,
    );
    repeat.count = RepeatCount::Indefinite { label: "n".into() };
    repeat.label = "n".into();
    source.generic.repeats.push(repeat);
    let bytes = source.file_json().unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
        crate::document::DISTANCE_BONDS
    );
    let reopened = Document::from_native_file(&bytes).unwrap();
    assert_eq!(reopened.version, crate::document::DISTANCE_BONDS);
    assert_eq!(reopened.generic, source.generic);
    let older = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/geometry/adamantane.rsk"
    ))
    .unwrap();
    assert_eq!(older.version, 19);
    assert!(older.generic.is_empty());
    assert_eq!(older.atoms.len(), 10);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&older.file_json().unwrap()).unwrap()["version"],
        crate::document::DISTANCE_BONDS
    );
    // Clipboard and recovery copies use the same established current gate.
    assert_eq!(source.current().version, crate::document::DISTANCE_BONDS);
}

/// 4-Chlorophenol as Clean up lays it out (Cl left, O right) with an
/// alternative list on each substituent, as in issue #316.
fn halophenol_lists() -> Document {
    let mut source = Document::default();
    let ring: Vec<_> = [0_f32, 60., 120., 180., 240., 300.]
        .into_iter()
        .map(|deg| {
            let (sin, cos) = deg.to_radians().sin_cos();
            source.add_atom("C", Point::new(24. * cos, 24. * sin))
        })
        .collect();
    for i in 0..6 {
        let order = if i % 2 == 0 { 2 } else { 1 };
        source.add_bond(ring[i], ring[(i + 1) % 6], order, "plain");
    }
    let oxygen = source.add_atom("O", Point::new(48., 0.));
    source.add_bond(ring[0], oxygen, 1, "plain");
    let chlorine = source.add_atom("Cl", Point::new(-48., 0.));
    source.add_bond(ring[3], chlorine, 1, "plain");
    let source = atom_text::apply(&source, chlorine, "[Cl,Br,I]", Mode::Auto).unwrap();
    atom_text::apply(&source, oxygen, "[OMe,OPh]", Mode::Auto).unwrap()
}

#[test]
fn small_ring_bearing_list_product_expands_within_default_work() {
    let source = halophenol_lists();
    let all = source.all_ids();
    for selected in [&[][..], all.as_slice()] {
        let prepared = prepare_expansion_input(&source, selected, Limits::default())
            .and_then(expand_prepared)
            .unwrap();
        assert_eq!(prepared.count(), 6);
    }
    let products = expand_drawings(&source, Limits::default()).unwrap();
    let mut atoms: Vec<_> = products.iter().map(|p| p.document.atoms.len()).collect();
    atoms.sort();
    // Three 4-haloanisoles and three 4-halophenyl phenyl ethers.
    assert_eq!(atoms, [9, 9, 9, 14, 14, 14]);
}
