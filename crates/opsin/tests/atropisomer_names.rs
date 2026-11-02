use opsin::{
    Axis, ParseOptions, Parser, Status, Structure,
    cip::CipSequenceRules,
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
    assert_eq!(axis.clockwise, expected == "P", "{name}");
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
    // The graph stores the high-CIP (last) ligand, rather than merely choosing
    // whichever ligand happens to be encountered first.
    let stereo = bond.axial_stereo.as_ref().unwrap();
    for (end, partner, reference) in [
        (bond.from, bond.to, stereo.refs[0]),
        (bond.to, bond.from, stereo.refs[1]),
    ] {
        let order = CipSequenceRules::for_axis(&structure.graph, end)
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(partner)
            .unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order.last(), Some(&reference));
    }
    let mut native = reshiki_chemistry::smiles::read(&text)
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
        reshiki_chemistry::stereo::cip::label::assign(&native, &Default::default()).unwrap();
    assert_eq!(
        labelled.state.properties.bond_codes[native_bond].as_deref(),
        Some(expected),
        "{name}: independent CIP disagreement"
    );
    (text, locants, axis)
}

#[test]
fn axial_cip_includes_the_root_duplicate_for_a_naphthyl_ligand() {
    let structure = parse("(2P)-2-[2-(hydroxymethyl)naphthalen-1-yl]-3,5-dimethylphenol");
    let bond = structure
        .graph
        .fragment(structure.fragment)
        .bonds
        .iter()
        .map(|&id| structure.graph.bond(id))
        .find(|bond| bond.axial_stereo.is_some())
        .unwrap();
    let (end, partner) = if structure
        .graph
        .atom(bond.from)
        .locants
        .contains(&"1".into())
    {
        (bond.from, bond.to)
    } else {
        (bond.to, bond.from)
    };
    let ordinary = CipSequenceRules::new(&structure.graph, end)
        .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(partner)
        .unwrap();
    let axial = CipSequenceRules::for_axis(&structure.graph, end)
        .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(partner)
        .unwrap();
    assert_ne!(ordinary, axial);
    assert!(
        structure
            .graph
            .atom(*axial.last().unwrap())
            .locants
            .contains(&"2".into())
    );
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
            clockwise: true,
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
