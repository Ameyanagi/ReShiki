//! Binary salt names and monatomic metal cations, outside the organic parent engine.
use super::{Name, component_namer::Derivation, molecule::Molecule, name_smiles_with_style};
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
        status: if in_salt {
            NameStatus::Preferred
        } else {
            NameStatus::Unverified(NameReason::UnauditedPath)
        },
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    }))
}

/// P-72.7: assess the anionic parent actually cited by the component pipeline.
pub(super) fn assess_anion_parent(mol: &Molecule, derivation: &Derivation) {
    let Derivation::Plan {
        parent_atoms,
        principal_group,
        retained_anion_name,
        ..
    } = derivation
    else {
        return;
    };
    let anionic_principal = principal_group.as_ref().is_some_and(|(key, atoms)| {
        super::perception::is_anionic_group_key(key)
            && atoms.iter().map(|i| mol.atoms[i].charge).sum::<i32>() < 0
    });
    // Carbon-free anions take retained or additive names, not a P-72.7 parent.
    if mol.atoms.values().all(|atom| atom.symbol != "C")
        || !mol.atoms.values().any(|atom| atom.charge < 0)
    {
        return;
    }
    // Nitro, N-oxide, azido, diazo and isocyano charges express neutral groups rather
    // than anionic centres. A separate cation does not cancel a real centre.
    let neutral_group_atoms: std::collections::BTreeSet<_> =
        super::perception::perceive_groups(mol)
            .into_iter()
            .filter(|group| {
                (matches!(
                    group.key.as_str(),
                    "nitro"
                        | "azido"
                        | "diazo"
                        | "acid_azide"
                        | "ring_acid_azide"
                        | "isocyano"
                        | "acid_isocyanide"
                        | "ring_acid_isocyanide"
                ) || group
                    .descriptor
                    .as_ref()
                    .is_some_and(|descriptor| descriptor.derivative == "zwitterion"))
                    && group
                        .atoms_involved
                        .iter()
                        .map(|i| mol.atoms[i].charge)
                        .sum::<i32>()
                        == 0
            })
            .flat_map(|group| group.atoms_involved)
            .collect();
    let anionic_centre = |i: &usize| mol.atoms[i].charge < 0 && !neutral_group_atoms.contains(i);
    if !mol.atoms.keys().any(anionic_centre) {
        return;
    }
    let principal_atoms = principal_group.as_ref().map(|(_, atoms)| atoms);
    // P-70.2 / P-74.2: only an anionic suffix group cites the charge; a
    // charge-separated or deprotonated neutral class, as in acetamide for
    // acetylazanide, does not. With no characteristic group, the parent cites -ide.
    let anionic_parent = principal_group.as_ref().map_or_else(
        || parent_atoms.iter().any(anionic_centre),
        |(key, _)| {
            anionic_principal
                // A cationic characteristic suffix can accompany skeletal -ide.
                || (matches!(key.as_str(), "aminium" | "iminium" | "diazonio")
                    && parent_atoms.iter().any(anionic_centre))
        },
    );
    let unresolved_alkoxide = matches!(
        retained_anion_name,
        super::assembly_parts::RetainedAnionName::Required { .. }
            | super::assembly_parts::RetainedAnionName::SubstitutedAlkyl
    );
    // P-72.7(a) prefers the parent with more anionic centres; on a tie,
    // (b)-(d) prefer -ide/-uide centres, which are not implemented.
    let cited =
        |i: &usize| parent_atoms.contains(i) || principal_atoms.is_some_and(|a| a.contains(i));
    let centres = |outside: bool, skeletal: bool| {
        mol.atoms
            .iter()
            .filter(|(i, atom)| {
                anionic_centre(i)
                    && cited(i) != outside
                    && (!skeletal || matches!(atom.symbol.as_str(), "C" | "N"))
            })
            .count()
    };
    let external_skeletal_anion =
        centres(true, true) > 0 && centres(true, false) >= centres(false, false);
    // The external carboxylate is spelled by P-72.6.1. Parent preference is
    // still unresolved when P-72.7(a) cannot decide without (b)-(e).
    let external_carboxylate = mol.atoms.iter().any(|(&carbon, atom)| {
        atom.is_carbon()
            && mol.get_neighbors(carbon).iter().any(|&oxo| {
                mol.atoms[&oxo].symbol == "O"
                    && mol.bond_order(carbon, oxo) == 2
                    && mol.get_neighbors(carbon).iter().any(|&oxygen| {
                        mol.atoms[&oxygen].symbol == "O"
                            && mol.atoms[&oxygen].charge < 0
                            && mol.bond_order(carbon, oxygen) == 1
                            && principal_atoms.is_none_or(|atoms| {
                                ![carbon, oxo, oxygen].iter().all(|i| atoms.contains(i))
                            })
                    })
            })
    });
    if !anionic_parent
        || unresolved_alkoxide
        || external_skeletal_anion
        || external_carboxylate && centres(true, false) >= centres(false, false)
    {
        let status = NameStatus::Unverified(NameReason::AnionParent);
        // The unresolved principal anion spelling remains the reason when a
        // separately staged cationic prefix also needs verification.
        if unresolved_alkoxide
            && mol.name_status() == NameStatus::Unverified(NameReason::CationPrefix)
        {
            mol.set_name_status(status);
        } else {
            mol.mark_name(status);
        }
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
        // The strongest ion downgrade wins; ties keep the first ion's reason.
        if name.status.severity() > status.severity() {
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
    fn mixed_salt_anion_spellings_preserve_status() {
        for (smiles, expected, status) in [
            (
                "[Na+].[Na+].Cc1ccc(cc1)C(=O)[O-].CC(=O)[O-]",
                "disodium 4-methylbenzoate acetate",
                NameStatus::Preferred,
            ),
            (
                "[Na+].[Na+].[Na+].CC(=O)CC[O-].O=C([O-])c1ccc(cc1)Oc1ccc(cc1)C(=O)[O-]",
                "trisodium 3-oxobutan-1-olate 4-[(4-carboxylatophenyl)oxy]benzoate",
                NameStatus::NotPreferred(NameReason::Multiplicative),
            ),
        ] {
            let name = name_salt(smiles, IsotopeStyle::Iupac).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
        }
    }

    fn assess_component(smiles: &str) -> NameStatus {
        let mol = super::super::graph_io::read_smiles(smiles).unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let name =
            super::super::component_namer::name_component_with_citation(&mol, &atoms, None, None)
                .unwrap();
        assert!(
            matches!(&name.derivation, Derivation::Plan { .. }),
            "{smiles}"
        );
        // Isolate this assessor from marks made while deriving the name.
        mol.set_name_status(NameStatus::Unverified(NameReason::UnauditedPath));
        assess_anion_parent(&mol, &name.derivation);
        mol.certify();
        mol.name_status()
    }

    #[test]
    fn anion_parent_assessment_uses_the_cited_derivation() {
        for smiles in [
            "[n-]1nnnc1C(=O)O",
            "[O-]C(=O)c1ccccc1Cc1cc[n-]c1",
            "[O-]C(=O)c1ccccc1[CH2-]",
            "C[N+](C)(C)CC[O-]",
            "[NH3+]CC([NH3+])C[O-]",
        ] {
            assert_eq!(
                assess_component(smiles),
                NameStatus::Unverified(NameReason::AnionParent),
                "{smiles}"
            );
        }
    }

    #[test]
    fn neutral_charge_separated_groups_are_not_anionic_centres() {
        for smiles in [
            "[O-][N+](=O)c1ccccc1",
            "C[N+](C)(C)[O-]",
            "CN=[N+]=[N-]",
            "C=[N+]=[N-]",
            "CC#[N+][O-]",
            "CC(=O)N=[N+]=[N-]",
            "C[N+]#[C-]",
            "CC(=O)[N+]#[C-]",
        ] {
            assert_eq!(assess_component(smiles), NameStatus::Preferred, "{smiles}");
        }
    }

    #[test]
    fn chosen_anionic_parents_keep_preferred_status() {
        for smiles in [
            "O=C([O-])c1cc[n-]c1",
            "[O-]C(=O)c1ccccc1-c1[n-]cc(C(=O)[O-])c1",
            "C[O-]",
            "[O-]c1ccccc1",
            "Oc1ccccc1[CH2-]",
            "[O-]c1ccccc1Cc1ccccc1C(=O)[O-]",
            "Oc1ccccc1-c1[n-]ccc1C",
            "OCCc1cc[n-]c1",
            "OC(=O)c1ccccc1-c1[n-]ccc1",
            "[NH3+][C-]1C=CC2=C1N=NO2",
        ] {
            assert_eq!(assess_component(smiles), NameStatus::Preferred, "{smiles}");
        }
    }

    #[test]
    fn retained_anion_assessment_requires_application_evidence() {
        use super::super::assembly_parts::RetainedAnionName;

        let mol = super::super::graph_io::read_smiles("C[O-]").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let mut component =
            super::super::component_namer::name_component_with_citation(&mol, &atoms, None, None)
                .unwrap();
        let Derivation::Plan {
            retained_anion_name,
            ..
        } = &mut component.derivation
        else {
            panic!("expected an assembled retained anion parent");
        };
        assert_eq!(
            retained_anion_name,
            &RetainedAnionName::Applied("methoxide".into())
        );
        // The same displayed text cannot certify a selected but unapplied name.
        *retained_anion_name = RetainedAnionName::Required {
            name: "methoxide".into(),
            absorbed_substituent: None,
        };
        assess_anion_parent(&mol, &component.derivation);
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::AnionParent)
        );
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
                NameStatus::Unverified(NameReason::UnauditedPath),
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
                "sodium 2-[(2-carboxyphenyl)methyl]phenoxide",
                NameStatus::Preferred,
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
                "sodium 2-[(2-hydroxyphenyl)methyl]phenoxide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].NC(=O)c1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-carbamoylphenyl)methyl]phenoxide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].O=Cc1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-formylphenyl)methyl]phenoxide",
                NameStatus::Preferred,
            ),
            (
                "[Na+].CC(=O)c1ccccc1Cc1ccccc1[O-]",
                "sodium 2-[(2-acetylphenyl)methyl]phenoxide",
                NameStatus::Preferred,
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
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].c1ccc2[n-]cc(C(=O)O)c2c1",
                "sodium 1H-indol-1-ide-3-carboxylic acid",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].OC(=O)c1nc[n-]c1",
                "sodium 1H-imidazol-1-ide-4-carboxylic acid",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[Na+].[n-]1ncc(C(=O)O)c1",
                "sodium 1H-pyrazol-1-ide-4-carboxylic acid",
                NameStatus::Unverified(NameReason::AnionParent),
            ),
            (
                "[O-]C(=O)c1ccccc1-c1[n-]cc(C(=O)[O-])c1",
                "5-(2-carboxylatophenyl)-1H-pyrrol-1-ide-3-carboxylate",
                NameStatus::Preferred,
            ),
            (
                "[NH3+]CC1[N-]C(=O)NC1=O",
                "5-(azaniumylmethyl)imidazolidin-1-ide-2,4-dione",
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
    fn retained_alkoxide_parent_is_preferred() {
        let name = super::super::name_smiles("C[O-]").unwrap();
        assert_eq!(name.text, "methoxide");
        assert_eq!(name.status, NameStatus::Preferred);
    }

    #[test]
    fn acid_anion_parent_precedes_the_neutral_acid() {
        let name = super::super::name_smiles("O=C([O-])CCC(=O)O").unwrap();
        assert_eq!(name.text, "3-carboxypropanoate");
        assert_eq!(name.status, NameStatus::Preferred);
    }

    #[test]
    fn salt_inherits_retained_alkoxide_parent_preference() {
        let name = super::super::name_smiles("[Na+].C[O-]").unwrap();
        assert_eq!(name.text, "sodium methoxide");
        assert_eq!(name.status, NameStatus::Preferred);
    }

    #[test]
    fn salt_inherits_the_preferred_acid_anion_parent() {
        let name = super::super::name_smiles("[Na+].O=C([O-])CCC(=O)O").unwrap();
        assert_eq!(name.text, "sodium 3-carboxypropanoate");
        assert_eq!(name.status, NameStatus::Preferred);
    }

    #[test]
    fn organic_ions_and_unaudited_inorganic_names_keep_their_status() {
        for (smiles, expected, status) in [
            ("CC(=O)[O-]", "acetate", NameStatus::Preferred),
            (
                "O=S(=O)([O-])[O-]",
                "sulfate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Cl-]",
                "chloride",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[N-]=[N+]=[N-]",
                "azide",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[O-]N=O",
                "nitrite",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            ("C[NH3+]", "methanaminium", NameStatus::Preferred),
            ("CC(=O)O", "acetic acid", NameStatus::Preferred),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
        }
    }

    #[test]
    fn balanced_salts_roundtrip_as_complete_graphs() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected, status) in [
            ("CC(=O)[O-].[Na+]", "sodium acetate", NameStatus::Preferred),
            (
                "C[NH3+].[Cl-]",
                "methanaminium chloride",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Na+].[Na+].O=S(=O)([O-])[O-]",
                "disodium sulfate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Na+].[K+].O=S(=O)([O-])[O-]",
                "potassium sodium sulfate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Ca+2].[Cl-].[Cl-]",
                "calcium dichloride",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Ca+2].CC(=O)[O-].CC(=O)[O-]",
                "calcium diacetate",
                NameStatus::Preferred,
            ),
            (
                "C[NH3+].C[NH3+].O=S(=O)([O-])[O-]",
                "bis(methanaminium) sulfate",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Fe+2]",
                "iron(2+)",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Na+]",
                "sodium(1+)",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[Ca+2]",
                "calcium(2+)",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
        ] {
            let name =
                super::super::name_smiles(smiles).unwrap_or_else(|e| panic!("{smiles}: {e}"));
            assert_eq!(name.text, expected);
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
            crate::naming::verify_identity(smiles, &reconstructed)
                .unwrap_or_else(|e| panic!("{smiles} -> {} -> {reconstructed}: {e}", name.text));
        }
    }

    /// A salt takes its strongest ion assessment (Preferred < Unverified <
    /// NotPreferred; ties keep the first ion's reason in citation order), so a
    /// known departure is never masked by an unchecked ion (P-77.1.1, P-77.2.1).
    /// Retained anion spellings and salt order must preserve stronger departures.
    #[test]
    fn salt_status_is_the_strongest_ion_downgrade() {
        use NameReason::{BridgedFusedParent, Multiplicative, RingAssembly, UnauditedParent};
        use NameStatus::{NotPreferred, Preferred, Unverified};
        const NTA: &str = "O=C([O-])CN(CC(=O)[O-])CC(=O)[O-]";
        const OXYDIBENZOATE: &str = "O=C([O-])c1ccc(cc1)Oc1ccc(cc1)C(=O)[O-]";
        const EPOXY_AMINIUM: &str = "C[N+](C)(C)c1ccc2c(c1)C1CCC2O1";
        const NONANYL_AMINIUM: &str = "C[N+](C)(C)CC1C2CC3CC1CC2C3";
        const BIS_AMINIUM: &str = "C[N+](C)(C)Cc1ccc(cc1)C[N+](C)(C)C";
        const PENTALENE_CARBOXYLATE: &str = "C1C2CC3CC1CC2C3C(=O)[O-]";
        for (smiles, expected, status) in [
            (
                format!("[Na+].[Na+].[Na+].[Na+].[O-]c1ccccc1.{NTA}"),
                "tetrasodium [bis(carboxylatomethyl)amino]acetate phenoxide",
                NotPreferred(Multiplicative),
            ),
            (
                format!("{NTA}.[O-]c1ccccc1.[Na+].[Na+].[Na+].[Na+]"),
                "tetrasodium [bis(carboxylatomethyl)amino]acetate phenoxide",
                NotPreferred(Multiplicative),
            ),
            (
                format!("{EPOXY_AMINIUM}.[Na+].[Na+].[Na+].C1=CCCCC1-C1=CCCC(C1)C(=O)[O-].{NTA}"),
                "N,N,N-trimethyl-1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-aminium trisodium 3-(cyclohex-2-en-1-yl)cyclohex-3-ene-1-carboxylate [bis(carboxylatomethyl)amino]acetate",
                NotPreferred(RingAssembly),
            ),
            (
                "C[N+](C)(C)C1=CCCCC1-C1=CCCCC1.[Na+].[O-]c1ccccc1.[O-]C(=O)c1ccccc1".into(),
                "6-(cyclohex-1-en-1-yl)-N,N,N-trimethylcyclohex-1-en-1-aminium sodium benzoate phenoxide",
                NotPreferred(RingAssembly),
            ),
            (
                format!("[Na+].[Na+].[Na+].CC(=O)CC[O-].{OXYDIBENZOATE}"),
                "trisodium 3-oxobutan-1-olate 4-[(4-carboxylatophenyl)oxy]benzoate",
                NotPreferred(Multiplicative),
            ),
            (
                format!("[K+].[S-]c1ccccc1.[K+].[K+].[K+].{NTA}"),
                "tetrapotassium benzenethiolate [bis(carboxylatomethyl)amino]acetate",
                NotPreferred(Multiplicative),
            ),
            (
                format!("{EPOXY_AMINIUM}.{EPOXY_AMINIUM}.{OXYDIBENZOATE}"),
                "bis(N,N,N-trimethyl-1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-aminium) 4-[(4-carboxylatophenyl)oxy]benzoate",
                NotPreferred(Multiplicative),
            ),
            (
                "[Na+].[O-]C(=O)c1ccccc1".into(),
                "sodium benzoate",
                Preferred,
            ),
            ("[Na+].[O-]c1ccccc1".into(), "sodium phenoxide", Preferred),
            (
                "[Na+].[O-]c1ccccc1.[Na+].[O-]C(=O)c1ccccc1".into(),
                "disodium benzoate phenoxide",
                Preferred,
            ),
            (
                format!("[Na+].[Na+].{OXYDIBENZOATE}"),
                "disodium 4-[(4-carboxylatophenyl)oxy]benzoate",
                NotPreferred(Multiplicative),
            ),
            (
                format!("[O-]c1ccccc1.[O-]c1ccccc1.{BIS_AMINIUM}"),
                "N,N,N-trimethyl-1-{4-[(trimethylammonio)methyl]phenyl}methanaminium diphenoxide",
                NotPreferred(Multiplicative),
            ),
            (
                format!("{PENTALENE_CARBOXYLATE}.{BIS_AMINIUM}.{PENTALENE_CARBOXYLATE}"),
                "N,N,N-trimethyl-1-{4-[(trimethylammonio)methyl]phenyl}methanaminium bis(1,2,3,3a,4,5,6,6a-octahydro-2,5-methanopentalene-1-carboxylate)",
                NotPreferred(Multiplicative),
            ),
            (
                format!("{NONANYL_AMINIUM}.[O-]C(=O)c1ccccc1"),
                "N,N,N-trimethyl-1-(tricyclo[3.3.1.0^{3,7}]nonan-2-yl)methanaminium benzoate",
                Unverified(UnauditedParent),
            ),
            (
                format!("{EPOXY_AMINIUM}.[O-]c1ccccc1"),
                "N,N,N-trimethyl-1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-aminium phenoxide",
                Unverified(BridgedFusedParent),
            ),
            (
                format!("{NONANYL_AMINIUM}.{NONANYL_AMINIUM}.{OXYDIBENZOATE}"),
                "bis[N,N,N-trimethyl-1-(tricyclo[3.3.1.0^{3,7}]nonan-2-yl)methanaminium] 4-[(4-carboxylatophenyl)oxy]benzoate",
                NotPreferred(Multiplicative),
            ),
            ("[O-]c1ccccc1".into(), "phenoxide", Preferred),
            (
                NTA.into(),
                "[bis(carboxylatomethyl)amino]acetate",
                NotPreferred(Multiplicative),
            ),
            ("CC(=O)O".into(), "acetic acid", Preferred),
        ] {
            let name =
                super::super::name_smiles(&smiles).unwrap_or_else(|e| panic!("{smiles}: {e}"));
            assert_eq!(
                (name.text.as_str(), name.status),
                (expected, status),
                "{smiles}"
            );
        }
    }

    #[test]
    fn salt_inherits_the_strongest_ion_status() {
        let name = super::super::name_smiles("[Na+].[Na+].[Cl-].C[O-]").unwrap();
        assert_eq!(name.text, "disodium chloride methoxide");
        assert_eq!(
            name.status,
            NameStatus::Unverified(NameReason::UnauditedPath)
        );
    }

    #[test]
    fn monatomic_cations_are_certified_only_in_salts() {
        for smiles in ["[Na+]", "[Fe+2]", "[Ca+2]"] {
            assert_eq!(
                monatomic_cation(smiles, false).unwrap().unwrap().status,
                NameStatus::Unverified(NameReason::UnauditedPath),
                "{smiles}"
            );
            assert_eq!(
                monatomic_cation(smiles, true).unwrap().unwrap().status,
                NameStatus::Preferred,
                "{smiles}"
            );
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
