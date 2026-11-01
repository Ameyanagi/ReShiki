//! Input states that the plain-name renderers cannot preserve.
use super::molecule::Molecule;
use super::perception::PerceivedGroup;
use std::collections::BTreeSet;

pub fn validate_principal_charge_roles(
    groups: &[PerceivedGroup],
    key: Option<&str>,
) -> Result<(), String> {
    let selected = || groups.iter().filter(|g| Some(g.key.as_str()) == key);
    let has_derivative = |derivative: &str| {
        selected().any(|g| {
            g.descriptor
                .as_ref()
                .is_some_and(|d| d.derivative == derivative)
        })
    };
    // Upstream divergence: a shared ester/anion suffix cannot assign separate charge states to mixed principal groups.
    if has_derivative("anion") && has_derivative("ester") {
        return Err(
            "openclatura: mixed ester and anion principal citations are not supported".into(),
        );
    }
    Ok(())
}

pub fn validate_input(mol: &Molecule) -> Result<(), String> {
    for (&idx, atom) in &mol.atoms {
        // Upstream divergence: elemental ion shortcuts require their exact charge and hydrogen state.
        if mol.atoms.len() == 1
            && atom.charge != 0
            && !matches!(
                (atom.symbol.as_str(), atom.charge, atom.total_h_count),
                ("Li" | "Na" | "K", 1, 0)
                    | ("Mg" | "Ca", 2, 0)
                    | ("F" | "Cl" | "Br" | "I", -1, 0)
                    | ("N", 1, 4)
                    | ("N", -1, 2)
                    | ("O" | "S", -1, 1)
                    | ("O" | "S" | "Se", -2, 0)
                    | ("O", 1, 3)
                    | ("P", 1, 4)
                    | ("H", -1 | 1, 0)
            )
        {
            return Err(format!(
                "openclatura: monatomic {} charge/hydrogen state is not supported",
                atom.symbol
            ));
        }
        // Upstream divergence: unit-charge names cannot represent unsupported charge magnitudes.
        if atom.charge.unsigned_abs() > 1
            && !(mol.atoms.len() == 1
                && atom.total_h_count == 0
                && matches!(
                    (atom.symbol.as_str(), atom.charge),
                    ("Mg" | "Ca", 2) | ("O" | "S" | "Se", -2)
                ))
        {
            return Err(format!(
                "openclatura: charge {} on {} atom {idx} is not supported",
                atom.charge, atom.symbol
            ));
        }
        if atom.charge != 0
            || atom.total_h_count == 0
            || !matches!(
                atom.symbol.as_str(),
                "P" | "As" | "Sb" | "Bi" | "S" | "Se" | "Te"
            )
            || mol.degree(idx) != 1
        {
            continue;
        }
        let neighbor = mol.get_neighbors(idx)[0];
        let bond = mol
            .get_bond(idx, neighbor)
            .ok_or("openclatura: missing terminal hydride bond")?;
        let expected = atom
            .element()
            .ok_or("openclatura: unsupported terminal hydride element")?
            .standard_valence;
        // Upstream divergence: ordinary terminal hydride/group names require their exact bonding number and H count.
        if (bond.order == 1 || mol.atoms[&neighbor].is_carbon())
            && bond.order + atom.total_h_count != expected
        {
            return Err(format!(
                "openclatura: terminal {} atom {idx} with {} hydrogens and bonding number {} is not supported",
                atom.symbol,
                atom.total_h_count,
                bond.order + atom.total_h_count
            ));
        }
    }
    Ok(())
}

/// Upstream divergence: CIP implementations disagree on the duplicate atom of
/// a mancude ring bond (RDKit's labels and the OPSIN reading of the same name
/// differ). Decline a stereocentre whose ligand ranking, compared sphere by
/// sphere, is decided or tied where that duplicate's element is ambiguous.
pub fn validate_mancude_duplicate_ranking(mol: &Molecule) -> Result<(), String> {
    const SPHERES: usize = 8;
    let z = |idx: usize| mol.atoms[&idx].element().map_or(0, |e| e.atomic_number);
    // Per sphere, descending atomic numbers with each ambiguous duplicate at
    // its lowest and at its highest element.
    let spheres = |center: usize, ligand: usize| -> Vec<[Vec<usize>; 2]> {
        let mut seen = BTreeSet::from([center, ligand]);
        let mut frontier = vec![(ligand, center)];
        let mut result = Vec::new();
        for _ in 0..SPHERES {
            let mut sphere: [Vec<usize>; 2] = Default::default();
            let mut next = Vec::new();
            for &(atom_idx, parent) in &frontier {
                let atom = &mol.atoms[&atom_idx];
                let mut entries = vec![1; atom.total_h_count];
                let mut aromatic_double = false;
                for &n in mol.get_neighbors(atom_idx) {
                    let order = mol.get_bond(atom_idx, n).map_or(1, |b| b.order);
                    let aromatic = atom.is_aromatic && mol.atoms[&n].is_aromatic;
                    aromatic_double |= aromatic && order == 2;
                    let duplicates = if aromatic { 0 } else { order - 1 };
                    entries.extend(std::iter::repeat_n(z(n), duplicates));
                    if n == parent {
                        continue;
                    }
                    entries.push(z(n));
                    if seen.insert(n) {
                        next.push((n, atom_idx));
                    }
                }
                let variants: Vec<_> = mol
                    .get_neighbors(atom_idx)
                    .iter()
                    .filter(|&&n| aromatic_double && mol.atoms[&n].is_aromatic)
                    .map(|&n| z(n))
                    .collect();
                for (side, duplicate) in sphere
                    .iter_mut()
                    .zip([variants.iter().min(), variants.iter().max()])
                {
                    side.extend(&entries);
                    side.extend(duplicate);
                }
            }
            for side in &mut sphere {
                side.sort_unstable_by(|a, b| b.cmp(a));
            }
            result.push(sphere);
            frontier = next;
        }
        result
    };
    for (&center, atom) in &mol.atoms {
        if atom.stereo.is_none() {
            continue;
        }
        let ligands: Vec<_> = mol
            .get_neighbors(center)
            .iter()
            .map(|&l| (z(l), spheres(center, l)))
            .collect();
        for (i, (zi, left)) in ligands.iter().enumerate() {
            for (zj, right) in &ligands[i + 1..] {
                if zi != zj {
                    continue;
                }
                for (l, r) in left.iter().zip(right) {
                    let orders: BTreeSet<_> = l
                        .iter()
                        .flat_map(|l| r.iter().map(move |r| l.cmp(r)))
                        .collect();
                    if orders.len() > 1 {
                        return Err(format!(
                            "openclatura: CIP rank at atom {center} depends on a mancude duplicate atom"
                        ));
                    }
                    if !orders.contains(&std::cmp::Ordering::Equal) {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::naming::openclatura::name_smiles;

    #[test]
    fn public_entry_declines_kekule_dependent_cip_ranks() {
        for smiles in [
            "NC(N)[C@@H](C)c1ncncc1",
            "O=[N+]([O-])[C@@H]1NCC[C@@H]1c1ncccc1",
            "O=[N+]([O-])[C@@H]1NCC[C@@H]1c1ncncc1Br",
            "C[C@H](Cc1ncccc1)CC=NC1CCCCC1",
            "C[C@H](Cc1ncccc1)CC=NC1CCC2(CC1)CCCC2",
        ] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
        }
        for (smiles, expected) in [
            (
                "O=[N+]([O-])[C@@H]1NCC[C@@H]1c1ccncc1",
                "4-[(2S,3R)-2-nitropyrrolidin-3-yl]pyridine",
            ),
            (
                "O=[N+]([O-])C[C@@H](C)c1ncccc1",
                "2-[(2R)-1-nitropropan-2-yl]pyridine",
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
    fn public_entry_declines_unrepresented_charge_states() {
        for smiles in [
            "[CH3+]",
            "C[CH2-]",
            "C1C[CH+]1",
            "C[NH+2]C",
            "C[Si-](C)(C)C",
            "C[B-](C)C",
            "[Mg+]",
            "[NaH+]",
            "[ClH-]",
            "COC(=O)C1CC(C(=O)[O-])NC(C(N)=S)C1",
            "CCC[PH-]",
            "CCC[As-](C)",
            "CCC[Sb-](C)(C)C",
            "CCC[SH2-]",
            "CCC[SeH2+]",
            "CCC[TeH2-]",
            "CCC[ClH+]",
            "CCC[IH-]",
        ] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn public_entry_declines_unrepresented_parent_elements_and_hydrogens() {
        for smiles in [
            "[Tl]1CC1",
            "[InH]1CCCC1",
            "C[SH3]",
            "C[SH5]",
            "C[SeH3]",
            "C[PH4]",
            "CO[SH3]",
            "CC(=O)[SeH3]",
            "[SH2]1CCCC1",
        ] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn public_entry_preserves_supported_charge_and_hydride_names() {
        for (smiles, expected) in [
            ("C", "methane"),
            ("CC", "ethane"),
            ("C[NH2+]C", "N-methylmethanaminium"),
            ("C[N+](C)(C)C", "N,N,N-trimethylmethanaminium"),
            ("CC(=O)[O-]", "acetate"),
            ("C[S-]", "methanethiolate"),
            ("C[SeH]", "methaneselenol"),
            ("CS", "methanethiol"),
            ("C[PH2]", "phosphanylmethane"),
            ("C[SH2]C", "dimethyl-lambda4-sulfane"),
            ("[CH2-][S+](C)C", "(dimethylsulfaniumyl)methanide"),
            ("C[Si](C)(C)C", "tetramethylsilane"),
            ("[NH4+]", "ammonium"),
            ("[O-2]", "oxide"),
            ("[Ca+2]", "calcium"),
            ("[N-]1[NH+]=CC=C1", "1H-pyrazol-1-ide-2-ium"),
        ] {
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }
}
