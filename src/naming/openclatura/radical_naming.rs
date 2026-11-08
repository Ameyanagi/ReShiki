//! Neutral radicals whose subtractive P-71 names have exact parser identities.
//!
//! https://iupac.qmul.ac.uk/BlueBook/P7.html, P-71.2.1–P-71.3.4.
//! A virtual excluded bond reuses the ordinary substituent renderer without
//! discarding the free-valence order. More elaborate radical characteristic
//! groups and radical ions are deliberately rejected before ordinary naming.
use super::{
    Name,
    formatting::strip_outer_parentheses,
    molecule::{Atom, AtomSet, Bond, Molecule},
};

fn unsupported() -> String {
    "openclatura: this neutral radical's parent or characteristic group is not supported".into()
}

pub(super) fn name_radical(mol: &Molecule) -> Result<Option<Name>, String> {
    let centers: Vec<_> = mol
        .atoms
        .values()
        .filter(|atom| atom.radical_electrons != 0)
        .collect();
    if centers.is_empty() {
        return Ok(None);
    }
    if mol.atoms.values().any(|atom| atom.charge != 0) {
        return Err("openclatura: radical ions are not supported".into());
    }
    if centers
        .iter()
        .map(|atom| atom.radical_electrons)
        .sum::<usize>()
        > 2
        || mol
            .atoms
            .values()
            .any(|atom| atom.has_stereo() || atom.isotope.is_some())
        || mol
            .bonds
            .values()
            .any(|bond| bond.stereo.is_some() || !(1..=3).contains(&bond.order))
        || super::graph_io::get_connected_components(mol).len() != 1
    {
        return Err(unsupported());
    }
    let text = if centers.len() == 2 {
        carbon_diyl(mol)?
    } else {
        let center = centers[0];
        match center.symbol.as_str() {
            "C" if carbon_parent(mol, center.idx, center.radical_electrons == 1) => {
                substituent_radical(mol, center.idx, center.radical_electrons)?
            }
            "N" => amine_radical(mol, center.idx, center.radical_electrons)?,
            "O" | "S" | "Se" if center.radical_electrons == 1 => {
                chalcogen_radical(mol, center.idx)?
            }
            _ => return Err(unsupported()),
        }
    };
    Ok(Some(Name {
        text: super::nest_enclosing_marks(&text),
        status: mol.name_status(),
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    }))
}

/// Only hydrocarbons and a carbonyl at the free carbon are audited here.
fn carbon_parent(mol: &Molecule, root: usize, allow_acyl: bool) -> bool {
    let carbon_degree = |id| {
        mol.get_neighbors(id)
            .iter()
            .filter(|n| mol.atoms[n].is_carbon())
            .count()
    };
    let unbranched = mol
        .atoms
        .values()
        .filter(|atom| atom.is_carbon())
        .all(|atom| carbon_degree(atom.idx) <= 2);
    // The only branched parent audited here is the saturated star centered on
    // the free valence (in particular 2-methylpropan-2-yl). Other branching and
    // ring side chains need a separate P-46/P-71 parent-selection audit.
    let centered_star = mol.atoms.values().all(|atom| atom.is_carbon())
        && mol.bonds.values().all(|bond| bond.order == 1)
        && mol
            .atoms
            .keys()
            .all(|&id| id == root || mol.degree(id) == 1)
        && mol.degree(root) + 1 == mol.atoms.len();
    (unbranched || centered_star)
        && mol.bonds.len() <= mol.atoms.len()
        && mol.atoms.values().all(|atom| {
            atom.is_carbon()
                || allow_acyl
                    && atom.symbol == "O"
                    && mol.degree(atom.idx) == 1
                    && mol.bond_order(root, atom.idx) == 2
                    && atom.total_h_count == 0
        })
}

/// An excluded virtual parent makes the free valence indistinguishable from
/// the attachment bond already handled by alkyl/alkylidene and acyl naming.
fn substituent_radical(mol: &Molecule, root: usize, order: usize) -> Result<String, String> {
    let mut attached = mol.clone();
    for atom in attached.atoms.values_mut() {
        atom.radical_electrons = 0;
    }
    let parent = attached.atoms.keys().next_back().copied().unwrap_or(0) + 1;
    let bond = attached.bonds.keys().next_back().copied().unwrap_or(0) + 1;
    let mut atom = Atom::new(parent, "C");
    atom.total_h_count = 4 - order;
    attached.add_atom(atom)?;
    attached.add_bond(Bond::new(bond, root, parent, order))?;
    let name = super::namer::name_subgraph_with_upstream(
        &attached,
        root,
        &AtomSet::from([parent]),
        Some(parent),
    )?;
    // P-71.2.1.2: 2-methylpropan-2-yl (PIN), not the tert-butyl prefix.
    Ok(match strip_outer_parentheses(&name).as_str() {
        "tert-butyl" => "2-methylpropan-2-yl".into(),
        name => name.into(),
    })
}

fn amine_radical(mol: &Molecule, root: usize, electrons: usize) -> Result<String, String> {
    let atom = &mol.atoms[&root];
    if mol.atoms.len() == 1 {
        return match (atom.total_h_count, electrons) {
            (2, 1) => Ok("azanyl".into()),
            (1, 2) => Ok("azanylidene".into()),
            _ => Err(unsupported()),
        };
    }
    // The pinned parser cannot interpret substituted aminylidene suffixes.
    // Decline these rather than return an unreconstructable preferred form.
    if electrons != 1
        || mol.bonds.len() >= mol.atoms.len()
        || mol
            .atoms
            .values()
            .any(|atom| atom.idx != root && !atom.is_carbon())
        || mol
            .get_neighbors(root)
            .iter()
            .any(|&n| mol.bond_order(root, n) != 1)
        || atom.total_h_count + mol.degree(root) + electrons != 3
    {
        return Err(unsupported());
    }
    let mut capped = mol.clone();
    let center = capped.atoms.get_mut(&root).ok_or_else(unsupported)?;
    center.radical_electrons = 0;
    center.total_h_count += electrons;
    center.explicit_h_count += electrons;
    let component = capped.atoms.keys().copied().collect::<AtomSet>();
    let name = super::component_namer::name_component(&capped, &component)?;
    let stem = name.strip_suffix("amine").ok_or_else(unsupported)?;
    Ok(format!(
        "{stem}{}",
        if electrons == 1 {
            "aminyl"
        } else {
            "aminylidene"
        }
    ))
}

fn chalcogen_radical(mol: &Molecule, root: usize) -> Result<String, String> {
    let atom = &mol.atoms[&root];
    if mol.atoms.len() == 1 {
        return match (atom.symbol.as_str(), atom.total_h_count) {
            ("O", 1) => Ok("hydroxyl".into()),
            ("S", 1) => Ok("sulfanyl".into()),
            ("Se", 1) => Ok("selanyl".into()),
            _ => Err(unsupported()),
        };
    }
    if atom.symbol == "O"
        && mol.atoms.len() == 2
        && atom.total_h_count == 0
        && mol.atoms.values().any(|other| {
            other.idx != root
                && other.symbol == "O"
                && other.total_h_count == 1
                && mol.bond_order(root, other.idx) == 1
        })
    {
        return Ok("hydroperoxyl".into());
    }
    if atom.symbol == "O" && atom.total_h_count == 0 && mol.degree(root) == 1 {
        let nitrogen = mol.get_neighbors(root)[0];
        if mol.atoms[&nitrogen].symbol == "N"
            && mol.bond_order(root, nitrogen) == 1
            && mol
                .get_neighbors(nitrogen)
                .iter()
                .all(|&n| mol.bond_order(nitrogen, n) == 1)
            && mol.degree(nitrogen) + mol.atoms[&nitrogen].total_h_count == 3
            && mol
                .atoms
                .values()
                .all(|other| other.idx == root || other.idx == nitrogen || other.is_carbon())
            && mol.bonds.len() < mol.atoms.len()
        {
            let prefix = super::namer::name_subgraph_with_upstream(
                mol,
                nitrogen,
                &AtomSet::from([root]),
                Some(root),
            )?;
            let prefix = strip_outer_parentheses(&prefix);
            let prefix = super::formatting::phenylamino_spelling(&prefix);
            let stem = prefix.strip_suffix("amino").ok_or_else(unsupported)?;
            return Ok(format!("{stem}aminoxyl"));
        }
    }
    if atom.total_h_count != 0
        || mol.degree(root) != 1
        || mol.bonds.len() > mol.atoms.len()
        || mol
            .get_neighbors(root)
            .iter()
            .any(|&n| mol.bond_order(root, n) != 1)
        || mol
            .atoms
            .values()
            .any(|other| other.idx != root && !other.is_carbon())
    {
        return Err(unsupported());
    }
    let name = substituent_radical(mol, root, 1)?;
    if atom.symbol == "O" {
        // P-71.3.4: methoxy -> methoxyl, including retained ethoxy/phenoxy.
        if !name.ends_with("oxy") {
            return Err(unsupported());
        }
        Ok(format!("{name}l"))
    } else {
        Ok(name)
    }
}

/// The two independent free valences must both receive explicit locants.
/// This path supports unbranched saturated chains, saturated monocycles and
/// benzene; a branch or further characteristic group needs a richer assembly.
fn carbon_diyl(mol: &Molecule) -> Result<String, String> {
    if mol
        .atoms
        .values()
        .any(|atom| !atom.is_carbon() || atom.radical_electrons > 1)
        || mol.atoms.len() < 2
        || mol.atoms.keys().any(|&i| mol.degree(i) > 2)
    {
        return Err(unsupported());
    }
    let cyclic = mol.bonds.len() == mol.atoms.len();
    let aromatic = mol.atoms.values().all(|atom| atom.is_aromatic);
    if !(mol.bonds.values().all(|bond| bond.order == 1)
        || cyclic && aromatic && mol.atoms.len() == 6)
    {
        return Err(unsupported());
    }
    let mut best = None::<Vec<usize>>;
    for &start in mol.atoms.keys().filter(|&&i| cyclic || mol.degree(i) == 1) {
        for &second in mol.get_neighbors(start) {
            let mut path = vec![start, second];
            while path.len() < mol.atoms.len() {
                let current = *path.last().ok_or_else(unsupported)?;
                let previous = path[path.len() - 2];
                let Some(&next) = mol
                    .get_neighbors(current)
                    .iter()
                    .find(|&&n| n != previous && n != start)
                else {
                    break;
                };
                path.push(next);
            }
            if path.len() != mol.atoms.len() {
                continue;
            }
            let locants: Vec<_> = path
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    (mol.atoms[id].radical_electrons == 1).then_some(index + 1)
                })
                .collect();
            if best.as_ref().is_none_or(|current| locants < *current) {
                best = Some(locants);
            }
        }
    }
    let locants = best.ok_or_else(unsupported)?;
    let parent = if aromatic {
        "benzene".into()
    } else {
        format!(
            "{}{}ane",
            if cyclic { "cyclo" } else { "" },
            super::rules::stems::stem_for(mol.atoms.len())
        )
    };
    // P-71.2.3: benzene-1,4-diyl and ethane-1,2-diyl are the PINs.
    Ok(format!("{parent}-{},{}-diyl", locants[0], locants[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{NameReason, NameStatus};

    fn roundtrip(smiles: &str, expected: &str) -> Result<(), String> {
        let mol = super::super::graph_io::read_smiles(smiles, &Default::default())?;
        let name = name_radical(&mol)?.ok_or("Expected radical name")?;
        assert_eq!(name.text, expected, "{smiles}");
        assert_eq!(
            name.status,
            NameStatus::Unverified(NameReason::UnauditedPath),
            "{smiles}"
        );
        let parsed = opsin::Parser::new()
            .map_err(|error| error.to_string())?
            .parse(
                &name.text,
                &opsin::ParseOptions {
                    allow_radicals: true,
                    ..Default::default()
                },
            );
        let structure = parsed
            .structure
            .ok_or_else(|| format!("{}: {}", name.text, parsed.message))?;
        let reconstructed = structure
            .semantic_cxsmiles()
            .map_err(|error| error.to_string())?;
        crate::naming::verify_identity(smiles, &reconstructed)
    }

    #[test]
    fn carbon_monoradicals_and_single_center_diradicals_roundtrip() -> Result<(), String> {
        for (smiles, name) in [
            ("[CH3]", "methyl"),
            ("C[CH2]", "ethyl"),
            ("C[CH]C", "propan-2-yl"),
            ("CCC[CH]CCC", "heptan-4-yl"),
            ("C[CH]CCCC", "hexan-2-yl"),
            ("C[C](C)C", "2-methylpropan-2-yl"),
            ("[CH2]C=C", "prop-2-en-1-yl"),
            ("[CH]1CCCCC1", "cyclohexyl"),
            ("[c]1ccccc1", "phenyl"),
            ("[CH2]", "methylidene"),
            ("C[CH]", "ethylidene"),
            ("C[C]C", "propan-2-ylidene"),
            ("[C]1CCCCC1", "cyclohexylidene"),
            ("C[C]=O", "acetyl"),
        ] {
            roundtrip(smiles, name)?;
        }
        Ok(())
    }

    #[test]
    fn separate_radical_centers_roundtrip_with_lowest_locants() -> Result<(), String> {
        for (smiles, name) in [
            ("[CH2][CH2]", "ethane-1,2-diyl"),
            ("[CH2]CC[CH2]", "butane-1,4-diyl"),
            ("C[CH]C[CH2]", "butane-1,3-diyl"),
            ("[CH]1C[CH]CCC1", "cyclohexane-1,3-diyl"),
            ("[c]1cc[c]cc1", "benzene-1,4-diyl"),
        ] {
            roundtrip(smiles, name)?;
        }
        Ok(())
    }

    #[test]
    fn heteroatom_radicals_use_p71_forms_and_roundtrip() -> Result<(), String> {
        for (smiles, name) in [
            ("[NH2]", "azanyl"),
            ("[NH]", "azanylidene"),
            ("C[NH]", "methanaminyl"),
            ("C[N]C", "N-methylmethanaminyl"),
            ("[OH]", "hydroxyl"),
            ("O[O]", "hydroperoxyl"),
            ("C[O]", "methoxyl"),
            ("N[O]", "aminoxyl"),
            ("CN(C)[O]", "dimethylaminoxyl"),
            ("CC[O]", "ethoxyl"),
            ("CCC[O]", "propoxyl"),
            ("CCCC[O]", "butoxyl"),
            ("c1ccccc1[O]", "phenoxyl"),
            ("[SH]", "sulfanyl"),
            ("C[S]", "methylsulfanyl"),
            ("[SeH]", "selanyl"),
            ("C[Se]", "methylselanyl"),
        ] {
            roundtrip(smiles, name)?;
        }
        Ok(())
    }

    #[test]
    fn unsupported_radicals_never_reach_closed_shell_naming() -> Result<(), String> {
        for smiles in [
            "[CH2+]",
            "[O]",
            "C[N]",
            "CC(=O)[NH]",
            "ON[O]",
            "[CH2]O",
            "C[CH]C[CH]C(C)C",
            "[CH2]C[CH][CH2]",
            "[PH2]",
            "[CH2]CC(C)C",
            "C[CH]CC(C)C",
            "[CH]1C2CCCCC2CC1",
        ] {
            assert!(
                super::super::name_smiles(smiles, &Default::default()).is_err(),
                "{smiles}"
            );
        }
        let closed = super::super::graph_io::read_smiles("CCC", &Default::default())?;
        assert!(name_radical(&closed)?.is_none());
        Ok(())
    }
}
