//! Binary salt names and monatomic metal cations, outside the organic parent engine.
use super::{Name, molecule::Molecule, name_smiles_with_style};
use crate::naming::{IsotopeStyle, NameReason, NameStatus};
use std::collections::BTreeMap;

pub(super) fn monatomic_cation(smiles: &str, in_salt: bool) -> Result<Option<Name>, String> {
    let parsed = crate::chemistry::smiles::prepare(smiles).map_err(|e| e.to_string())?;
    let [atom] = parsed.state.graph.atoms.as_slice() else {
        return Ok(None);
    };
    if atom.charge <= 0
        || atom.isotope != 0
        || atom.radical_electrons != 0
        || atom.explicit_hydrogens != 0
    {
        return Ok(None);
    }
    let (name, common_charge) = match atom.atomic_number {
        3 => ("lithium", Some(1)),
        4 => ("beryllium", Some(2)),
        11 => ("sodium", Some(1)),
        12 => ("magnesium", Some(2)),
        19 => ("potassium", Some(1)),
        20 => ("calcium", Some(2)),
        26 => ("iron", None),
        27 => ("cobalt", None),
        28 => ("nickel", None),
        29 => ("copper", None),
        30 => ("zinc", Some(2)),
        37 => ("rubidium", Some(1)),
        38 => ("strontium", Some(2)),
        47 => ("silver", Some(1)),
        48 => ("cadmium", Some(2)),
        55 => ("caesium", Some(1)),
        56 => ("barium", Some(2)),
        80 => ("mercury", None),
        82 => ("lead", None),
        87 => ("francium", Some(1)),
        88 => ("radium", Some(2)),
        _ => return Ok(None),
    };
    Ok(Some(Name {
        text: if in_salt && common_charge == Some(atom.charge) {
            name.into()
        } else {
            format!("{name}({}+)", atom.charge)
        },
        status: NameStatus::Preferred,
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    }))
}

/// Assess each anion before a salt can inherit its preference status.
pub(super) fn assess_anion_parent(mol: &Molecule, text: &str) {
    // P-73.6: azaniumyl, not ammonio, is the preferred cation prefix, as on a
    // zwitterion whose skeletal anion is the parent.
    if text.contains("ammonio")
        && mol
            .atoms
            .values()
            .any(|atom| atom.charge < 0 && matches!(atom.symbol.as_str(), "C" | "N"))
    {
        mol.mark_name(NameStatus::Unverified(NameReason::AnionParent));
        return;
    }
    // Carbon-free anions take retained or additive names, not a P-72.7 parent.
    if mol.atoms.values().all(|atom| atom.symbol != "C")
        || mol.atoms.values().map(|atom| atom.charge).sum::<i32>() >= 0
    {
        return;
    }
    // Only the outer parent citation proves an anionic parent: an ionic
    // suffix inside a bracketed substituent does not name the parent.
    let mut depth = 0usize;
    let parent: String = text
        .chars()
        .filter(|ch| match ch {
            '(' | '[' | '{' => {
                depth += 1;
                false
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect();
    let words: Vec<_> = parent
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    let last = words.last().copied().unwrap_or_default();
    // A skeletal anion can precede a neutral suffix, as in
    // 1H-pyrrol-1-ide-3-carboxylic acid.
    let anionic_parent = last.ends_with("ate")
        || last.ends_with("ite")
        || words.iter().any(|word| {
            word.ends_with("ide")
                && ![
                    "amide",
                    "imide",
                    "hydrazide",
                    "oxide",
                    "sulfide",
                    "selenide",
                    "telluride",
                ]
                .iter()
                .any(|neutral| word.ends_with(neutral))
        });
    // P-72.6.1: the preferred prefix for -C(=O)O- is carboxylato.
    if last.ends_with("olate") || !anionic_parent || text.contains("oxidocarbonyl") {
        mol.mark_name(NameStatus::Unverified(NameReason::AnionParent));
    }
}

pub(super) fn name_salt(smiles: &str, style: IsotopeStyle) -> Result<Name, String> {
    let mut ions = BTreeMap::new();
    let mut charge = 0i32;
    for component in smiles.split('.') {
        let state = crate::chemistry::smiles::prepare(component)
            .map_err(|e| e.to_string())?
            .state;
        let positive = state.graph.atoms.iter().any(|a| a.charge > 0);
        let negative = state.graph.atoms.iter().any(|a| a.charge < 0);
        let q: i32 = state.graph.atoms.iter().map(|a| i32::from(a.charge)).sum();
        if q == 0
            || positive && negative
            || state.graph.atoms.iter().any(|a| a.radical_electrons != 0)
        {
            return Err("Salt naming requires charged components without zwitterions or radicals; neutral components and mixtures are unsupported".into());
        }
        charge += q;
        let name = match monatomic_cation(component, true)? {
            Some(name) => name,
            None => name_smiles_with_style(component, style)?,
        };
        let order: String = name
            .text
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let entry = ions
            .entry((q < 0, order, name.text.clone(), component))
            .or_insert((name, 0));
        entry.1 += 1;
    }
    if charge != 0 {
        return Err(
            "Salt naming requires a charge-balanced set of ions; the total charge must be zero"
                .into(),
        );
    }
    let mut status = NameStatus::Preferred;
    let mut words = Vec::new();
    for (_, (name, count)) in ions {
        if status == NameStatus::Preferred && name.status != NameStatus::Preferred {
            status = name.status;
        }
        let complex = name.text.chars().any(|c| !c.is_ascii_alphabetic())
            || super::formatting::starts_with_multiplier(&name.text)
            || name.text.ends_with("aminium");
        let word = if count == 1 {
            name.text
        } else {
            let prefix = if complex {
                super::rules::multipliers::complex_(count)
            } else {
                super::rules::multipliers::basic(count)
            };
            if complex {
                format!("{prefix}({})", name.text)
            } else {
                format!("{prefix}{}", name.text)
            }
        };
        words.push(word);
    }
    Ok(Name {
        text: super::nest_enclosing_marks(&words.join(" ")),
        status,
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charged_substituents_do_not_certify_a_neutral_outer_parent() {
        for (smiles, text) in [
            (
                "Oc1ccccc1-c1[n-]ccc1C",
                "2-(3-methyl-1H-pyrrol-1-ide-2-yl)phenol",
            ),
            ("Oc1ccccc1[CH2-]", "2-methanidylphenol"),
            ("OCCc1cc[n-]c1", "2-(1H-pyrrol-1-ide-3-yl)ethan-1-ol"),
            (
                "OC(=O)c1ccccc1-c1[n-]ccc1",
                "2-(1H-pyrrol-1-ide-2-yl)benzoic acid",
            ),
        ] {
            let mol = super::super::graph_io::read_smiles(smiles).unwrap();
            assess_anion_parent(&mol, text);
            assert_eq!(
                mol.name_status(),
                NameStatus::Unverified(NameReason::AnionParent),
                "{text}"
            );
        }
    }

    #[test]
    fn outer_skeletal_anion_citations_can_precede_a_neutral_suffix() {
        for (smiles, text) in [
            ("O=C(O)c1cc[n-]c1", "1H-pyrrol-1-ide-3-carboxylic acid"),
            ("Oc1ccccc1[CH2-]", "(2-hydroxyphenyl)methanide"),
            (
                "[O-]c1ccccc1Cc1ccccc1C(=O)[O-]",
                "2-[(2-oxidophenyl)methyl]benzoate",
            ),
        ] {
            let mol = super::super::graph_io::read_smiles(smiles).unwrap();
            assess_anion_parent(&mol, text);
            assert_eq!(mol.name_status(), NameStatus::Preferred, "{text}");
        }
    }

    #[test]
    fn sulfonium_ylide_status_follows_substituent_anions() {
        for (smiles, expected, status) in [
            (
                "C[S+](C)[CH-]c1cc([O-])ccc1[O-]",
                "(dimethylsulfaniumyl)2,5-dioxidophenylmethanide",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "C[S+](C)[CH-]c1ccc([N+](=O)[O-])cc1",
                "(dimethylsulfaniumyl)4-nitrophenylmethanide",
                NameStatus::Preferred,
            ),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected);
            assert_eq!(name.status, status, "{smiles}");
        }
    }

    #[test]
    fn unsupported_anionic_parent_examples_still_decline() {
        for smiles in [
            "[Na+].Oc1ccccc1-[n-]1cccc1",
            "[Na+].Oc1ccccc1-n1cccc1",
            "[Na+].Oc1ccccc1[C-]1C=CC=C1",
            "[Na+].OCC[CH2-]",
        ] {
            assert!(super::super::name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn anionic_parents_precede_neutral_groups_and_roundtrip_complete_salts() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected, status) in [
            (
                "[Na+].Oc1ccccc1-c1[n-]ccc1C",
                "sodium 2-(2-hydroxyphenyl)-3-methyl-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "Oc1ccccc1-c1[n-]ccc1C",
                "2-(2-hydroxyphenyl)-3-methyl-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            ("[n-]1cccc1", "1H-pyrrol-1-ide", NameStatus::Preferred),
            (
                "[Na+].Cc1cc[n-]c1",
                "sodium 3-methyl-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].Oc1ccccc1Cc1cc[n-]c1",
                "sodium 3-[(2-hydroxyphenyl)methyl]-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].OC(=O)c1ccccc1-c1[n-]ccc1",
                "sodium 2-(2-carboxyphenyl)-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].Oc1ccccc1[CH2-]",
                "sodium (2-hydroxyphenyl)methanide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].Oc1ccc(cc1)[CH2-]",
                "sodium (4-hydroxyphenyl)methanide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].OC(=O)c1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-carboxyphenyl)methyl]benzen-1-olate",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].OC(=O)c1ccccc1Cc1ccccc1C(=O)[O-]",
                "sodium 2-[(2-carboxyphenyl)methyl]benzoate",
                NameStatus::Preferred,
            ),
            (
                "[Na+].[O-]C(=O)c1ccccc1Cc1ccccc1O",
                "sodium 2-[(2-hydroxyphenyl)methyl]benzoate",
                NameStatus::Preferred,
            ),
            (
                "[Na+].[O-]C(=O)Cc1ccccc1O",
                "sodium (2-hydroxyphenyl)acetate",
                NameStatus::Preferred,
            ),
            (
                "[Na+].Oc1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-hydroxyphenyl)methyl]benzen-1-olate",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].NC(=O)c1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-carbamoylphenyl)methyl]benzen-1-olate",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].O=Cc1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-formylphenyl)methyl]benzen-1-olate",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].CC(=O)c1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-acetylphenyl)methyl]benzen-1-olate",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].[Na+].[O-]c1ccccc1Cc1ccccc1C(=O)[O-]",
                "disodium 2-[(2-oxidophenyl)methyl]benzoate",
                NameStatus::Preferred,
            ),
            (
                "[Na+].OCCc1cc[n-]c1",
                "sodium 3-(2-hydroxyethyl)-1H-pyrrol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].Oc1ccccc1-c1nc[n-]c1",
                "sodium 4-(2-hydroxyphenyl)-1H-imidazol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].[n-]1nnnc1-c1ccc(O)cc1",
                "sodium 5-(4-hydroxyphenyl)-1H-tetrazol-1-ide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].[n-]1nnnc1C(=O)O",
                "sodium 1H-tetrazol-1-ide-5-carboxylic acid",
                NameStatus::Preferred,
            ),
            (
                "[O-]C(=O)c1ccccc1-c1[n-]cc(C(=O)[O-])c1",
                "5-[2-(oxidocarbonyl)phenyl]-1H-pyrrol-1-ide-3-carboxylate",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[NH3+]CC1[N-]C(=O)NC1=O",
                "5-(ammoniomethyl)imidazolidin-1-ide-2,4-dione",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            ("CC(=O)[O-]", "acetate", NameStatus::Preferred),
            (
                "O=C([O-])CCC(=O)O",
                "3-carboxypropanoate",
                NameStatus::Preferred,
            ),
            (
                "[Na+].O=C([O-])CCC(=O)O",
                "sodium 3-carboxypropanoate",
                NameStatus::Preferred,
            ),
        ] {
            let name = super::super::name_smiles(smiles)
                .unwrap_or_else(|error| panic!("{smiles}: {error}"));
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{}: {}",
                name.text,
                parsed.message
            );
            let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &reconstructed).unwrap_or_else(|error| {
                panic!("{smiles} -> {} -> {reconstructed}: {error}", name.text)
            });
        }
    }

    #[test]
    fn hydroxy_anion_parent_preference_is_unverified() {
        let name = super::super::name_smiles("C[O-]").unwrap();
        assert_eq!(name.text, "methanolate");
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn acid_anion_parent_precedes_the_neutral_acid() {
        let name = super::super::name_smiles("O=C([O-])CCC(=O)O").unwrap();
        assert_eq!(name.text, "3-carboxypropanoate");
        assert_eq!(name.status, NameStatus::Preferred);
    }

    #[test]
    fn salt_inherits_hydroxy_anion_parent_uncertainty() {
        let name = super::super::name_smiles("[Na+].C[O-]").unwrap();
        assert_eq!(name.text, "sodium methanolate");
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn salt_inherits_the_preferred_acid_anion_parent() {
        let name = super::super::name_smiles("[Na+].O=C([O-])CCC(=O)O").unwrap();
        assert_eq!(name.text, "sodium 3-carboxypropanoate");
        assert_eq!(name.status, NameStatus::Preferred);
    }

    #[test]
    fn ordinary_anion_and_cation_names_keep_preferred_status() {
        for (smiles, expected) in [
            ("CC(=O)[O-]", "acetate"),
            ("O=S(=O)([O-])[O-]", "sulfate"),
            ("[Cl-]", "chloride"),
            ("[N-]=[N+]=[N-]", "azide"),
            ("[O-]N=O", "nitrite"),
            ("C[NH3+]", "methanaminium"),
            ("CC(=O)O", "acetic acid"),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
        }
    }

    #[test]
    fn balanced_salts_roundtrip_as_complete_graphs() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("CC(=O)[O-].[Na+]", "sodium acetate"),
            ("C[NH3+].[Cl-]", "methanaminium chloride"),
            ("[Na+].[Na+].O=S(=O)([O-])[O-]", "disodium sulfate"),
            ("[Na+].[K+].O=S(=O)([O-])[O-]", "potassium sodium sulfate"),
            ("[Ca+2].[Cl-].[Cl-]", "calcium dichloride"),
            ("[Ca+2].CC(=O)[O-].CC(=O)[O-]", "calcium diacetate"),
            (
                "C[NH3+].C[NH3+].O=S(=O)([O-])[O-]",
                "bis(methanaminium) sulfate",
            ),
            ("[Fe+2]", "iron(2+)"),
            ("[Na+]", "sodium(1+)"),
            ("[Ca+2]", "calcium(2+)"),
        ] {
            let name =
                super::super::name_smiles(smiles).unwrap_or_else(|e| panic!("{smiles}: {e}"));
            assert_eq!(name.text, expected);
            assert_eq!(name.status, NameStatus::Preferred);
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{}: {}",
                name.text,
                parsed.message
            );
            let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &reconstructed)
                .unwrap_or_else(|e| panic!("{smiles} -> {} -> {reconstructed}: {e}", name.text));
        }
    }

    #[test]
    fn mixtures_unbalanced_sets_and_zwitterions_are_refused() {
        for smiles in [
            "C.C",
            "CCO.[Na+].[Cl-]",
            "[Na+].[Na+].[Cl-]",
            "[NH3+]CC(=O)[O-].[Na+].[Cl-]",
        ] {
            assert!(super::super::name_smiles(smiles).is_err(), "{smiles}");
        }
    }
}
