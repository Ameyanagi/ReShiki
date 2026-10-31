//! Native Rust port of openclatura 0.4.3 (MIT), plain-name pipeline.
//! Attribution and license terms are in `licenses/openclatura/`.
//!
//! Supports chains, monocycles, retained fused parents, pure spiro and non-fusion
//! von Baeyer ring systems. Systematic fusion planning is deferred.
// Graph IDs and numbered parent paths are validated at construction and retain
// their identity in induced subgraphs. Direct indexing preserves the upstream
// rule comparisons; immutable JSON Value indexing also returns null for a
// missing optional field. Neither requires speculative recovery values.
#![cfg_attr(not(test), allow(clippy::indexing_slicing))]

mod additive;
mod assembler;
mod assembly_charge;
mod assembly_parent;
mod assembly_parts;
mod assembly_prefixes;
mod assembly_utils;
mod chains;
mod component_group_rules;
mod component_modifiers;
mod component_namer;
mod formatting;
mod functional_prefixes;
mod fused_ion_templates;
mod fusion;
mod graph_io;
mod graph_kernel;
mod group_atom_roles;
mod heteroatom_subgraphs;
mod identity_validation;
mod ionic_naming;
mod locant_elision;
mod locants;
mod molecule;
mod name_assembly;
mod name_postprocessing;
mod namer;
mod nomenclature;
mod numbering;
mod parent_pipeline;
mod parent_selection;
mod perception;
mod polycycle_topology;
mod principal_groups;
mod principal_suffixes;
mod python_int_set;
mod retained_derivative_hydrogen;
mod retained_fused_production;
mod retained_fused_templates;
mod retained_graph_model;
mod retained_name_policy;
mod retained_specs;
mod ring_renderer;
mod rules;
mod small_ring_stereo;
mod special_cases;
mod subgraph_tools;
mod von_baeyer;

pub fn name_smiles(smiles: &str) -> Result<String, String> {
    let mol = graph_io::read_smiles(smiles)?;
    if mol.atoms.values().any(|atom| atom.isotope.is_some()) {
        return Err("openclatura: isotopic naming not ported".into());
    }
    // Match upstream require_validated_chalcogens before any shortcut can consume Po.
    if mol.atoms.values().any(|atom| atom.symbol == "Po") {
        return Err("openclatura: polonium nomenclature is not validated".into());
    }
    // Upstream divergence: nitrogen stereocentres and ring stereocentres with a
    // lone pair (for example three single bonds on P, As, S+) read back through OPSIN as
    // another stereoisomer, or lose their descriptor.
    let cyclic = chains::get_cyclic_atoms(&mol, &molecule::AtomSet::new());
    if mol.atoms.values().any(|atom| {
        (atom.stereo.is_some() || atom.raw_stereo.is_some())
            && (atom.symbol == "N"
                || !atom.is_carbon()
                    && cyclic.contains(&atom.idx)
                    && mol.degree(atom.idx) == 3
                    && has_nonbonding_pair(atom, 3)
                    && mol
                        .get_neighbors(atom.idx)
                        .iter()
                        .all(|&n| mol.get_bond(atom.idx, n).is_some_and(|b| b.order == 1)))
    }) {
        return Err("openclatura: N and ring lone-pair stereocentres are not validated".into());
    }
    let components = graph_io::get_connected_components(&mol);
    if components.len() != 1 {
        return Err("openclatura: expected one connected component".into());
    }
    namer::name_component(&mol, &components[0])
}

/// Count nonbonding electrons on the main-group elements that can carry
/// tetrahedral tags. Attached hydrogens also consume bonding electrons:
/// a three-neighbour Si–H centre has four bonds and no lone pair.
fn has_nonbonding_pair(atom: &molecule::Atom, bond_order_sum: usize) -> bool {
    let outer_electrons = match atom.symbol.as_str() {
        "C" | "Si" | "Ge" | "Sn" | "Pb" => 4,
        "N" | "P" | "As" | "Sb" | "Bi" => 5,
        "O" | "S" | "Se" | "Te" | "Po" => 6,
        "F" | "Cl" | "Br" | "I" | "At" => 7,
        _ => return false,
    };
    outer_electrons
        - atom.charge
        - (bond_order_sum + atom.total_h_count + atom.radical_electrons) as i32
        >= 2
}

#[cfg(test)]
mod tests {
    use super::name_smiles;
    use std::io::Write;

    #[test]
    fn retained_fused_parents_precede_unported_fusion_planning() {
        for (smiles, expected) in [
            ("c1ccc2ccccc2c1", "naphthalene"),
            ("c1ccc2cc3ccccc3cc2c1", "anthracene"),
            ("c1ccc2[nH]ccc2c1", "1H-indole"),
            ("c1ccc2c(c1)[nH]c1ccccc12", "9H-carbazole"),
            ("C1CCC2=CC=CC=C2C1", "1,2,3,4-tetrahydronaphthalene"),
            ("c1ccc2c(c1)CCN2", "2,3-dihydro-1H-indole"),
        ] {
            assert_eq!(name_smiles(smiles).as_deref(), Ok(expected), "{smiles}");
        }
        for smiles in ["c1oc2ncccc2c1", "C1OCC2COCC12", "C1CCC2(C1)Oc1ccccc1O2"] {
            assert_eq!(
                name_smiles(smiles).unwrap_err(),
                "openclatura: systematic fusion not ported",
                "{smiles}"
            );
        }
    }

    #[test]
    fn spiro_parents_and_substituents_follow_oracle_names() {
        for (smiles, expected) in [
            ("C1CCC2(CC1)CCCC2", "spiro[4.5]decane"),
            ("C1CCC2(CC1)OCCO2", "1,4-dioxaspiro[4.5]decane"),
            ("O=C1CCCC12CCCCC2", "spiro[4.5]decan-1-one"),
            ("C1=CC2(C1)CC1(CCC1)C2", "dispiro[3.1.3.1]dec-1-ene"),
            ("C1=CC12NC21CC1", "7-azadispiro[2.0.2.1]hept-1-ene"),
            (
                "CN/C=C\\C1CC2(C1)CCC2",
                "(1Z)-N-methyl-2-(spiro[3.3]heptan-2-yl)ethen-1-amine",
            ),
            (
                "CC1=CC=C(C=C1)N2C(=O)C[C@@]3(C2=O)C=C(NN3)C(=O)OCC4=CC=CC=C4",
                "benzyl (5S)-7-(4-methylphenyl)-6,8-dioxo-1,2,7-triazaspiro[4.4]non-3-ene-3-carboxylate",
            ),
        ] {
            assert_eq!(name_smiles(smiles).as_deref(), Ok(expected), "{smiles}");
        }
    }

    #[test]
    fn unsupported_spiro_topologies_decline() {
        for smiles in [
            "C1CC12CC23CC34CC4",
            "S123(CC1)(CC2)CC3",
            "[c-]1ccccc1-c1ccccc1",
            "C1CCC2(CC1)C1CCCCC1C2",
            "C1CCC2(CC1)CCCC2.C",
            "[13CH2]1CCC2(CC1)CCCC2",
            "C[C@H]1C2(CC2)[C@@H](C)C13CC3",
            "C[C@H]1C2(CC2)[C@H](C)C13CC3",
        ] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn declines_unrendered_group_and_attachment_features() {
        for smiles in [
            "CC(=[Po])[OH]",
            "C/N=N/c1cccnc1",
            "C/N=N\\c1cccnc1",
            "CC(=O)NOC(=O)Cc1nc(C(=O)O)cs1",
            "O=C(O)[C@H](O)[C@@H](O)C(=O)OO",
            "CN(C(=O)[O-])C1(B(O)O)CCC1",
            "COC(=O)NOOC1CCCCC1",
            "O[N@]1CC12CC2",
            "CN(C)[N@]1CC1(C)CO",
            "C[P@]1CCCC1C",
            "C[P@]1CCCC12CCCC2",
            "C[As@]1CCCC1C",
            "C[S@+]1CCCC1C",
        ] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn tetrahedral_group_four_ring_hydrides_have_no_lone_pair() {
        for element in ["Si", "Ge", "Sn"] {
            let smiles = format!("C[{element}@H]1CCCC1F");
            assert!(name_smiles(&smiles).is_ok(), "{smiles}");
        }
        for smiles in ["C[P@]1CCCC1F", "C[As@]1CCCC1F", "C[S@+]1CCCC1F"] {
            assert_eq!(
                name_smiles(smiles).unwrap_err(),
                "openclatura: N and ring lone-pair stereocentres are not validated",
                "{smiles}",
            );
        }
    }

    #[test]
    #[ignore = "development parity against the external openclatura oracle TSV"]
    fn parity_file() {
        let path = std::env::var("OPENCLATURA_PARITY").expect("OPENCLATURA_PARITY TSV path");
        let out =
            std::env::var("OPENCLATURA_PARITY_OUT").expect("OPENCLATURA_PARITY_OUT diff path");
        let input = std::fs::read_to_string(path).expect("read parity input");
        let mut output =
            std::io::BufWriter::new(std::fs::File::create(out).expect("create parity diff"));
        writeln!(output, "class\tsmiles\texpected\tactual").unwrap();
        let (mut exact, mut different, mut declined) = (0, 0, 0);
        for (index, line) in input.lines().filter(|line| !line.is_empty()).enumerate() {
            let (smiles, expected) = line.split_once('\t').expect("SMILES and expected name");
            if std::env::var_os("OPENCLATURA_PARITY_VERBOSE").is_some() {
                eprintln!("parity row {index}: {smiles}");
            }
            match name_smiles(smiles) {
                Ok(actual) if actual == expected => exact += 1,
                Ok(actual) => {
                    different += 1;
                    writeln!(output, "different\t{smiles}\t{expected}\t{actual}").unwrap();
                }
                Err(reason) => {
                    declined += 1;
                    writeln!(output, "declined\t{smiles}\t{expected}\t{reason}").unwrap();
                }
            }
            output.flush().unwrap();
        }
        output.flush().unwrap();
        println!(
            "openclatura parity: exact={exact} different={different} declined={declined} total={}",
            exact + different + declined
        );
    }
}
