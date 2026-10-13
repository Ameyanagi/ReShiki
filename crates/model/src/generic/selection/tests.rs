use super::*;
use crate::{
    arrows::Preset,
    document::{Arrow, Point},
    generic::{
        RepeatCount, RepeatPattern, RepeatUnit, prepare_component_expansion_input,
        prepare_expansion_input,
    },
    grouping::Group,
    reactions::{Participant, Reaction},
};

fn single_atom() -> Document {
    let mut source = Document::default();
    source.add_atom("C", Point::default());
    source
}

fn empty_repeat() -> RepeatUnit {
    RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members: vec![],
        crossings: vec![],
        crossing_brackets: vec![],
        head_count: None,
        usage: "repeat".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 1 },
        label: "1".into(),
        flip: String::new(),
    }
}

// These records deliberately need not validate: a work refusal must happen
// before even the accounting pass over an untrusted live document.
fn metadata_cases() -> Vec<(&'static str, Document)> {
    let mut cases = Vec::new();
    let mut groups = single_atom();
    groups.groups = (0..64)
        .map(|id| Group {
            id,
            members: vec![],
            integral: false,
        })
        .collect();
    cases.push(("groups", groups));
    let mut abbreviated = single_atom();
    abbreviated.abbreviations = vec![
        crate::abbreviations::Abbreviation {
            label_style: None,
            label_color_override: false,
            highlight: None,
            label: "C".into(),
            reverse_label: String::new(),
            anchor: 1,
            members: vec![],
            alignment: Default::default(),
        };
        64
    ];
    cases.push(("abbreviations", abbreviated));
    let mut named = single_atom();
    named.molecule_names = vec![
        crate::molecule_names::MoleculeName {
            annotation: 2,
            atoms: vec![],
            smiles: String::new(),
            text: String::new(),
        };
        64
    ];
    cases.push(("names", named));
    let mut repeated = single_atom();
    repeated.generic.repeats = vec![empty_repeat(); 64];
    cases.push(("repeats", repeated));
    let mut arrowed = single_atom();
    arrowed.arrows = (2..66)
        .map(|id| {
            Arrow::new(
                id,
                Point::default(),
                Point::new(42., 0.),
                Preset::Forward,
                Default::default(),
            )
        })
        .collect();
    cases.push(("arrows", arrowed));
    let mut reacted = single_atom();
    reacted.reactions = vec![Reaction::new(2); 64];
    cases.push(("reactions", reacted));
    cases
}

#[test]
fn scope_counting_refuses_metadata_records_before_inspection() {
    for (kind, source) in metadata_cases() {
        for connected in [false, true] {
            probe::reset();
            let mut work = Work::new(3);
            let refusal = if connected {
                component(&source, &[1], &mut work).map(|_| ())
            } else {
                check_molecules(&source, &[1], &mut work)
            };
            assert!(refusal.unwrap_err().contains("work limit"), "{kind}");
            assert_eq!(probe::count(Stage::ScopeCount), 0, "{kind}");
            assert_eq!(probe::count(Stage::ScopeParticipants), 0, "{kind}");
            assert_eq!(probe::count(Stage::ScopeTraverse), 0, "{kind}");

            probe::reset();
            let limits = Limits {
                max_work: 1,
                ..Limits::default()
            };
            let refusal = if connected {
                prepare_component_expansion_input(&source, &[1], limits)
            } else {
                prepare_expansion_input(&source, &[1], limits)
            };
            assert!(refusal.err().unwrap().contains("work limit"), "{kind}");
            assert!(
                probe::counts().iter().all(|count| *count == 0),
                "{kind}: refuse before scope counting, participant/member passes, repeat counting, validation and snapshot"
            );
        }
    }
}

#[test]
fn scope_counting_charges_reaction_participants_before_entering_their_pass() {
    let mut source = single_atom();
    let mut reaction = Reaction::new(2);
    reaction.reactants = vec![
        Participant {
            atoms: vec![1; 1024],
            coefficient: 1,
        };
        64
    ];
    source.reactions.push(reaction);
    probe::reset();
    let mut work = Work::new(record_count(&source) * 2 + 1);
    assert!(
        component(&source, &[1], &mut work)
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(probe::count(Stage::ScopeCount), 1);
    assert_eq!(probe::count(Stage::ScopeParticipants), 0);
    assert_eq!(probe::count(Stage::ScopeTraverse), 0);
}

#[test]
fn scope_members_are_reserved_before_native_member_traversal() {
    let mut source = single_atom();
    source.atoms[0].centroid = vec![1; 1024];
    source.groups.push(Group {
        id: 2,
        members: vec![1; 1024],
        integral: false,
    });
    for connected in [false, true] {
        probe::reset();
        let mut work = Work::new(record_count(&source) * 2 + 1);
        let refusal = if connected {
            component(&source, &[1], &mut work).map(|_| ())
        } else {
            check_molecules(&source, &[1], &mut work)
        };
        assert!(refusal.unwrap_err().contains("work limit"));
        assert_eq!(probe::count(Stage::ScopeCount), 1);
        assert_eq!(probe::count(Stage::ScopeTraverse), 0);
    }
}

#[test]
fn input_repeat_and_drawing_size_counting_refuse_before_their_pass() {
    for (kind, source) in metadata_cases()
        .into_iter()
        .filter(|(kind, _)| matches!(*kind, "groups" | "abbreviations" | "repeats"))
    {
        let mut counted = Work::new(usize::MAX);
        check_molecules(&source, &[1], &mut counted).unwrap();
        let prefix = usize::MAX - counted.remaining;
        let counting =
            source.generic.repeats.len() + source.groups.len() + source.abbreviations.len();
        probe::reset();
        assert!(
            prepare_expansion_input(
                &source,
                &[1],
                Limits {
                    max_work: prefix + counting - 1,
                    ..Limits::default()
                }
            )
            .err()
            .unwrap()
            .contains("work limit"),
            "{kind}"
        );
        assert_eq!(probe::count(Stage::ScopeCount), 1, "{kind}");
        assert_eq!(probe::count(Stage::ScopeTraverse), 1, "{kind}");
        assert_eq!(probe::count(Stage::RepeatCount), 0, "{kind}");
        assert_eq!(probe::count(Stage::Validate), 0, "{kind}");
        assert_eq!(probe::count(Stage::Snapshot), 0, "{kind}");
    }
}

#[test]
fn component_scope_applies_default_source_and_work_limits() {
    let mut source = single_atom();
    let second = source.add_atom("O", Point::new(42., 0.));
    source.add_bond(1, second, 1, "plain");
    assert_eq!(component_scope(&source, &[1]).unwrap(), vec![1, second]);
    let atom = source.atoms[0].clone();
    source.atoms = vec![atom; super::super::limits::MAX_SOURCE_ATOMS + 1];
    probe::reset();
    assert!(
        component_scope(&source, &[1])
            .unwrap_err()
            .contains("source")
    );
    assert_eq!(probe::count(Stage::ScopeCount), 0);
    let mut source = single_atom();
    source.atoms[0].centroid = vec![1; Limits::default().max_work / 64 + 1];
    probe::reset();
    assert!(
        component_scope(&source, &[1])
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(probe::count(Stage::ScopeTraverse), 0);
}
