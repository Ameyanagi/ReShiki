// SPDX-License-Identifier: MIT
// Origin: original
// Source: ReShiki | crates/opsin/tests/atropisomer_names.rs | 2df788dc456793c687b934afd6d4b317e3c73ef8

use opsin::{
    Axis, ParseOptions, Parser, Status, Structure,
    graph::{AxialStereo, Graph},
    smiles,
};

fn parse(name: &str) -> Structure {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    result.structure.unwrap()
}

fn written_axis(name: &str, expected: &str, locant: &str) -> (String, Vec<Option<String>>, Axis) {
    let structure = parse(name);
    let bond = structure
        .graph
        .fragment(structure.fragment)
        .bonds
        .iter()
        .map(|&id| structure.graph.bond(id))
        .find(|bond| bond.axial_stereo.is_some())
        .unwrap();
    assert!(structure.semantic_cxsmiles().is_err());
    assert!(structure.semantic_cxsmiles_with_locants().is_err());
    let (text, locants, axes) = structure.semantic_cxsmiles_with_locants_and_axes().unwrap();
    assert_eq!(axes.len(), 1, "{name}");
    let axis = axes.into_iter().next().unwrap();
    assert_eq!(axis.descriptor, expected, "{name}");
    if [bond.from, bond.to]
        .iter()
        .any(|atom| structure.principal_parent_atoms.contains(atom))
    {
        assert!(
            axis.atoms
                .iter()
                .any(|&index| locants[index].as_deref() == Some(locant)),
            "{name}: {locants:?}"
        );
    }

    // Check exported indices against the graph reconstructed from that exact
    // SMILES, whose traversal differs from OPSIN's construction atom order.
    let mut written = Graph::default();
    let fragment =
        smiles::build_fragment(&mut written, text.split(" |").next().unwrap(), "", "none").unwrap();
    let atoms = &written.fragment(fragment).atoms;
    assert_eq!(atoms.len(), locants.len());
    assert!(
        written
            .bond_between(atoms[axis.atoms[0]], atoms[axis.atoms[1]])
            .is_some()
    );
    for end in 0..2 {
        assert!(
            written
                .bond_between(atoms[axis.atoms[end]], atoms[axis.refs[end]])
                .is_some()
        );
    }
    // Native indices enumerate active construction atoms in increasing AtomId
    // order. Each reference is the first nonaxis neighbour in that order.
    let stereo = bond.axial_stereo.as_ref().unwrap();
    assert_eq!(stereo.descriptor, expected, "{name}");
    assert_eq!(axis.clockwise, stereo.clockwise, "{name}");
    for (end, partner, reference) in [
        (bond.from, bond.to, stereo.refs[0]),
        (bond.to, bond.from, stereo.refs[1]),
    ] {
        let carrier = structure
            .graph
            .atom(end)
            .bonds
            .iter()
            .filter_map(|&id| {
                let bond = structure.graph.bond(id);
                bond.active.then(|| bond.other_atom(end)).flatten()
            })
            .filter(|&atom| atom != partner && structure.graph.atom(atom).active)
            .min()
            .unwrap();
        assert_eq!(reference, carrier, "{name}");
    }
    // Check consistency after reimporting the exported SMILES and translating
    // its reference neighbours into the native bond carrier convention.
    let mut native = nazuchem_kernel::smiles::read(&text, &Default::default())
        .unwrap()
        .prepared
        .state;
    let native_bond = native
        .graph
        .bonds
        .iter()
        .position(|bond| [bond.a, bond.b] == axis.atoms || [bond.b, bond.a] == axis.atoms)
        .unwrap();
    let bond = &native.graph.bonds[native_bond];
    let refs = if bond.a == axis.atoms[0] {
        axis.refs
    } else {
        [axis.refs[1], axis.refs[0]]
    };
    let mut clockwise = axis.clockwise;
    for (end, partner, reference) in [(bond.a, bond.b, refs[0]), (bond.b, bond.a, refs[1])] {
        let carrier = native
            .graph
            .bonds
            .iter()
            .filter_map(|bond| {
                let other = if bond.a == end {
                    Some(bond.b)
                } else if bond.b == end {
                    Some(bond.a)
                } else {
                    None
                }?;
                (other != partner).then_some(other)
            })
            .min()
            .unwrap();
        clockwise ^= carrier != reference;
    }
    native.metadata.bonds[native_bond].stereo = if clockwise { 6 } else { 7 };
    native.metadata.bonds[native_bond].stereo_atoms = refs.to_vec();
    let labelled =
        nazuchem_kernel::stereo::cip::label::assign(&native, &Default::default()).unwrap();
    assert_eq!(
        labelled.state.properties.bond_codes[native_bond].as_deref(),
        Some(expected),
        "{name}: native CIP reimport consistency disagreement"
    );
    (text, locants, axis)
}

fn check_both_axis_hands_and_general_descriptors(base: &str, locant: &str) {
    let m = written_axis(&format!("({locant}M)-{base}"), "M", locant);
    let p = written_axis(&format!("({locant}P)-{base}"), "P", locant);
    assert_eq!(m.0, p.0, "{base}");
    assert_eq!(m.1, p.1, "{base}");
    assert_eq!(m.2.atoms, p.2.atoms, "{base}");
    assert_eq!(m.2.refs, p.2.refs, "{base}");
    assert_ne!(m.2.clockwise, p.2.clockwise, "{base}");
    for (descriptor, expected, axis) in [("Ra", "M", m), ("Sa", "P", p)] {
        assert_eq!(
            axis,
            written_axis(&format!("({locant}{descriptor})-{base}"), expected, locant),
            "{base}"
        );
    }
}

#[test]
fn axial_carriers_follow_native_atom_order_across_cip_priority_cases() {
    for (base, locant) in [
        ("[1,1'-biphenyl]-2-ol-2'-d", "1"),
        ("2-methyl[1,1'-biphenyl]-2'-d", "1"),
        ("2-bromo[1,1'-biphenyl]-2'-d", "1"),
        ("2-[2-methyl-6-(propan-2-yl)pyridin-4-yl]phenol", "2"),
        ("2-[2-ethyl-6-(propan-2-yl)pyridin-4-yl]phenol", "2"),
        ("2-(2-methyl-6-phenylpyridin-4-yl)phenol", "2"),
    ] {
        check_both_axis_hands_and_general_descriptors(base, locant);
    }
}

#[test]
fn axial_locants_follow_the_annotated_component_namespace() {
    for (base, locant) in [
        (
            "2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol",
            "2",
        ),
        ("[1,1'-binaphthalene]-2,2'-diol", "1"),
        (
            "2'-hydroxy[1,1'-binaphthalen]-2-yl (1S)-2,2-dimethylcyclopropane-1-carboxylate",
            "1",
        ),
    ] {
        check_both_axis_hands_and_general_descriptors(base, locant);
    }
}

#[test]
fn axial_locants_do_not_fall_back_to_a_substituent_namespace() {
    let parser = Parser::new().unwrap();
    for (base, locant) in [
        (
            "2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol",
            "1",
        ),
        ("2-(2-hydroxyphenyl)phenol", "1"),
        ("2-(8-hydroxynaphthalen-7-yl)phenol", "7"),
        ("2-(6-methylnaphthalen-7-yl)phenol", "7"),
        ("2-(7-hydroxynaphthalen-8-yl)phenol", "8"),
        ("2-(7-methylnaphthalen-8-yl)phenol", "8"),
    ] {
        for descriptor in ["M", "P", "Ra", "Sa"] {
            let name = format!("({locant}{descriptor})-{base}");
            let result = parser.parse(&name, &ParseOptions::default());
            assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
            assert!(result.structure.is_none(), "{name}");
        }
    }
}

#[test]
fn constitutionally_equivalent_aryl_ligands_cannot_define_an_axis() {
    let parser = Parser::new().unwrap();
    for base in [
        "1-(2-hydroxyphenyl)cyclopenta-2,4-dien-1-ide",
        "1-(2-methylphenyl)cyclopenta-2,4-dien-1-ide",
    ] {
        for prefix in ["", "sodium "] {
            for descriptor in ["M", "P", "Ra", "Sa"] {
                let name = format!("{prefix}(1{descriptor})-{base}");
                let result = parser.parse(&name, &ParseOptions::default());
                assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
                assert!(result.structure.is_none(), "{name}");
            }
        }
    }
}

#[test]
fn naphthyl_phenol_parent_locant_preserves_both_axis_hands() {
    for (descriptor, expected, clockwise) in [
        ("M", "M", true),
        ("P", "P", false),
        ("Ra", "M", true),
        ("Sa", "P", false),
    ] {
        let name = format!("(2{descriptor})-2-(8-hydroxynaphthalen-7-yl)phenol");
        let (_, _, axis) = written_axis(&name, expected, "2");
        // Independent RDKit 2026.09.1 CIP and 3D geometry checks fix this hand.
        assert_eq!(axis.atoms, [2, 11], "{name}");
        assert_eq!(axis.refs, [3, 12], "{name}");
        assert_eq!(axis.clockwise, clockwise, "{name}");
    }
}

#[test]
fn blue_book_p_93_5_7_1_pin_and_general_names_have_the_same_axis() {
    for (pin, general, locant) in [
        (
            "(1M)-6,6'-dinitro[1,1'-biphenyl]-2,2'-dicarboxylic acid",
            "(1Ra)-6,6'-dinitro[1,1'-biphenyl]-2,2'-dicarboxylic acid",
            "1",
        ),
        (
            "(1P)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylic acid",
            "(1Sa)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylic acid",
            "1",
        ),
        (
            "(1M)-2',6-diamino-6'-methoxy[1,1'-biphenyl]-2-carboxylic acid",
            "(1Ra)-2',6-diamino-6'-methoxy[1,1'-biphenyl]-2-carboxylic acid",
            "1",
        ),
        (
            "(2P)-2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol",
            "(2Sa)-2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol",
            "2",
        ),
        (
            "(1M)-2'-hydroxy[1,1'-binaphthalen]-2-yl (1S)-2,2-dimethylcyclopropane-1-carboxylate",
            "(1Ra)-2'-hydroxy[1,1'-binaphthalen]-2-yl (1S)-2,2-dimethylcyclopropane-1-carboxylate",
            "1",
        ),
    ] {
        let expected = if pin.starts_with("(1M)") { "M" } else { "P" };
        assert_eq!(
            written_axis(pin, expected, locant),
            written_axis(general, expected, locant)
        );
    }
}

#[test]
fn both_binol_hands_and_their_general_descriptors_are_preserved() {
    let m = written_axis("(1M)-[1,1'-binaphthalene]-2,2'-diol", "M", "1");
    let p = written_axis("(1P)-[1,1'-binaphthalene]-2,2'-diol", "P", "1");
    assert_eq!(m.0, p.0);
    assert_eq!(m.1, p.1);
    assert_eq!(m.2.atoms, p.2.atoms);
    assert_eq!(m.2.refs, p.2.refs);
    assert_ne!(m.2, p.2);
    assert_eq!(
        m,
        written_axis("(1Ra)-[1,1'-binaphthalene]-2,2'-diol", "M", "1")
    );
    assert_eq!(
        p,
        written_axis("(1Sa)-[1,1'-binaphthalene]-2,2'-diol", "P", "1")
    );
}

#[test]
fn axis_requires_a_locant_an_aryl_link_and_distinct_ligands() {
    for name in [
        "(M)-[1,1'-binaphthalene]-2,2'-diol",
        "(2M)-[1,1'-binaphthalene]-2,2'-diol",
        "(1M)-biphenyl",
        "(1M)-1-cyclohexylcyclohexane",
        "(1M)-1-(cyclohex-1-en-1-yl)cyclohex-1-ene",
        "(1Rp)-[1,1'-binaphthalene]-2,2'-diol",
        "(1Sp)-[1,1'-binaphthalene]-2,2'-diol",
        "(1M,1M)-[1,1'-binaphthalene]-2,2'-diol",
        "(1M,1P)-[1,1'-binaphthalene]-2,2'-diol",
        "(1M,1'P)-[1,1'-binaphthalene]-2,2'-diol",
        "rac-(1M)-[1,1'-binaphthalene]-2,2'-diol",
        "rel-(1M)-[1,1'-binaphthalene]-2,2'-diol",
        "(rac)-(1M)-[1,1'-binaphthalene]-2,2'-diol",
    ] {
        let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
    }
}

#[test]
fn fragment_copy_remaps_both_axial_references() {
    let mut structure = parse("(1P)-[1,1'-binaphthalene]-2,2'-diol");
    let original = structure.fragment;
    let copied = structure
        .graph
        .copy_and_relabel_fragment(original, 1)
        .unwrap();
    let original_bond = structure
        .graph
        .fragment(original)
        .bonds
        .iter()
        .map(|&id| structure.graph.bond(id))
        .find(|bond| bond.axial_stereo.is_some())
        .unwrap();
    let original_stereo = original_bond.axial_stereo.as_ref().unwrap();
    let original_atoms = &structure.graph.fragment(original).atoms;
    let copied_atoms = &structure.graph.fragment(copied).atoms;
    let mapped = |atom| copied_atoms[original_atoms.iter().position(|&id| id == atom).unwrap()];
    let copied_bond = structure
        .graph
        .bond_between(mapped(original_bond.from), mapped(original_bond.to))
        .unwrap();
    assert_eq!(
        structure.graph.bond(copied_bond).axial_stereo,
        Some(AxialStereo {
            refs: original_stereo.refs.map(mapped),
            clockwise: original_stereo.clockwise,
            descriptor: original_stereo.descriptor.clone(),
        })
    );
}

#[test]
fn ordinary_semantic_exports_are_unchanged_and_have_no_axes() {
    let structure = parse("ethanol");
    let (text, locants, axes) = structure.semantic_cxsmiles_with_locants_and_axes().unwrap();
    assert!(axes.is_empty());
    assert_eq!(text, structure.semantic_cxsmiles().unwrap());
    assert_eq!(
        (text, locants),
        structure.semantic_cxsmiles_with_locants().unwrap()
    );
}
