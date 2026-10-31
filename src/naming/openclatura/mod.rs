//! Native Rust port of openclatura 0.4.3 (MIT), plain-name pipeline.
//! Attribution and license terms are in `licenses/openclatura/`.
//!
//! Supports one connected component whose ring systems are separate monocycles.
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
mod principal_groups;
mod principal_suffixes;
mod rules;
mod small_ring_stereo;
mod special_cases;
mod subgraph_tools;

pub fn name_smiles(smiles: &str) -> Result<String, String> {
    let mol = graph_io::read_smiles(smiles)?;
    if mol.atoms.values().any(|atom| atom.isotope.is_some()) {
        return Err("openclatura: isotopic naming not ported".into());
    }
    // Match upstream require_validated_chalcogens before any shortcut can consume Po.
    if mol.atoms.values().any(|atom| atom.symbol == "Po") {
        return Err("openclatura: polonium nomenclature is not validated".into());
    }
    let components = graph_io::get_connected_components(&mol);
    if components.len() != 1 {
        return Err("openclatura: expected one connected component".into());
    }
    namer::name_component(&mol, &components[0])
}

#[cfg(test)]
mod tests {
    use super::name_smiles;
    use std::io::Write;

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
        ] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
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
