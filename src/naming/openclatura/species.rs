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
    if mol.atoms.values().map(|atom| atom.charge).sum::<i32>() >= 0 {
        return;
    }
    let olate = text.trim_end_matches([')', ']', '}']).ends_with("olate");
    let acid = text
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|word| word == "acid");
    let oxido_acid = text.contains("oxido")
        && mol.atoms.iter().any(|(&id, atom)| {
            atom.symbol == "O"
                && atom.charge < 0
                && mol.degree(id) == 1
                && mol.get_neighbors(id).iter().any(|&center| {
                    mol.bond_order(id, center) == 1
                        && mol.get_neighbors(center).iter().any(|&other| {
                            mol.atoms[&other].symbol == "O" && mol.bond_order(center, other) == 2
                        })
                })
        });
    if olate || acid || oxido_acid {
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
    fn hydroxy_anion_parent_preference_is_unverified() {
        let name = super::super::name_smiles("C[O-]").unwrap();
        assert_eq!(name.text, "methanolate");
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn acid_anion_parent_preference_is_unverified() {
        let name = super::super::name_smiles("O=C([O-])CCC(=O)O").unwrap();
        assert_eq!(name.text, "4-oxido-4-oxobutanoic acid");
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn oxido_prefix_for_deprotonated_acid_is_unverified() {
        let name = super::super::name_smiles("O=C([O-])C#[N+][O-]").unwrap();
        assert!(name.text.contains("oxido"), "{}", name.text);
        assert!(!name.text.contains("acid"), "{}", name.text);
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn salt_inherits_hydroxy_anion_parent_uncertainty() {
        let name = super::super::name_smiles("[Na+].C[O-]").unwrap();
        assert_eq!(name.text, "sodium methanolate");
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn salt_inherits_acid_anion_parent_uncertainty() {
        let name = super::super::name_smiles("[Na+].O=C([O-])CCC(=O)O").unwrap();
        assert_eq!(name.text, "sodium 4-oxido-4-oxobutanoic acid");
        assert_eq!(name.status, NameStatus::Unverified(NameReason::AnionParent));
    }

    #[test]
    fn ordinary_anion_and_cation_names_keep_preferred_status() {
        for (smiles, expected) in [
            ("CC(=O)[O-]", "acetate"),
            ("O=S(=O)([O-])[O-]", "sulfate"),
            ("[Cl-]", "chloride"),
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
