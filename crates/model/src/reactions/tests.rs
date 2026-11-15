use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, Point},
};

#[test]
fn only_reaction_presets_accept_new_roles() {
    for &preset in Preset::ALL {
        for role in Role::ALL {
            let mut doc = Document::default();
            let atom = doc.add_atom("C", Point::default());
            let arrow = doc.next_id();
            doc.arrows.push(Arrow::new(
                arrow,
                Point::new(80., 0.),
                Point::new(160., 0.),
                preset,
                ArrowStyle::preset(preset),
            ));
            let before = doc.clone();
            let result = assign(&mut doc, arrow, &[atom], role);
            if matches!(preset, Preset::Forward | Preset::Equilibrium | Preset::Bent) {
                result.unwrap();
                assert_eq!(doc.reactions[0].participants(role)[0].atoms, [atom]);
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    preset.reaction_role_refusal().unwrap(),
                    "{preset} must refuse {role:?} with its explanation"
                );
                assert_eq!(doc, before, "Refusal must preserve {preset} drawing");
            }
        }
    }
}

#[test]
fn legacy_resonance_roles_remain_valid_and_reconcile_without_changes() {
    let mut doc = Document::default();
    let reactant = doc.add_atom("C", Point::default());
    let product = doc.add_atom("C", Point::new(240., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(80., 0.),
        Point::new(160., 0.),
        Preset::Resonance,
        ArrowStyle::preset(Preset::Resonance),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: vec![reactant],
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    doc.reactions.push(reaction);
    let before = doc.clone();
    doc.validate().unwrap();
    reconcile(&mut doc).unwrap();
    assert_eq!(doc, before);
}

#[test]
fn whole_molecules_agrees_with_the_molecules_of_a_selection() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("N", Point::new(200., 0.));
    let d = doc.add_atom("C", Point::new(242., 0.));
    doc.add_bond(c, d, 0, "dotted");
    let point =
        crate::attachments::add(&mut doc, &[a, b], crate::attachments::Kind::Variable).unwrap();
    for selected in [
        vec![],
        vec![a],
        vec![a, b],
        vec![a, b, point],
        vec![c],
        vec![c, d],
        vec![a, b, point, c],
        vec![point],
        doc.all_ids(),
    ] {
        let set: HashSet<_> = selected.iter().copied().collect();
        assert_eq!(
            whole_molecules(&doc, &set),
            molecules(&doc, &selected)
                .iter()
                .flatten()
                .all(|id| set.contains(id)),
            "{selected:?}"
        );
    }
    // A zero-order bond joins nothing: each end is its own molecule.
    assert_eq!(molecules(&doc, &[c, d]), [vec![c], vec![d]]);
}
