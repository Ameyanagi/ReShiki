//! Native Rust port of openclatura 0.4.3 (MIT), plain-name pipeline.
//! Attribution and license terms are in `licenses/openclatura/`.
//!
//! Supports chains, monocycles, retained and systematic fused parents, pure
//! spiro and von Baeyer ring systems, with independent reconstruction proofs.
// Direct graph/path indexing preserves upstream invariants and panic behavior;
// JSON Value indexing preserves null for missing optional fields.
#![cfg_attr(not(test), allow(clippy::indexing_slicing))]

mod additive;
mod assembler;
mod assembly_charge;
mod assembly_parent;
mod assembly_parts;
mod assembly_prefixes;
mod assembly_spiro;
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
mod heteroatom_subgraphs;
mod identity_validation;
mod ionic_naming;
mod locant_elision;
mod locants;
mod molecule;
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
mod ring_parent;
mod ring_renderer;
mod rules;
mod small_ring_stereo;
mod special_cases;
mod spiro_subgraph;
mod subgraph_tools;
mod von_baeyer;

/// The naming profile on the worker wire and in generated-name warnings.
pub(super) const PROFILE: &str = "openclatura 0.4.3 Rust port";

/// A generated name and how it relates to the preferred IUPAC name.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Name {
    pub text: String,
    pub status: crate::naming::NameStatus,
}

pub(super) fn name_smiles(smiles: &str) -> Result<Name, String> {
    let mol = graph_io::read_smiles(smiles)?;
    if mol.atoms.values().any(|atom| atom.isotope.is_some()) {
        return Err("openclatura: isotopic naming not ported".into());
    }
    // Match upstream require_validated_chalcogens before any shortcut can consume Po.
    if mol.atoms.values().any(|atom| atom.symbol == "Po") {
        return Err("openclatura: polonium nomenclature is not validated".into());
    }
    ionic_naming::validate_heteroatom_oxo_charge_states(&mol)?;
    // Upstream divergence: nitrogen stereocentres and ring stereocentres with a
    // lone pair (for example three single bonds on P, As, S+) read back through OPSIN as
    // another stereoisomer, or lose their descriptor.
    let cyclic = chains::get_cyclic_atoms(&mol, &molecule::AtomSet::new());
    if mol.atoms.values().any(|atom| {
        atom.has_stereo()
            && (atom.symbol == "N"
                || !atom.is_carbon()
                    && cyclic.contains(&atom.idx)
                    && mol.degree(atom.idx) == 3
                    && has_nonbonding_pair(atom, 3)
                    && mol
                        .get_neighbors(atom.idx)
                        .iter()
                        .all(|&n| mol.bond_order(atom.idx, n) == 1))
    }) {
        return Err("openclatura: N and ring lone-pair stereocentres are not validated".into());
    }
    let components = graph_io::get_connected_components(&mol);
    if components.len() != 1 {
        return Err("openclatura: expected one connected component".into());
    }
    let text = component_namer::name_component(&mol, &components[0])?;
    Ok(Name {
        text: nest_enclosing_marks(&text),
        status: mol.name_status(),
    })
}

/// Rewrite the namer's enclosing parentheses in the IUPAC nesting order
/// `{[({[( )]})]}` (P-16.5.4), choosing each mark from what it encloses.
///
/// Square brackets (fusion, von Baeyer, spiro descriptors), the braces of
/// superscripts such as `1^{3,7}` and the parentheses of indicated or added
/// hydrogen are part of the parent name and do not count (P-16.5.4.1.1,
/// P-16.5.4.1.2). Any other parenthesis, including stereodescriptors and
/// compound locants, counts as a level (P-16.5.4.1.3), and a mark that would
/// directly follow or precede one of the same kind moves up a level
/// (P-16.5.4.1.5).
pub(super) fn nest_enclosing_marks(name: &str) -> String {
    // Per open group: its start, the deepest counted level inside it, and the
    // levels of counted groups that start right after it opens and end right
    // before it closes.
    struct Group {
        open: usize,
        inner: usize,
        first: Option<usize>,
        last: Option<(usize, usize)>,
    }
    let mut out = name.to_owned();
    let mut stack: Vec<Group> = Vec::new();
    for (i, byte) in name.bytes().enumerate() {
        match byte {
            b'(' | b'[' => stack.push(Group {
                open: i,
                inner: 0,
                first: None,
                last: None,
            }),
            close @ (b')' | b']') => {
                let Some(group) = stack.pop() else { continue };
                // Indicated or added hydrogen such as `(4aH,9H)` does not count.
                let counted = close == b')'
                    && !name[group.open + 1..i].split(',').all(|locant| {
                        locant.strip_suffix('H').is_some_and(|locant| {
                            let digits = locant
                                .trim_end_matches(|c: char| c == '\'' || c.is_ascii_lowercase());
                            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                        })
                    });
                let mut level = group.inner + usize::from(counted);
                if counted {
                    let last = group.last.filter(|&(end, _)| end + 1 == i).map(|(_, l)| l);
                    while [group.first, last]
                        .into_iter()
                        .flatten()
                        .any(|l| l % 3 == level % 3)
                    {
                        level += 1;
                    }
                    let mark = (level - 1) % 3;
                    out.replace_range(group.open..=group.open, ["(", "[", "{"][mark]);
                    out.replace_range(i..=i, [")", "]", "}"][mark]);
                }
                if let Some(parent) = stack.last_mut() {
                    parent.inner = parent.inner.max(level);
                    if counted {
                        if group.open == parent.open + 1 {
                            parent.first = Some(level);
                        }
                        parent.last = Some((i, level));
                    }
                }
            }
            _ => {}
        }
    }
    out
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
    use super::{name_smiles, nest_enclosing_marks};
    use std::io::Write;

    #[test]
    fn enclosing_marks_nest_by_what_they_enclose() {
        for (smiles, expected) in [
            (
                "CC(C)CC1=CC=C(C=C1)C(C)C(=O)O",
                "2-[4-(2-methylpropyl)phenyl]propanoic acid",
            ),
            (
                "OCCOCCOCCOCCO",
                "2-{2-[2-(2-hydroxyethoxy)ethoxy]ethoxy}ethan-1-ol",
            ),
            (
                "OCCOCCOCCOCCOCCO",
                "2-(2-{2-[2-(2-hydroxyethoxy)ethoxy]ethoxy}ethoxy)ethan-1-ol",
            ),
            (
                "CN(CC(=O)O)Nc1cnc(CO)cn1",
                "2-({[5-(hydroxymethyl)pyrazin-2-yl]amino}(methyl)amino)acetic acid",
            ),
            (
                "C[S@@](=O)CSCNN",
                "1-[({[(R)-methanesulfinyl]methyl}sulfanyl)methyl]hydrazine",
            ),
            // Stereodescriptors and compound locants count as a level; fusion
            // and von Baeyer brackets do not (P-16.5.4.1.2, P-16.5.4.1.3).
            ("C/C=C/Cc1ccc(O)cc1", "4-[(2E)-but-2-en-1-yl]phenol"),
            (
                "CN(CCO)[C@H]1CCCN1",
                "2-{methyl[(2S)-pyrrolidin-2-yl]amino}ethan-1-ol",
            ),
            (
                "OC(=O)CC1CC2CCC1C2",
                "2-(bicyclo[2.2.1]heptan-2-yl)acetic acid",
            ),
            (
                "CC(=O)NC12C=C1BNC2",
                "N-[3-aza-2-borabicyclo[3.1.0]hex-1(6)-en-5-yl]acetamide",
            ),
            (
                "OC(=O)Cc1ccc(cc1)-c1c[nH]c2ncccc12",
                "2-[4-(1H-pyrrolo[2,3-b]pyridin-3-yl)phenyl]acetic acid",
            ),
            (
                "CC(=O)N[C@@H]1CCN2C=CN[C@H]12",
                "N-[(7R,7aS)-5,6,7,7a-tetrahydro-1H-pyrrolo[1,2-a]imidazol-7-yl]acetamide",
            ),
        ] {
            assert_eq!(name_smiles(smiles).unwrap().text, expected, "{smiles}");
        }
    }

    /// The P-16.5.4 examples, given as the namer writes them before nesting.
    #[test]
    fn enclosing_marks_follow_the_blue_book_examples() {
        for (enclosed, expected) in [
            (
                "4,4'-(3-((5-((4'-cyano[1,1'-biphenyl]-4-yl)oxy)pentyl)oxy)-3-oxopropane-1,2-diyl)dibenzoic acid",
                "4,4'-[3-({5-[(4'-cyano[1,1'-biphenyl]-4-yl)oxy]pentyl}oxy)-3-oxopropane-1,2-diyl]dibenzoic acid",
            ),
            (
                "1-(3,4-dihydroquinolin-1(2H)-yl)ethan-1-one",
                "1-(3,4-dihydroquinolin-1(2H)-yl)ethan-1-one",
            ),
            (
                "10,10'-([1,1'-biphenyl]-4,4'-diylbis(oxy))di(decanoic acid)",
                "10,10'-[[1,1'-biphenyl]-4,4'-diylbis(oxy)]di(decanoic acid)",
            ),
            (
                "1-(tricyclo[3.3.1.1^{3,7}]decan-1-yl)ethan-1-one",
                "1-(tricyclo[3.3.1.1^{3,7}]decan-1-yl)ethan-1-one",
            ),
            (
                "2-(bicyclo[6.6.1]pentadeca-8(15)-en-1-yl)ethan-1-ol",
                "2-[bicyclo[6.6.1]pentadeca-8(15)-en-1-yl]ethan-1-ol",
            ),
            // P-16.5.4.1.5: `((2S)-` would put two parentheses together.
            (
                "(3S)-2-((2S)-2-(((2S)-1-ethoxy-1-oxo-4-phenylbutan-2-yl)amino)propanoyl)-1,2,3,4-tetrahydroisoquinoline-3-carboxylic acid",
                "(3S)-2-[(2S)-2-{[(2S)-1-ethoxy-1-oxo-4-phenylbutan-2-yl]amino}propanoyl]-1,2,3,4-tetrahydroisoquinoline-3-carboxylic acid",
            ),
        ] {
            assert_eq!(nest_enclosing_marks(enclosed), expected);
        }
    }

    #[test]
    fn preferred_controls_keep_their_names() {
        use crate::naming::NameStatus;
        for (smiles, expected) in [
            ("CCO", "ethanol"),
            ("O=C(O)c1ccccc1", "benzoic acid"),
            (
                "CC(C)CC1=CC=C(C=C1)C(C)C(=O)O",
                "2-[4-(2-methylpropyl)phenyl]propanoic acid",
            ),
            ("c1ccc2[nH]ccc2c1", "1H-indole"),
            ("C=CCC#C", "pent-1-en-4-yne"),
            ("SS", "disulfane"),
            ("SSS", "trisulfane"),
            ("CC(=O)C#N", "acetyl cyanide"),
            ("COC(=O)Nc1ccccc1", "methyl phenylcarbamate"),
            ("CCC#[N+][O-]", "propanenitrile oxide"),
            (
                "COS(=O)(=O)c1ccc(C(=O)O)cc1",
                "4-(methoxysulfonyl)benzoic acid",
            ),
            ("COS(=O)(=O)NCC(=O)O", "2-methoxysulfonylaminoacetic acid"),
            // Sulfonyl and sulfinyl prefixes are named from the acid (P-65.3.2.2.2).
            ("CS(=O)(=O)c1ccccc1", "(methanesulfonyl)benzene"),
            ("C[S@](=O)c1ccccc1", "[(S)-methanesulfinyl]benzene"),
            ("CS(=O)(=O)N1CCCC1", "1-(methanesulfonyl)pyrrolidine"),
            (
                "O=C(O)c1ccc(cc1)S(=O)(=O)N1CCN(C)CC1",
                "4-(4-methylpiperazine-1-sulfonyl)benzoic acid",
            ),
            (
                "O=C(O)C1CCN(C1)S(=O)(=O)c1ccc(C)cc1",
                "1-(4-methylbenzene-1-sulfonyl)pyrrolidine-3-carboxylic acid",
            ),
            (
                "CC(NS(=O)(=O)c1ccc(C)cc1)C(=O)O",
                "2-(4-methylbenzene-1-sulfonamido)propanoic acid",
            ),
            (
                "CS(=O)(=O)OCCC(=O)O",
                "3-[(methanesulfonyl)oxy]propanoic acid",
            ),
            ("OC(=O)CS(=O)(=O)c1ccccc1", "2-(benzenesulfonyl)acetic acid"),
            ("OS(=O)(=O)ON=C1CCCCC1", "N-(sulfooxy)cyclohexanimine"),
            // Substituted anisoles take systematic PINs (P-63.2.3).
            ("COc1ccc(Cl)cc1", "1-chloro-4-methoxybenzene"),
            ("COc1ccccc1OC", "1,2-dimethoxybenzene"),
            ("O=CCl", "formyl chloride"),
            ("CNC=O", "N-methylformamide"),
            ("COC=O", "methyl formate"),
            // Chalcogen analogues of oxo and formyl (P-64.6.1, P-66.6.3),
            // multiplied with bis to avoid ambiguity (P-16.3.6).
            ("O=C(O)CC(=S)C", "3-sulfanylidenebutanoic acid"),
            ("O=C1CSC(=S)N1", "2-sulfanylidene-1,3-thiazolidin-4-one"),
            ("CC(=[Te])CC(=[Se])C", "4-tellanylidenepentane-2-selone"),
            (
                "CC(=[Se])CCc1ccc(cc1)C(=O)O",
                "4-(3-selanylidenebutyl)benzoic acid",
            ),
            ("OC(=O)CC(=S)CC(=S)C", "3,5-bis(sulfanylidene)hexanoic acid"),
            ("O=C(O)c1ccc(C=S)cc1", "4-(methanethioyl)benzoic acid"),
            ("O=C(O)c1ccc(C=[Se])cc1", "4-(methaneselenoyl)benzoic acid"),
            ("O=C(O)c1ccc(C=[Te])cc1", "4-(methanetelluroyl)benzoic acid"),
        ] {
            let name = name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
        }
    }

    #[test]
    fn known_departures_keep_their_names_and_report_the_rule_family() {
        use crate::naming::{NameReason as R, NameStatus};
        for (smiles, expected, reason) in [
            ("CCC#N", "propionitrile", R::RetainedGeneralName),
            ("O=C(O)CC(=O)O", "malonic acid", R::RetainedGeneralName),
            ("CC(C)Oc1ccccc1", "isopropoxybenzene", R::AlkoxyPrefix),
            (
                "CN(C(C)C(=O)O)S(=O)(=O)c1ccc(C)cc1",
                "2-(N-methyltosylamino)propanoic acid",
                R::SulfonylPrefix,
            ),
            (
                "OC(=O)CS(=O)(=O)CS(=O)(=O)O",
                "2-(sulfomethylsulfonyl)acetic acid",
                R::SulfonylPrefix,
            ),
            (
                "O=C(O)CNc1ccccc1",
                "2-(phenylamino)acetic acid",
                R::AminoPrefix,
            ),
            ("NC(=N)NCC(=O)O", "2-guanidinoacetic acid", R::AminoPrefix),
            ("CS(C)=O", "dimethyl sulfoxide", R::FunctionalClass),
            ("CN(C)C", "trimethylazane", R::HydrideParent),
            ("CP(C)(C)=O", "trimethylphosphane oxide", R::FunctionalClass),
            ("C#[N+][O-]", "methanenitrile oxide", R::UnretainedParent),
            (
                "CCC(C)=NN=CC",
                "butan-2-one ethan-1-ylidenehydrazone",
                R::FunctionalClass,
            ),
            ("CO[Se]C", "dimethyl selenoperoxide", R::FunctionalClass),
            ("CSC", "dimethylsulfane", R::HydrideParent),
            ("CSSC", "1,2-dimethyldisulfane", R::HydrideParent),
            ("C=O", "methanal", R::UnretainedParent),
            ("CC=O", "ethanal", R::UnretainedParent),
            ("COc1ccccc1", "methoxybenzene", R::UnretainedParent),
            ("NC(=O)C(N)=O", "ethanediamide", R::UnretainedParent),
            (
                "CN(C=O)C(=O)O",
                "1-(N-methylformamido)formic acid",
                R::UnretainedParent,
            ),
            (
                "O1C=NC2=NC=CC=C21",
                "oxazolo[4,5-b]pyridine",
                R::FusionPrefix,
            ),
            ("c1cnc2scnc2c1", "thiazolo[5,4-b]pyridine", R::FusionPrefix),
            ("c1cnc2oncc2c1", "isoxazolo[5,4-b]pyridine", R::FusionPrefix),
            ("O=C(O)Cl", "hydroxyformyl chloride", R::UnretainedParent),
            ("O=C(OC)Cl", "methoxyformyl chloride", R::UnretainedParent),
            (
                "CNC(=O)Cl",
                "1-chloro-N-methylformamide",
                R::UnretainedParent,
            ),
            (
                "O=CO[PH](=O)O",
                "(hydroxyphosphinoyloxy)methanal",
                R::UnretainedParent,
            ),
            (
                "O=C(O)OP(=O)(O)c1ccccn1",
                "1-{[hydroxy(pyridin-2-yl)phosphoryl]oxy}formic acid",
                R::UnretainedParent,
            ),
            (
                "COC(=O)N(C=O)c1ccsc1",
                "methyl [N-(thiophen-3-yl)formamido]formate",
                R::UnretainedParent,
            ),
            (
                "CCNC(=O)CC(=O)NCC",
                "N,N'-diethylpropanediamide",
                R::DiamideLocants,
            ),
            ("C#CCC=C", "pent-4-en-1-yne", R::MultipleBondNumbering),
            ("C#CCCC=C", "hex-5-en-1-yne", R::MultipleBondNumbering),
        ] {
            let name = name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::NotPreferred(reason), "{smiles}");
        }
    }

    #[test]
    fn unchecked_paths_keep_their_names_and_report_the_uncertainty() {
        use crate::naming::{NameReason as R, NameStatus};
        for (smiles, expected, reason) in [
            ("NCC=O", "2-aminoethanal", R::SubstitutedRetainedParent),
            (
                "OC(=O)Cc1ccc(cc1)C(=O)O",
                "2-(4-carboxyphenyl)acetic acid",
                R::PrincipalGroupTie,
            ),
            (
                "NC(=O)CNC(=O)c1ccccc1",
                "2-benzamidoacetamide",
                R::PrincipalGroupTie,
            ),
            (
                "CC[N+](C)(C)[S-]",
                "N,N-dimethylethanamine sulfide",
                R::AmineChalcogenide,
            ),
            (
                "C1=Cc2cc3ccc(cc4nc(cc5ccc(cc1n2)[nH]5)C=C4)[nH]3",
                "porphyrin",
                R::UnauditedParent,
            ),
            (
                "C1=C2CCC(=N2)C=C2CCC(N2)C2CCC(=N2)C=C2CCC1=N2",
                "corrin",
                R::UnauditedParent,
            ),
            (
                "C12C(C=C(C3=CC=CC=C13)O2)=O",
                "1,4-epoxynaphthalen-2(1H)-one",
                R::AtomOrder,
            ),
            (
                "C12=CC=C(C3=CC=CC=C13)O2",
                "1,4-epoxynaphthalene",
                R::BridgedFusedParent,
            ),
            (
                "c1oc2cc3nc4ccccc4nc3cc2c1",
                "benzofuro[5,6-b]benzo[e]pyrazine",
                R::OpsinFusionLayout,
            ),
        ] {
            let name = name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Unverified(reason), "{smiles}");
        }
    }

    #[test]
    fn retained_and_systematic_fused_parents_follow_parent_precedence() {
        for (smiles, expected) in [
            ("c1ccc2ccccc2c1", "naphthalene"),
            ("c1ccc2cc3ccccc3cc2c1", "anthracene"),
            ("c1ccc2[nH]ccc2c1", "1H-indole"),
            ("c1ccc2c(c1)[nH]c1ccccc12", "9H-carbazole"),
            ("C1CCC2=CC=CC=C2C1", "1,2,3,4-tetrahydronaphthalene"),
            ("c1ccc2c(c1)CCN2", "2,3-dihydro-1H-indole"),
            ("c1oc2ncccc2c1", "furo[2,3-b]pyridine"),
            ("C1OCC2COCC12", "tetrahydro-1H,3H-furo[3,4-c]furan"),
        ] {
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
        assert_eq!(
            name_smiles("C1CCC2(C1)Oc1ccccc1O2")
                .map(|name| name.text)
                .as_deref(),
            Ok("spiro[1,3-benzodioxole-2,1'-cyclopentane]"),
        );
    }

    #[test]
    fn angular_oxazole_thiazole_acetic_acid_round_trips() {
        let smiles = "O=C(O)Cc1nc2ccc3ocnc3c2s1";
        let name = name_smiles(smiles).unwrap().text;
        assert_eq!(
            name,
            "2-(thiazolo[4',5':1,2]benzo[3,4-d][1,3]oxazol-7-yl)acetic acid"
        );
        let request = crate::naming::local::parse_request(&name).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&name, &reply).unwrap();
        assert_eq!(
            crate::naming::canonical_smiles(smiles).unwrap(),
            crate::naming::canonical_smiles(&parsed.smiles).unwrap()
        );
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
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn unsupported_spiro_topologies_decline() {
        for smiles in [
            "C1CC12CC23CC34CC4",
            "S123(CC1)(CC2)CC3",
            "[c-]1ccccc1-c1ccccc1",
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
        writeln!(output, "class\tsmiles\texpected\tactual\tstatus\treason").unwrap();
        let (mut exact, mut different, mut declined) = (0, 0, 0);
        for (index, line) in input.lines().filter(|line| !line.is_empty()).enumerate() {
            let (smiles, expected) = line.split_once('\t').expect("SMILES and expected name");
            if std::env::var_os("OPENCLATURA_PARITY_VERBOSE").is_some() {
                eprintln!("parity row {index}: {smiles}");
            }
            match name_smiles(smiles) {
                Ok(actual) if actual.text == expected => exact += 1,
                Ok(actual) => {
                    different += 1;
                    let wire = serde_json::to_value(actual.status).unwrap();
                    let status = wire["status"].as_str().unwrap();
                    let reason = wire["reason"].as_str().unwrap_or("");
                    writeln!(
                        output,
                        "different\t{smiles}\t{expected}\t{}\t{status}\t{reason}",
                        actual.text
                    )
                    .unwrap();
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
