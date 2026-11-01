//! Graph-derived lambda descriptors for neutral systematic fusion parents.
use super::super::{
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct FusionLambdaDescriptor {
    pub atom_id: usize,
    pub locant: String,
    pub bonding_number: usize,
    pub standard_valence: usize,
}
impl FusionLambdaDescriptor {
    pub fn text(&self) -> String {
        format!("{}lambda^{}", self.locant, self.bonding_number)
    }
}
pub fn fusion_lambda_descriptors(
    mol: &Molecule,
    atoms: &AtomSet,
    locants: &BTreeMap<usize, String>,
) -> Result<Vec<FusionLambdaDescriptor>, String> {
    let mut descriptors = Vec::new();
    for &id in atoms {
        let atom = &mol.atoms[&id];
        if atom.charge != 0 || atom.is_carbon() {
            continue;
        }
        let locant = locants
            .get(&id)
            .ok_or_else(|| format!("missing completed-system locant for parent atom {id}"))?;
        let hydrogen_count = if atom.total_h_count != 0 {
            atom.total_h_count
        } else {
            atom.explicit_h_count
        };
        let bonding_number = hydrogen_count
            + mol
                .get_neighbors(id)
                .iter()
                .filter_map(|&n| mol.get_bond(id, n))
                .map(|b| b.order)
                .sum::<usize>();
        let standard_valence = atom.element().map_or(0, |e| e.standard_valence);
        if bonding_number > standard_valence {
            // Upstream divergence: an uncited nonstandard-valence heteroatom H
            // stops the joint carbon-H proof while leaving several parent pi
            // assignments available. A lambda/H citation alone can then move
            // the implicit carbon hydrogen when the name is reconstructed.
            if hydrogen_count != 0 {
                return Err("openclatura: fused lambda-hydrogen parent state not ported".into());
            }
            descriptors.push(FusionLambdaDescriptor {
                atom_id: id,
                locant: locant.clone(),
                bonding_number,
                standard_valence,
            });
        }
    }
    descriptors.sort_by_key(|d| (locant_key(&d.locant), d.bonding_number));
    Ok(descriptors)
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::*;
    #[test]
    fn complete_graph_oxo_bonds_determine_lambda_bonding_number() {
        let mol = read_smiles("S1(=O)(=O)CCC1").unwrap();
        let sulfur = *mol.atoms.iter().find(|(_, a)| a.symbol == "S").unwrap().0;
        let descriptors = fusion_lambda_descriptors(
            &mol,
            &AtomSet::from([sulfur]),
            &BTreeMap::from([(sulfur, "2a".into())]),
        )
        .unwrap();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].text(), "2alambda^6");
    }

    #[test]
    fn lambda_hydrogen_does_not_waive_an_unproved_carbon_hydrogen_state() {
        let mol = read_smiles("CC1=C(C(=O)O)CC2=CN=C[SH]21").unwrap();
        let sulfur = *mol.atoms.iter().find(|(_, a)| a.symbol == "S").unwrap().0;
        assert_eq!(mol.atoms[&sulfur].total_h_count, 1);
        assert_eq!(mol.degree(sulfur), 3);
        let result = fusion_lambda_descriptors(
            &mol,
            &AtomSet::from([sulfur]),
            &BTreeMap::from([(sulfur, "4".into())]),
        );
        assert_eq!(
            result.unwrap_err(),
            "openclatura: fused lambda-hydrogen parent state not ported"
        );
    }
}
