//! #1044/#1048: without coordinates the @/@@ tags state the configuration and
//! molecule/reaction SMILES import draws wedges from them, while the original
//! worker reads CX wedge flags on its own depiction. Live comparisons check
//! acceptance and unchanged responses; native_import tests check the tag result.
//! Wavy bonds inhibit paint clearing (#1049), but not remote axial perception.
//! Undrawable wedge/hash paint on other orders is cleared for every conformer.
use reshiki::chemistry::{kekulize::Direction, reaction, smiles};

/// Retained axes can leave symmetric drawings unchanged; compare first (#1048).
pub fn changed(format: &str, text: &str, comparison: &anyhow::Result<()>) -> bool {
    comparison.is_err() && flagged(format, text)
}

/// The frozen worker may return invalid paint for these otherwise valid
/// reactions. Native drawing import clears it as molecule import already does.
pub fn cleared_undrawable(format: &str, text: &str) -> bool {
    format == "rsmi"
        && reaction::read_smiles(text).is_ok_and(|imported| {
            imported
                .reactants
                .iter()
                .chain(&imported.agents)
                .chain(&imported.products)
                .any(undrawable)
        })
}

fn undrawable(participant: &smiles::Imported) -> bool {
    let state = &participant.prepared.state;
    state
        .graph
        .bonds
        .iter()
        .zip(&state.directions)
        .any(|(bond, direction)| {
            !matches!(bond.order, 1 | 4) && matches!(direction, Direction::Wedge | Direction::Hash)
        })
}

/// Eligible paint or retained axes, not necessarily an observable change.
fn flagged(format: &str, text: &str) -> bool {
    match format {
        "smiles" => smiles::read(text).is_ok_and(|imported| {
            let directions = &imported.prepared.state.directions;
            imported.conformers.is_empty()
                && !directions.contains(&Direction::Unknown)
                && directions
                    .iter()
                    .any(|d| matches!(d, Direction::Wedge | Direction::Hash))
        }),
        "rsmi" => reaction::read_smiles(text).is_ok_and(|imported| {
            imported
                .reactants
                .iter()
                .chain(&imported.agents)
                .chain(&imported.products)
                .any(|participant| {
                    let state = &participant.prepared.state;
                    undrawable(participant)
                        || participant.conformers.is_empty()
                            && ((!state.directions.contains(&Direction::Unknown)
                                && state.graph.bonds.iter().zip(&state.directions).any(
                                    |(bond, direction)| {
                                        matches!(bond.order, 1 | 4)
                                            && matches!(
                                                direction,
                                                Direction::Wedge | Direction::Hash
                                            )
                                    },
                                ))
                                || state
                                    .metadata
                                    .bonds
                                    .iter()
                                    .any(|b| matches!(b.stereo, 6 | 7)))
                })
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_flags_cover_each_role_and_untagged_participants() {
        for text in [
            "C[C@H](N)C(=O)O>>O |wU:1.0|",
            "O>C[C@H](N)C(=O)O>O |wD:2.0|",
            "O>>C[C@H](N)C(=O)O |wU:2.0|",
            "CCC>>O |wD:1.0|",
            "c1ccccc1>>O |wU:0.0|",
            "CO>N>C-2C=1CCC2C1 |wU:7.5|",
            "CO>C-2C=1CCC2C1>CC |wD:6.6|",
            "C>C-2C=1CCC2C1.C1CC1>O |wU:5.5|",
        ] {
            assert!(flagged("rsmi", text), "{text}");
        }
    }

    #[test]
    fn reaction_wavy_exclusion_is_per_participant() {
        assert!(!flagged("rsmi", "C[C@H](N)C(=O)O>>O |w:0.0,wD:1.1|"));
        // A different participant's wavy bond does not stop the tagged redraw.
        let text = "C[C@H](N)C(=O)O>CC>O |wU:1.0,w:6.5|";
        let imported = reaction::read_smiles(text).unwrap();
        assert!(
            imported.agents[0]
                .prepared
                .state
                .directions
                .contains(&Direction::Unknown)
        );
        assert!(flagged("rsmi", text));
    }

    #[test]
    fn reaction_flags_survive_kekulization_for_classification() {
        let text = "c1(ncc[nH]1)-c1c(F)cccc1>>O |wU:0.0|";
        let imported = reaction::read_smiles(text).unwrap();
        assert!(
            imported.reactants[0]
                .prepared
                .state
                .directions
                .iter()
                .all(|direction| !matches!(direction, Direction::Wedge | Direction::Hash))
        );
        assert!(flagged("rsmi", text));
    }

    #[test]
    fn reaction_axes_with_remote_wavy_bonds_are_classified() {
        let text = "(c1(ncc[nH]1)-c1c(F)cccc1.CC)>>O |wU:0.0,w:12.13|";
        let imported = reaction::read_smiles(text).unwrap();
        let state = &imported.reactants[0].prepared.state;
        assert!(state.directions.contains(&Direction::Unknown));
        assert!(
            state
                .metadata
                .bonds
                .iter()
                .any(|b| matches!(b.stereo, 6 | 7))
        );
        assert!(
            state
                .directions
                .iter()
                .all(|direction| !matches!(direction, Direction::Wedge | Direction::Hash))
        );
        assert!(flagged("rsmi", text));
    }

    #[test]
    fn reaction_consumed_wedges_without_axes_keep_response_comparison() {
        let text = "c1ncc[nH]1>>O |wU:0.0|";
        let imported = reaction::read_smiles(text).unwrap();
        let state = &imported.reactants[0].prepared.state;
        assert!(
            state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
        assert!(
            state
                .directions
                .iter()
                .all(|direction| !matches!(direction, Direction::Wedge | Direction::Hash))
        );
        assert!(!flagged("rsmi", text));
    }

    #[test]
    fn reaction_undrawable_wedges_are_classified_with_or_without_coordinates() {
        for text in [
            "CO>N->[Cu]<-N>CC |wD:3.1|",
            "C/C=C/C>N>CC |wD:2.1|",
            "N->[Cu]<-N>N>CC |wD:1.0,SgD:0:name:data::::,SgH:0:0|",
            "CC>N>C#N |wU:4.1|",
            "C/C=C/C>N>CC |(0,0,;1,1,;2,0,;3,1,;4,0,;5,1,;6,0,),wD:2.1|",
            "CO>N->[Cu]<-N>CC |(0,0,1;1,1,1;2,0,1;3,1,1;4,0,1;5,1,1;6,0,1),wU:3.1|",
            "C/C=C/C>N>CC |wD:2.1,w:0.0|",
        ] {
            assert!(cleared_undrawable("rsmi", text), "{text}");
            assert!(flagged("rsmi", text), "{text}");
        }
    }

    #[test]
    fn unflagged_coordinated_or_invalid_inputs_keep_response_comparison() {
        for (format, text) in [
            ("rsmi", "C[C@H](N)C(=O)O>>O"),
            ("rsmi", "C[C@H](N)C(=O)O>>O |w:1.0|"),
            ("rsmi", "C[C@H](N)C(=O)O>>O |(1,2,),wU:1.0|"),
            ("rsmi", "C[C@H](N)C(=O)O>>O |(1,2,3),wU:1.0|"),
            ("rsmi", "invalid |wU:1.0|"),
            ("rsmi", "CO>N->[Cu]<-N>CC |w:3.1|"),
            ("rsmi", "C/C=C/C>N>CC |w:2.1|"),
            ("mol", "C[C@H](N)C(=O)O |wU:1.0|"),
            ("smiles", "C[C@H](N)C(=O)O |w:0.0,wD:1.1|"),
        ] {
            assert!(!flagged(format, text), "{format}: {text}");
        }
        assert!(flagged("smiles", "C[C@H](N)C(=O)O |wU:1.0|"));
    }
}
