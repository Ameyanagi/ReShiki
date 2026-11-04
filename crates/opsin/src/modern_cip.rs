//! Full CIP ordering for descriptors that depend on other stereogenic units.
//! The OPSIN constitutional rules remain the fast path for ordinary R/S.
use crate::graph::{AtomId, BondStereoValue, Graph, StereoReference};
use crate::stereochemistry_handler::StereochemistryError;
use reshiki_chemistry::{
    electronic::Hybridization,
    graph::{Atom, Bond, Graph as CipGraph},
    kekulize::Direction,
    ranking::Metadata,
    stereo::{
        cip::{configuration::parity4, label},
        perception::{Properties, RingCache, State},
    },
};
use std::collections::HashMap;

fn error(message: impl ToString) -> StereochemistryError {
    StereochemistryError::Uninterpretable(message.to_string())
}

/// Keep the OPSIN graph's physical hydrogens and bond insertion order. A
/// parity's centre reference denotes a lone pair, which the CIP adapter also
/// represents with the central atom after its physical neighbours.
pub(crate) fn state(
    graph: &Graph,
) -> Result<(State, HashMap<AtomId, usize>), StereochemistryError> {
    let atoms: Vec<_> = graph.atoms.iter().filter(|a| a.active).collect();
    let indices: HashMap<_, _> = atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
    let source_bonds: Vec<_> = graph.bonds.iter().filter(|b| b.active).collect();
    let cip_graph = CipGraph {
        atoms: atoms
            .iter()
            .map(|a| {
                Ok(Atom {
                    atomic_number: a.element.atomic_number(),
                    isotope: u16::try_from(a.isotope.unwrap_or_default()).map_err(error)?,
                    charge: i8::try_from(a.charge).map_err(error)?,
                    no_implicit: true,
                    ..Atom::default()
                })
            })
            .collect::<Result<_, StereochemistryError>>()?,
        bonds: source_bonds
            .iter()
            .map(|b| Bond {
                a: indices[&b.from],
                b: indices[&b.to],
                order: b.order,
                aromatic: false,
            })
            .collect(),
    };
    let mut metadata = Metadata::unspecified(&cip_graph);
    for (index, atom) in atoms.iter().enumerate() {
        let Some(parity) = &atom.parity else { continue };
        let mut carriers: Vec<_> = graph.neighbours(atom.id).into_iter().map(Some).collect();
        if carriers.len() == 3 {
            carriers.push(Some(atom.id));
        }
        let references = parity.atom_refs.map(|reference| match reference {
            Some(StereoReference::Atom(atom)) => Some(atom),
            _ => None,
        });
        let permutation = parity4(&references, &carriers).map_err(error)?;
        if permutation == 0 || !matches!(parity.parity, -1 | 1) {
            return Err(error(
                "Full CIP tetrahedral references do not match atom neighbours",
            ));
        }
        metadata.atoms[index].chiral_tag = if (parity.parity == 1) ^ (permutation == 1) {
            1
        } else {
            2
        };
    }
    for (index, bond) in source_bonds.iter().enumerate() {
        if let Some(stereo) = &bond.stereo {
            metadata.bonds[index].stereo = match stereo.value {
                BondStereoValue::Cis => 4,
                BondStereoValue::Trans => 5,
            };
            metadata.bonds[index].stereo_atoms =
                vec![indices[&stereo.atom_refs[0]], indices[&stereo.atom_refs[3]]];
        }
    }
    let state = State {
        valences: cip_graph.valences().map_err(error)?,
        directions: vec![Direction::None; cip_graph.bonds.len()],
        conjugated: vec![false; cip_graph.bonds.len()],
        hybridizations: vec![Hybridization::Unspecified; cip_graph.atoms.len()],
        properties: Properties::unspecified(&cip_graph),
        rings: RingCache::default(),
        graph: cip_graph,
        metadata,
    };
    Ok((state, indices))
}

pub(crate) fn atom_labels(graph: &Graph) -> Result<HashMap<AtomId, String>, StereochemistryError> {
    let (state, indices) = state(graph)?;
    let labelled = label::assign(&state, &label::Options::default()).map_err(error)?;
    Ok(indices
        .into_iter()
        .filter_map(|(atom, index)| {
            labelled.state.properties.atoms[index]
                .cip_code
                .clone()
                .map(|label| (atom, label))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParseOptions, Parser, Status};

    #[test]
    fn mancude_names_preserve_the_named_configuration() {
        use reshiki_chemistry::smiles;

        let canonical = |text: &str| {
            let imported = smiles::read(text).unwrap();
            smiles::write::write(&imported.prepared.state, Default::default())
                .unwrap()
                .text
        };
        let parser = Parser::new().unwrap();
        for (name, expected, expected_atom_labels) in [
            (
                "3-[(S)-(cyclopenta-1,3-dien-1-yl)(hydroxy)methyl]cyclopenta-2,4-dien-1-ide",
                "O[C@H](C1=CC=CC1)C1=C[CH-]C=C1",
                Some(&["S"][..]),
            ),
            (
                "3-[(R)-(cyclopenta-1,3-dien-1-yl)(hydroxy)methyl]cyclopenta-2,4-dien-1-ide",
                "O[C@@H](C1=CC=CC1)C1=C[CH-]C=C1",
                Some(&["R"][..]),
            ),
            (
                "3-[(S)-(cyclopenta-1,4-dien-1-yl)(hydroxy)methyl]cyclopenta-2,4-dien-1-ide",
                "O[C@H](C1=CCC=C1)C1=C[CH-]C=C1",
                Some(&["S"][..]),
            ),
            (
                "2-[(S)-(4-fluoro-1H-pyrrol-1-id-2-yl)(hydroxy)methyl]-2H-pyrrol-2-ide",
                "O[C@H](C1=CC(F)=C[N-]1)[C-]1C=CC=N1",
                Some(&["S"][..]),
            ),
            (
                "2-[(R)-(4-fluoro-1H-pyrrol-1-id-2-yl)(hydroxy)methyl]-2H-pyrrol-2-ide",
                "O[C@@H](C1=CC(F)=C[N-]1)[C-]1C=CC=N1",
                Some(&["R"][..]),
            ),
            (
                "2-[(R)-(cyclopenta-2,4-dien-1-id-1-yl)(hydroxy)methyl]bicyclo[3.3.0]octa-1(5),3-dien-2-ide",
                "O[C@H]([C-]1C=CC=C1)[C-]1C=CC2=C1CCC2",
                Some(&["R"][..]),
            ),
            (
                "2-[(S)-(cyclopenta-2,4-dien-1-id-1-yl)(hydroxy)methyl]bicyclo[3.3.0]octa-1(5),3-dien-2-ide",
                "O[C@@H]([C-]1C=CC=C1)[C-]1C=CC2=C1CCC2",
                Some(&["S"][..]),
            ),
            // V01/V02: the independent app labeller returns S/R for these R/S
            // names, so enforce P-92.1.4.4 through structure identity alone.
            (
                "1-[(R)-(cyclopenta-2,4-dien-1-id-1-yl)(hydroxy)methyl]cyclohepta-2,4,6-trien-1-ylium",
                "O[C@H]([C-]1C=CC=C1)[C+]1C=CC=CC=C1",
                None,
            ),
            (
                "1-[(S)-(cyclopenta-2,4-dien-1-id-1-yl)(hydroxy)methyl]cyclohepta-2,4,6-trien-1-ylium",
                "O[C@@H]([C-]1C=CC=C1)[C+]1C=CC=CC=C1",
                None,
            ),
            // The E/Z descriptor ranks the same two rings at the double bond.
            (
                "3-[(1Z)-1-(cyclopenta-1,3-dien-1-yl)-2-fluoroethenyl]cyclopenta-2,4-dien-1-ide",
                "F/C=C(\\C1=CC=CC1)C1=C[CH-]C=C1",
                Some(&[][..]),
            ),
            (
                "3-[(1E)-1-(cyclopenta-1,3-dien-1-yl)-2-fluoroethenyl]cyclopenta-2,4-dien-1-ide",
                "F/C=C(/C1=CC=CC1)C1=C[CH-]C=C1",
                Some(&[][..]),
            ),
            (
                "2-[(1Z)-1-(cyclopenta-2,4-dien-1-id-1-yl)-2-fluoroethenyl]bicyclo[3.3.0]octa-1(5),3-dien-2-ide",
                "F/C=C(/[C-]1C=CC=C1)[C-]1C=CC2=C1CCC2",
                Some(&[][..]),
            ),
            (
                "2-[(1E)-1-(cyclopenta-2,4-dien-1-id-1-yl)-2-fluoroethenyl]bicyclo[3.3.0]octa-1(5),3-dien-2-ide",
                "F/C=C(\\[C-]1C=CC=C1)[C-]1C=CC2=C1CCC2",
                Some(&[][..]),
            ),
            (
                "1-[(1Z)-1-(cyclopenta-2,4-dien-1-id-1-yl)-2-fluoroethenyl]cyclohepta-2,4,6-trien-1-ylium",
                "F/C=C(/[C-]1C=CC=C1)[C+]1C=CC=CC=C1",
                Some(&[][..]),
            ),
            (
                "1-[(1E)-1-(cyclopenta-2,4-dien-1-id-1-yl)-2-fluoroethenyl]cyclohepta-2,4,6-trien-1-ylium",
                "F/C=C(\\[C-]1C=CC=C1)[C+]1C=CC=CC=C1",
                Some(&[][..]),
            ),
        ] {
            let result = parser.parse(name, &ParseOptions::strict());
            assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
            let structure = result.structure.unwrap();
            assert_eq!(
                canonical(&structure.semantic_cxsmiles().unwrap()),
                canonical(expected),
                "{name}"
            );
            let labels = atom_labels(&structure.graph).unwrap();
            let labels: Vec<_> = labels.values().map(String::as_str).collect();
            if let Some(expected_atom_labels) = expected_atom_labels {
                assert_eq!(labels, expected_atom_labels, "{name}");
            }
        }
    }

    #[test]
    fn full_cip_adapter_preserves_ordinary_and_pseudo_atom_parity() {
        let parser = Parser::new().unwrap();
        for (name, expected) in [
            ("(2R)-butan-2-ol", vec![("2", "R")]),
            ("(2S)-butan-2-ol", vec![("2", "S")]),
            (
                "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
                vec![("1", "R"), ("3", "r"), ("5", "S")],
            ),
            (
                "(1R,3s,5S)-8-azabicyclo[3.2.1]octan-3-ol",
                vec![("1", "R"), ("3", "s"), ("5", "S")],
            ),
            (
                "(1R,3s,5S,8r)-bicyclo[3.2.1]octane-3,8-diol",
                vec![("1", "R"), ("3", "s"), ("5", "S"), ("8", "r")],
            ),
            (
                "(1R,3s,5S,8s)-bicyclo[3.2.1]octane-3,8-diol",
                vec![("1", "R"), ("3", "s"), ("5", "S"), ("8", "s")],
            ),
            (
                "(1s,3s)-1,3-dimethylcyclobutane",
                vec![("1", "s"), ("3", "s")],
            ),
            ("(1r,4r)-cyclohexane-1,4-diol", vec![("1", "r"), ("4", "r")]),
            ("N-[(1s,3s)-3-methylcyclobutyl]acetamide", vec![]),
        ] {
            let result = parser.parse(name, &ParseOptions::strict());
            assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
            let structure = result.structure.unwrap();
            let labels = atom_labels(&structure.graph).unwrap();
            for (locant, expected) in expected {
                let atom = structure
                    .graph
                    .atom_by_locant(structure.fragment, locant)
                    .unwrap();
                assert_eq!(
                    labels.get(&atom).map(String::as_str),
                    Some(expected),
                    "{name}: {locant}"
                );
            }
        }
    }

    #[test]
    fn lowercase_descriptors_are_not_uppercase_aliases() {
        let parser = Parser::new().unwrap();
        let lower = parser.parse(
            "(2S,3s,8R)-1-azapentacyclo[4.2.0.0^{3,8}.0^{4,7}.0^{2,5}]octa-4(7),5-diene",
            &ParseOptions::strict(),
        );
        let upper = parser.parse(
            "(2S,3S,8R)-1-azapentacyclo[4.2.0.0^{3,8}.0^{4,7}.0^{2,5}]octa-4(7),5-diene",
            &ParseOptions::strict(),
        );
        assert_eq!(lower.status, Status::Success, "{}", lower.message);
        assert_eq!(upper.status, Status::Success, "{}", upper.message);
        assert_ne!(
            lower.structure.unwrap().semantic_cxsmiles().unwrap(),
            upper.structure.unwrap().semantic_cxsmiles().unwrap()
        );
    }

    #[test]
    fn contradictory_lowercase_ring_descriptors_do_not_leave_provisional_stereo() {
        let parser = Parser::new().unwrap();
        let name = "(1r,3s)-1,3-dimethylcyclobutane";
        let strict = parser.parse(name, &ParseOptions::strict());
        assert_eq!(strict.status, Status::Failure);
        let mut options = ParseOptions::strict();
        options.warn_rather_than_fail_on_uninterpretable_stereochemistry = true;
        let warning = parser.parse(name, &options);
        assert_eq!(warning.status, Status::Warning, "{}", warning.message);
        assert!(
            warning
                .structure
                .unwrap()
                .graph
                .atoms
                .iter()
                .all(|atom| atom.parity.is_none())
        );
    }

    #[test]
    fn lowercase_descriptors_follow_absolute_centres_even_when_written_first() {
        let parser = Parser::new().unwrap();
        let options = ParseOptions::strict();
        let a = parser.parse(
            "(3r,1R,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
            &options,
        );
        let b = parser.parse(
            "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
            &options,
        );
        assert_eq!(a.status, Status::Success, "{}", a.message);
        assert_eq!(b.status, Status::Success, "{}", b.message);
        assert_eq!(
            a.structure.unwrap().semantic_cxsmiles().unwrap(),
            b.structure.unwrap().semantic_cxsmiles().unwrap()
        );
    }
}
