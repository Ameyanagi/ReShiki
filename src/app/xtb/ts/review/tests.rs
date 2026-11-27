//! Scheme review through the same messages as the row, menu and palette.
use super::*;
use crate::app::xtb::ts::card::tests::claisen;
#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
use crate::app::xtb::ts::card::tests::{diels_alder, hydrogen_transfer};
use reshiki::document::{Arrow, Point};
use reshiki::reactions::{Participant, Reaction};

fn app(doc: Document, arrow: u64) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = vec![arrow];
    app.tab.busy = false;
    app
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn open(app: &mut App, mode: Mode) {
    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Review(mode, None))));
}

fn drawn_claisen() -> (Document, u64) {
    let (mut doc, arrow) = claisen();
    let reaction = doc.reactions.pop().unwrap();
    doc.arrows[0].start = Point::new(110., 0.);
    doc.arrows[0].end = Point::new(250., 0.);
    for (participants, start) in [(&reaction.reactants, -80.), (&reaction.products, 310.)] {
        for (index, id) in participants[0].atoms.iter().enumerate() {
            let atom = doc.atom_mut(*id).unwrap();
            atom.position = Point::new(start + 25. * index as f32, 0.);
            if atom.map_num == 6 {
                atom.map_num = 0;
            }
        }
    }
    doc.add_atom("O", Point::new(180., -32.));
    (doc, arrow)
}

#[test]
fn palette_counts_assigned_and_inferred_arrows_together() {
    let (mut doc, inferred) = drawn_claisen();
    let mut reaction = reactions::preview(&doc, inferred).unwrap();
    assert!(reaction.ready());
    let assigned = doc.next_id();
    doc.arrows.push(Arrow::new(
        assigned,
        Point::new(110., 400.),
        Point::new(250., 400.),
        Default::default(),
        Default::default(),
    ));
    reaction.arrow = assigned;
    doc.reactions.push(reaction);

    let mut app = app(doc, assigned);
    assert_eq!(app.xtb_review_target(), Some(assigned));
    app.tab.selected.clear();
    assert_eq!(app.xtb_review_target(), None);
    app.tab.doc.reactions.clear();
    app.tab.doc.arrows[1].purpose = reshiki::purpose::Purpose::Illustration;
    assert_eq!(app.xtb_review_target(), Some(inferred));
    app.tab.selected = vec![assigned];
    assert_eq!(app.xtb_review_target(), None);
}

#[test]
fn species_title_formula_uses_plain_digits_and_the_requested_atoms() {
    let (doc, _) = claisen();
    assert_eq!(
        species_formula(&doc, &doc.reactions[0].reactants[0].atoms).as_deref(),
        Some("C5H8O")
    );
}

#[test]
fn barrier_support_uses_inferred_or_current_roles_without_requiring_an_atom_map() {
    let (doc, arrow) = drawn_claisen();
    let mut app = app(doc, arrow);
    assert!(app.xtb_reaction_supports_barrier(arrow));
    assert!(app.tab.doc.reactions.is_empty());
    app.tab.doc.reactions.push(Reaction::new(arrow));
    assert!(!app.xtb_reaction_supports_barrier(arrow));
}

#[test]
fn barrier_support_counts_molecular_species_and_rejects_coefficients() {
    let (mut doc, arrow) = ions(false);
    let left: Vec<_> = doc.reactions[0]
        .reactants
        .iter()
        .flat_map(|participant| participant.atoms.iter().copied())
        .collect();
    doc.add_bond(left[0], left[1], 1, "plain");
    doc.reactions[0].reactants = vec![Participant {
        atoms: left,
        coefficient: 1,
    }];
    let right = doc.reactions[0]
        .products
        .iter()
        .flat_map(|participant| participant.atoms.iter().copied())
        .collect();
    doc.reactions[0].products = vec![Participant {
        atoms: right,
        coefficient: 1,
    }];
    let mut app = app(doc, arrow);
    assert_eq!(
        species::shape(&app.tab.doc, &app.tab.doc.reactions[0]),
        Ok((1, 2))
    );
    assert!(app.xtb_reaction_supports_barrier(arrow));
    let reaction = &mut app.tab.doc.reactions[0];
    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
    assert!(app.xtb_reaction_supports_barrier(arrow));
    app.tab.doc.reactions[0].reactants[0].coefficient = 2;
    assert!(!app.xtb_reaction_supports_barrier(arrow));
    app.tab.doc.reactions[0].reactants[0].coefficient = 1;
    let extra = app.tab.doc.add_atom("O", Point::new(-30., 0.));
    app.tab.doc.reactions[0].reactants[0].atoms.push(extra);
    assert!(!app.xtb_reaction_supports_barrier(arrow));
}

#[test]
fn reviewed_unpaired_electrons_convert_to_internal_multiplicity_and_check_parity() {
    let (doc, _) = ions(false);
    let (mut inputs, _) = species::inputs(&doc, &doc.reactions[0]).unwrap();
    assert!(inputs.iter().all(|input| input.unpaired == "0"));
    inputs[0].unpaired = "2".into();
    assert_eq!(species::validate(&inputs).unwrap()[0].multiplicity, 3);
    inputs[0].unpaired = "1".into();
    assert!(
        species::validate(&inputs).is_err(),
        "electron parity still applies"
    );
    for invalid in ["-1", "0.5", "4294967295"] {
        inputs[0].unpaired = invalid.into();
        assert!(
            species::validate(&inputs)
                .unwrap_err()
                .contains("nonnegative whole-number")
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn unified_calculate_and_rerun_shortcut_follow_the_same_output_choices() {
    for shortcut in [false, true] {
        for (barrier, energy, free, transition_state) in [
            (true, true, false, true),
            (true, false, false, true),
            (false, true, false, false),
            (false, false, true, false),
            (true, false, true, true),
        ] {
            let (doc, arrow) = claisen();
            let mut app = app(doc, arrow);
            app.tab.xtb.ts_card.barrier = barrier;
            app.tab.xtb.ts_card.reaction_energy = energy;
            app.tab.xtb.ts_card.free_energy = free;
            assert!(app.xtb_can_calculate_reaction(arrow));
            let action = if shortcut {
                XtbAction::Entry(super::super::super::Entry::Rerun)
            } else {
                XtbAction::Ts(TsAction::Calculate(arrow))
            };
            let _ = app.update(Message::Xtb(action));
            let run = app
                .tab
                .xtb
                .run
                .as_ref()
                .expect("the chosen calculation starts");
            assert_eq!(run.ts.is_some(), transition_state);
            if let Some(ts) = &run.ts {
                assert_eq!(ts.request.free_energy, free);
            } else {
                assert_eq!(
                    run.config.task,
                    if free {
                        CalcTask::OptFreq
                    } else {
                        CalcTask::Optimize
                    }
                );
            }
            assert!(!app.xtb_can_calculate_reaction(arrow));
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn unsupported_barrier_falls_back_to_energy_and_no_effective_output_blocks_calculate() {
    let (mut doc, arrow) = ions(false);
    for atom in &mut doc.atoms {
        atom.element = if atom.map_num == 1 { "O" } else { "C" }.into();
        atom.charge = 0;
    }
    for (energy, free) in [(false, false), (true, false), (false, true)] {
        let mut app = app(doc.clone(), arrow);
        app.tab.xtb.ts_card.reaction_energy = energy;
        app.tab.xtb.ts_card.free_energy = free;
        assert!(!app.xtb_reaction_supports_barrier(arrow));
        assert_eq!(app.xtb_can_calculate_reaction(arrow), energy || free);
        let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Calculate(arrow))));
        assert_eq!(app.xtb_running(), energy || free);
        if let Some(run) = &app.tab.xtb.run {
            assert!(run.ts.is_none());
            assert_eq!(
                run.config.task,
                if free {
                    CalcTask::OptFreq
                } else {
                    CalcTask::Optimize
                }
            );
        }
    }
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    app.tab.xtb.ts_card.reaction_energy = false;
    app.tab.xtb.ts_card.barrier = false;
    assert!(!app.xtb_can_calculate_reaction(arrow));
    let _ = app.update(Message::Xtb(XtbAction::Entry(
        super::super::super::Entry::Rerun,
    )));
    assert!(!app.xtb_running());
    assert!(!app.ts_review_open());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn refused_barrier_runs_the_energy_batch_from_calculate_and_rerun() {
    use reshiki::reactions::Role;

    for (carbons, mapped, reason) in [
        (2, true, "no bonds forming or breaking"),
        (41, true, "up to 120 atoms"),
        (129, false, "up to 120 atoms"),
    ] {
        let (mut doc, arrow) = ions(false);
        doc.atoms.clear();
        for side in [Role::Reactant, Role::Product] {
            let mut atoms = Vec::new();
            for i in 0..carbons {
                let id = doc.add_atom("C", Point::new(i as f32 * 24., 0.));
                if mapped {
                    doc.atom_mut(id).unwrap().map_num = i + 1;
                }
                if let Some(&previous) = atoms.last() {
                    doc.add_bond(previous, id, 1, "plain");
                }
                atoms.push(id);
            }
            let participants = match side {
                Role::Reactant => &mut doc.reactions[0].reactants,
                Role::Product => &mut doc.reactions[0].products,
                Role::Agent => unreachable!(),
            };
            *participants = vec![Participant {
                atoms,
                coefficient: 1,
            }];
        }
        assert!(
            barrier_refusal(&ts_plan::plan_for_review(&doc, arrow))
                .unwrap()
                .contains(reason)
        );
        for action in [
            XtbAction::Ts(TsAction::Calculate(arrow)),
            XtbAction::Entry(super::super::super::Entry::Rerun),
        ] {
            let mut app = app(doc.clone(), arrow);
            let _ = app.update(Message::Xtb(action));
            assert!(!app.ts_review_open());
            assert!(app.tab.xtb.run.as_ref().is_some_and(|run| run.ts.is_none()));
            assert!(app.tab.xtb.energy_state.in_progress());
            assert!(
                app.xtb_energy_result(arrow, None)
                    .1
                    .unwrap()
                    .contains(reason)
            );
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn failed_automatic_mapping_falls_back_only_when_reaction_energy_is_selected() {
    let (mut doc, arrow) = claisen();
    for atom in &mut doc.atoms {
        atom.map_num = 0;
    }
    let reason = "Could not find atom correspondence";
    for (energy, free) in [(true, false), (false, true), (false, false)] {
        let mut app = app(doc.clone(), arrow);
        app.tab.xtb.ts_card.reaction_energy = energy;
        app.tab.xtb.ts_card.free_energy = free;
        let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Calculate(arrow))));
        assert!(app.xtb_automatic_mapping_pending(arrow));
        let id = app.tab.xtb.ts_review.as_ref().unwrap().id;
        let _ = app.update(send(Action::Ready(id, Err(reason.into()))));
        assert_eq!(app.tab.doc, doc);
        if energy || free {
            assert!(!app.ts_review_open());
            let run = app.tab.xtb.run.as_ref().unwrap();
            assert!(run.ts.is_none());
            assert_eq!(
                run.config.task,
                if free {
                    CalcTask::OptFreq
                } else {
                    CalcTask::Optimize
                }
            );
            assert_eq!(app.xtb_energy_result(arrow, None).1, Some(reason));
        } else {
            let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
            assert_eq!(dialog.mode, Mode::TransitionState);
            assert_eq!(dialog.validate().unwrap_err(), reason);
            assert!(!app.xtb_running());
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn automatic_map_refusal_stays_with_the_energy_batch_after_species_storage() {
    use crate::app::xtb::tests::{MOVED, START, event, optimized};
    use reshiki::geometry::Point3;

    let (mut doc, arrow) = ions(false);
    doc.atoms.retain(|atom| atom.map_num == 1);
    for atom in &mut doc.atoms {
        atom.map_num = 0;
        atom.charge = 0;
        atom.element = "O".into();
    }
    doc.reactions[0].reactants.truncate(1);
    doc.reactions[0].products.truncate(1);
    let mut app = app(doc, arrow);
    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Calculate(arrow))));
    finish_mapping(&mut app);
    let refusal = "The atom map shows no bonds forming or breaking and no hydrogen moves";
    assert_eq!(app.xtb_energy_result(arrow, None).1, Some(refusal));
    assert!(app.tab.doc.atoms.iter().all(|atom| atom.map_num == 0));

    for _ in 0..2 {
        let positions = START.iter().map(|&[x, y, z]| Point3 { x, y, z }).collect();
        let _ = app.finish_model(positions, vec![0, 0]);
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.xtb_energy_result(arrow, None).1, Some(refusal));
    }
    assert_eq!(app.tab.doc.science.results.len(), 2);
    assert!(app.xtb_energy_result(arrow, None).0.is_some());
    app.tab.doc.atoms[0].isotope = 18;
    assert_eq!(app.xtb_energy_result(arrow, None).1, None);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn free_energy_choice_is_kept_while_a_review_waits() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    app.tab.xtb.ts_card.free_energy = true;
    let _ = app.xtb_review_show(Mode::Energy, Some(arrow));
    assert!(!app.xtb_can_calculate_reaction(arrow));
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(dialog.task, CalcTask::OptFreq);
    app.tab.xtb.ts_card.free_energy = false;
    let _ = app.update(send(Action::Accept));
    assert_eq!(
        app.tab.xtb.run.as_ref().unwrap().config.task,
        CalcTask::OptFreq
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn finish_mapping(app: &mut App) {
    finish_mapping_with(app, |_| {});
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn finish_mapping_with(app: &mut App, change: impl FnOnce(&mut Proposal)) {
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(matches!(dialog.mapping, Mapping::Finding));
    let document = dialog
        .scheme
        .as_ref()
        .map_or(&app.tab.doc, |(_, document)| document.as_ref());
    let source = with_roles(document, dialog.arrow).unwrap();
    let mut proposal =
        reshiki::reaction_mapping::propose(&source, dialog.arrow, Default::default()).unwrap();
    change(&mut proposal);
    let _ = app.update(send(Action::Ready(dialog.id, Ok(Arc::new(proposal)))));
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn sure_auto_map_starts_silently_and_commits_inferred_roles_and_maps_together() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    assert!(
        !app.ts_review_visible(),
        "A sure map is found without a modal"
    );
    assert!(app.xtb_automatic_mapping_pending(arrow));
    assert!(!app.xtb_automatic_mapping_pending(arrow + 1));
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert_eq!(app.xtb.page, Page::Xtb);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.inferred);
    assert_eq!(dialog.inputs.len(), 2);
    assert_eq!(
        dialog.ignored.as_deref(),
        Some("Ignored: reagents and conditions · H2O")
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    finish_mapping_with(&mut app, |proposal| {
        assert!(proposal.complete && !proposal.multiple_best);
    });
    assert_eq!(
        Changes::from_plan(&app.tab.doc, &scheme_plan(&app.tab.doc, arrow).unwrap()).bonds,
        "forms C1–C6 · breaks O3–C4"
    );
    assert!(!app.ts_review_open());
    assert!(!app.xtb_automatic_mapping_pending(arrow));
    assert!(app.xtb_running());
    assert_eq!(app.tab.doc.reactions.len(), 1);
    assert_eq!(app.tab.doc.reactions[0].agents.len(), 1);
    assert_eq!(app.xtb_reaction_bond_marks().len(), 2);
    assert!(
        app.tab.doc.reactions[0].reactants[0]
            .atoms
            .iter()
            .all(|id| app.tab.doc.atom(*id).unwrap().map_num != 0)
    );
    assert!(app.tab.history.can_undo());
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert_eq!(
        app.tab
            .xtb
            .run
            .as_ref()
            .unwrap()
            .ts
            .as_ref()
            .unwrap()
            .request
            .images,
        15
    );
    let _ = app.update(Message::Xtb(XtbAction::Cancel));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.reactions, before.reactions);
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .zip(before.atoms)
            .all(|(after, before)| after.map_num == before.map_num)
    );
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn edit_map_hands_off_the_inferred_proposal_without_editing_the_drawing() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    assert!(app.ts_review_visible());
    finish_mapping(&mut app);
    let _ = app.update(send(Action::EditMap));
    assert!(!app.ts_review_open());
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.mapping.proposal().is_some());
    let _ = app.update(Message::Mapping(
        crate::app::reaction_mapping::Action::ApplyProposal,
    ));
    assert_eq!(app.tab.doc.reactions.len(), 1);
    assert!(app.tab.history.can_undo());
    assert!(!app.xtb_running());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn an_ambiguous_added_species_map_can_skip_its_barrier_and_keep_the_scheme_open() {
    let (doc, plan) = scheme::plan::Plan::example();
    let before = doc.clone();
    let mut app = self::app(doc, plan.arrows[0]);
    app.tab.selected = plan.arrows;
    let scheme = |action| Message::Xtb(XtbAction::Ts(TsAction::Scheme(action)));
    let _ = app.update(scheme(scheme::Action::Open));
    for quantity in [scheme::Quantity::FreeEnergy, scheme::Quantity::FreeBarrier] {
        let _ = app.update(scheme(scheme::Action::Toggle(0, quantity)));
    }
    let _ = app.update(scheme(scheme::Action::Run));
    finish_mapping_with(&mut app, |proposal| proposal.multiple_best = true);
    assert!(app.ts_review_visible());
    assert!(app.tab.xtb.ts_review.as_ref().unwrap().detached_map);
    let _ = app.update(send(Action::EditMap));
    assert!(
        app.ts_review_visible(),
        "The detached map cannot open in the drawing"
    );
    let _ = app.update(send(Action::SkipBarrier));
    assert!(!app.ts_review_open());
    assert!(app.xtb_scheme_running());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
async fn next_scheme_review(task: Task<Message>) -> Message {
    use iced::futures::StreamExt;

    let mut stream = iced_runtime::task::into_stream(task).expect("continue the map queue");
    while let Some(event) = stream.next().await {
        let iced_runtime::Action::Output(message) = event else {
            continue;
        };
        let action = match &message {
            Message::Tab(_, inner) => inner.as_ref(),
            message => message,
        };
        if matches!(
            action,
            Message::Xtb(XtbAction::Ts(TsAction::Scheme(scheme::Action::Next(_))))
        ) {
            return message;
        }
    }
    panic!("a failed map must queue the following step");
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[tokio::test]
async fn an_unbuildable_added_hydrogen_map_keeps_energy_and_queues_the_next_step() {
    for free_energy in [false, true] {
        let (doc, plan) = scheme::plan::Plan::example();
        let before = doc.clone();
        let second = plan.arrows[1];
        let mut app = self::app(doc, second);
        app.tab.selected = plan.arrows;
        let scheme = |action| Message::Xtb(XtbAction::Ts(TsAction::Scheme(action)));
        let _ = app.update(scheme(scheme::Action::Open));
        // Only step 2 needs a map, and its electronic endpoint energy was off.
        let _ = app.update(scheme(scheme::Action::Toggle(0, scheme::Quantity::Barrier)));
        let _ = app.update(scheme(scheme::Action::Toggle(1, scheme::Quantity::Energy)));
        if free_energy {
            let _ = app.update(scheme(scheme::Action::Toggle(
                1,
                scheme::Quantity::FreeEnergy,
            )));
            let _ = app.update(scheme(scheme::Action::Toggle(
                1,
                scheme::Quantity::FreeBarrier,
            )));
        }
        let _ = app.update(scheme(scheme::Action::Run));
        assert!(app.xtb_automatic_mapping_pending(second));
        assert!(!app.ts_review_visible());
        let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
        let id = dialog.id;
        let (_, document) = dialog.scheme.as_ref().unwrap();
        let source = with_roles(document, second).unwrap();
        let proposal =
            reshiki::reaction_mapping::propose(&source, second, Default::default()).unwrap();
        assert_eq!(proposal.unmatched_reactants, 2);
        let candidate = proposal.apply(&source, true).unwrap();
        assert!(ts_plan::plan_for_review(&candidate, second).is_err());
        let task = app.update(send(Action::Ready(id, Ok(Arc::new(proposal)))));
        assert!(!app.ts_review_open());
        assert!(app.xtb_scheme_running());
        let state = app.tab.xtb.scheme.as_ref().unwrap();
        let (options, notes) = state.reviewed_step(1).unwrap();
        assert_eq!(
            options,
            scheme::run::StepOptions {
                reaction_energy: true,
                barrier: false,
                free_energy,
                free_barrier: false,
            }
        );
        assert!(
            notes.iter().any(|note| {
                note == "Barrier skipped: the map could not include the added H₂"
            })
        );
        let _ = next_scheme_review(task).await;
        // The next event starts native work. This test stops at its boundary.
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(scheme(scheme::Action::Cancel));
        let (options, notes) = app
            .tab
            .xtb
            .scheme
            .as_ref()
            .unwrap()
            .reviewed_step(1)
            .unwrap();
        assert_eq!(options, scheme::run::StepOptions::default());
        assert!(
            notes
                .iter()
                .all(|note| !note.starts_with("Barrier skipped:"))
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[tokio::test]
async fn a_failed_map_continues_to_the_next_barrier_review() {
    let (doc, plan) = scheme::plan::Plan::example();
    let first = plan.arrows[0];
    let second = plan.arrows[1];
    let mut app = self::app(doc.clone(), first);
    app.tab.selected = plan.arrows;
    for action in [scheme::Action::Open, scheme::Action::Run] {
        let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Scheme(action))));
    }
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(dialog.arrow, first);
    let task = app.update(send(Action::Ready(
        dialog.id,
        Err("The atom map could not be built".into()),
    )));
    let next = next_scheme_review(task).await;
    let _ = app.update(next);
    assert_eq!(app.tab.xtb.ts_review.as_ref().unwrap().arrow, second);
    assert!(app.ts_review_open());
    let (options, _) = app
        .tab
        .xtb
        .scheme
        .as_ref()
        .unwrap()
        .reviewed_step(0)
        .unwrap();
    assert!(options.reaction_energy);
    assert!(!options.barrier && !options.free_barrier);
    assert_eq!(app.tab.doc, doc);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_drawn_only_scheme_map_still_opens_in_the_drawing() {
    let (mut doc, first) = hydrogen_transfer();
    let mut arrow = doc.arrows[0].clone();
    arrow.id = doc.next_id();
    let second = arrow.id;
    doc.arrows.push(arrow);
    let mut reverse = doc.reactions[0].clone();
    reverse.arrow = second;
    std::mem::swap(&mut reverse.reactants, &mut reverse.products);
    doc.reactions.push(reverse);
    let before = doc.clone();
    let mut app = self::app(doc, first);
    app.tab.selected = vec![first, second];
    for action in [scheme::Action::Open, scheme::Action::Run] {
        let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Scheme(action))));
    }
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(!dialog.detached_map);
    let _ = app.update(send(Action::SkipBarrier));
    assert!(app.ts_review_open());
    let _ = app.update(send(Action::EditMap));
    assert!(!app.ts_review_open());
    assert!(!app.xtb_scheme_running());
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert_eq!(app.tab.doc, before);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_scheme_proposal_handoff_can_auto_map_and_apply_the_full_drawing() {
    use crate::app::reaction_mapping::Action as MapAction;

    let (mut doc, first) = hydrogen_transfer();
    for atom in &mut doc.atoms {
        atom.map_num = 0;
    }
    let mut arrow = doc.arrows[0].clone();
    arrow.id = doc.next_id();
    let second = arrow.id;
    doc.arrows.push(arrow);
    let mut reverse = doc.reactions[0].clone();
    reverse.arrow = second;
    std::mem::swap(&mut reverse.reactants, &mut reverse.products);
    doc.reactions.push(reverse);
    let before = doc.clone();
    let mut app = self::app(doc, first);
    app.tab.selected = vec![first, second];
    for action in [scheme::Action::Open, scheme::Action::Run] {
        let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Scheme(action))));
    }
    finish_mapping_with(&mut app, |proposal| proposal.multiple_best = true);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.mapping.proposal().is_some());
    assert!(!dialog.detached_map);
    let arrow = dialog.arrow;
    let _ = app.update(send(Action::EditMap));
    assert!(!app.ts_review_open());
    assert!(!app.xtb_scheme_running());
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.mapping.proposal().is_none());

    let _ = app.update(Message::Mapping(MapAction::Auto));
    let (epoch, serial) = app.tab.mapping.pending.expect("Auto-map must be available");
    let proposal = Arc::new(
        reshiki::reaction_mapping::propose(&app.tab.doc, arrow, Default::default()).unwrap(),
    );
    let _ = app.update(Message::Mapping(MapAction::Ready(
        epoch,
        serial,
        Ok(proposal),
    )));
    assert!(app.tab.mapping.proposal().unwrap().current(&app.tab.doc));
    let _ = app.update(Message::Mapping(MapAction::ApplyProposal));
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.doc.atoms.iter().any(|atom| atom.map_num != 0));
    assert_eq!(app.tab.doc.arrows, before.arrows);
    assert!(app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn complete_map_with_moving_hydrogen_requires_the_sheet() {
    let (doc, arrow) = hydrogen_transfer();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(matches!(dialog.mapping, Mapping::Drawing));
    assert_eq!(
        dialog.changes.as_ref().unwrap().hydrogens,
        ["H from C2 to O3 (least motion)"]
    );
    assert!(dialog.validate().is_ok());
    let _ = app.update(send(Action::Cancel));
    assert!(!app.ts_review_open());
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn proton_transfer_with_barrier_off_reviews_and_runs_reaction_energy() {
    let (doc, arrow) = hydrogen_transfer();
    let mut app = app(doc, arrow);
    app.tab.xtb.ts_card.barrier = false;
    app.tab.xtb.ts_card.reaction_energy = true;
    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Calculate(arrow))));
    assert!(app.ts_review_visible());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(dialog.changes.as_ref().unwrap().action, "transfer hydrogen");
    assert_eq!(
        dialog.changes.as_ref().unwrap().hydrogens,
        ["H from C2 to O3 (least motion)"]
    );
    assert_eq!(dialog.heading(), "Calculate reaction energy");
    assert_eq!(dialog.run_label(), "Calculate reaction energy");

    let _ = app.update(send(Action::Accept));
    assert!(!app.ts_review_open());
    let run = app.tab.xtb.run.as_ref().expect("reaction energy starts");
    assert!(run.ts.is_none());
    assert_eq!(run.config.task, CalcTask::Optimize);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_sure_proposal_with_moving_hydrogen_still_waits_for_review() {
    let (mut doc, arrow) = hydrogen_transfer();
    for atom in &mut doc.atoms {
        if atom.map_num == 1 {
            atom.map_num = 0;
        }
    }
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(app.xtb_automatic_mapping_pending(arrow));
    finish_mapping_with(&mut app, |proposal| {
        assert!(proposal.complete && !proposal.multiple_best);
    });
    assert!(app.ts_review_visible());
    assert!(!app.xtb_automatic_mapping_pending(arrow));
    assert!(!app.xtb_running());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(
        dialog.changes.as_ref().unwrap().hydrogens,
        ["H from C2 to O3 (least motion)"]
    );
    assert!(dialog.validate().is_ok());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn missing_isotope_maps_open_auto_map_with_an_explanation() {
    let (mut doc, arrow) = claisen();
    let attachments = [
        (doc.reactions[0].reactants[0].atoms[0], false),
        (doc.reactions[0].products[0].atoms[3], true),
    ];
    for (parent, product) in attachments {
        let position = doc.atom(parent).unwrap().position;
        let h = doc.add_atom("H", Point::new(position.x, position.y + 25.));
        doc.atom_mut(h).unwrap().isotope = 2;
        doc.add_bond(parent, h, 1, "plain");
        let reaction = &mut doc.reactions[0];
        let side = if product {
            &mut reaction.products
        } else {
            &mut reaction.reactants
        };
        side[0].atoms.push(h);
    }
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.unmapped_isotope_hydrogen);
    assert!(app.ts_review_visible());
    assert!(matches!(dialog.mapping, Mapping::Finding));
    finish_mapping(&mut app);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.unmapped_isotope_hydrogen);
    assert!(dialog.validate().is_ok());
    assert!(!app.xtb_running());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn mapper_ties_and_unfinished_searches_require_review_before_applying() {
    for tied in [true, false] {
        let (doc, arrow) = drawn_claisen();
        let before = doc.clone();
        let mut app = app(doc, arrow);
        open(&mut app, Mode::TransitionState);
        finish_mapping_with(&mut app, |proposal| {
            proposal.multiple_best = tied;
            proposal.complete = tied;
        });
        assert!(app.ts_review_visible());
        assert!(!app.xtb_running());
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
        assert!(dialog.needs_review());
        assert!(dialog.validate().is_ok());
        assert_eq!(dialog.subtitle().starts_with("Several atom maps fit"), tied);
        assert!(dialog.pictures.is_some());
        let changes = dialog.changes.as_ref().unwrap();
        assert!(!changes.marks.is_empty());
        assert_eq!(
            changes.marks.len(),
            dialog.plan.as_ref().unwrap().form.len() + dialog.plan.as_ref().unwrap().break_.len()
        );
        let _ = app.update(send(Action::Accept));
        assert!(!app.ts_review_open());
        assert!(app.xtb_running());
        assert_eq!(app.tab.history.undo_frames().len(), 1);
    }
}

#[test]
fn review_picture_and_run_action_describe_the_same_bonds() {
    let (doc, arrow) = claisen();
    let plan = ts_plan::plan_for_review(&doc, arrow).unwrap();
    let changes = Changes::from_plan(&doc, &plan);
    assert_eq!(changes.bonds, "forms C1–C6 · breaks O3–C4");
    assert_eq!(changes.action, "form C1–C6, break O3–C4");
    assert_eq!(changes.marks.len(), 2);
    assert!(matches!(changes.marks[0].kind, BondChangeKind::Form));
    assert!(matches!(changes.marks[1].kind, BondChangeKind::Break));
    for mark in &changes.marks {
        assert!(mark.atoms.iter().all(|id| plan.start_atoms.contains(id)));
        assert!(!mark.highlighted);
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn diels_alder_review_crosses_product_bonds_and_draws_reactant_arcs() {
    let (doc, arrow) = diels_alder();
    let mut app = app(doc, arrow);
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(dialog.plan.as_ref().unwrap().start, Side::Product);
    let changes = dialog.changes.as_ref().unwrap();
    assert_eq!(changes.bonds, "forms C1–C6, C4–C5");
    assert_eq!(changes.action, "form C1–C6, C4–C5");
    let [start, other] = dialog.pictures.as_ref().unwrap();
    assert_eq!(start.label, "Products (search start)");
    assert_eq!(other.label, "Reactants");
    let doc = &app.tab.doc;
    let pairs = |picture: &Picture, kind| {
        picture
            .marks
            .iter()
            .map(|mark| {
                assert_eq!(mark.kind, kind);
                let [a, b] = mark.atoms;
                assert_eq!(
                    doc.bond_between(a, b).is_some(),
                    kind == BondChangeKind::Break
                );
                mark.atoms.map(|id| doc.atom(id).unwrap().map_num)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(pairs(start, BondChangeKind::Break), [[1, 6], [4, 5]]);
    assert_eq!(pairs(other, BondChangeKind::Form), [[1, 6], [4, 5]]);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn review_settings_start_folded_and_the_summary_tracks_edits() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(
        dialog.state_summary(),
        "All species neutral, no unpaired electrons"
    );
    assert_eq!(dialog.run_label(), "Run: form C1–C6, break O3–C4");
    assert!(!dialog.edit_species);
    assert!(!dialog.advanced);
    let _ = app.update(send(Action::EditSpecies));
    let _ = app.update(send(Action::Advanced));
    let _ = app.update(send(Action::Charge(0, "−2".into())));
    let _ = app.update(send(Action::Unpaired(0, "2".into())));
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.edit_species);
    assert!(dialog.advanced);
    assert_eq!(
        dialog.state_summary(),
        "1 charged species, 1 species with unpaired electrons"
    );
    let _ = app.update(send(Action::EditSpecies));
    let _ = app.update(send(Action::Advanced));
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(!dialog.edit_species);
    assert!(!dialog.advanced);
    assert!(
        dialog.validate().is_err(),
        "folding does not bypass charge balance"
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[tokio::test]
async fn accepting_or_canceling_the_review_returns_focus_to_the_drawing() {
    use iced::advanced::widget::operation::Focusable;
    use iced::futures::StreamExt;

    struct Focus(bool);
    impl Focusable for Focus {
        fn is_focused(&self) -> bool {
            self.0
        }
        fn focus(&mut self) {
            self.0 = true;
        }
        fn unfocus(&mut self) {
            self.0 = false;
        }
    }

    for action in [Action::Accept, Action::Cancel] {
        let accept = matches!(action, Action::Accept);
        let (doc, arrow) = claisen();
        let mut app = app(doc, arrow);
        let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
        let task = app.xtb_review_action(action);
        assert!(!app.ts_review_open());
        assert_eq!(app.xtb_running(), accept);
        let mut stream = iced_runtime::task::into_stream(task).expect("focus task");
        let Some(iced_runtime::Action::Widget(mut operation)) = stream.next().await else {
            panic!("closing the review must first restore drawing focus");
        };
        let mut field = Focus(true);
        operation.focusable(None, iced::Rectangle::default(), &mut field);
        assert!(!field.is_focused(), "the next key must reach the drawing");
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn review_shows_the_proposal_and_reveals_settings_only_when_opened() {
    use crate::app::xtb::ts::card::tests::ViewHarness;
    use iced::widget::text;

    let ui = ViewHarness::new().await;
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    let snapshot = ui.inspect(app.with_ts_review(text("").into()));
    for expected in [
        "Which bonds change?",
        "Check the bonds that change.",
        "Reactants (search start)",
        "Products",
        "forms C1–C6 · breaks O3–C4",
        "All species neutral, no unpaired electrons",
        "Run: form C1–C6, break O3–C4",
    ] {
        assert!(
            snapshot.texts.iter().any(|text| text == expected),
            "{expected}"
        );
    }
    assert!(snapshot.semantics.duplicate_ids.is_empty());
    assert!(
        snapshot
            .semantics
            .nodes
            .iter()
            .all(|node| { node.id != "ts.review.path-points" && node.id != "ts.review.0.Charge" })
    );
    for id in ["ts.review.state", "ts.review.advanced"] {
        let action = ui
            .activate(app.with_ts_review(text("").into()), id)
            .unwrap();
        let _ = app.update(action);
    }
    let opened = ui.inspect(app.with_ts_review(text("").into()));
    for id in [
        "ts.review.path-points",
        "ts.review.0.Charge",
        "ts.review.0.Unpaired electrons",
    ] {
        assert!(
            opened.semantics.nodes.iter().any(|node| node.id == id),
            "{id}"
        );
    }
    assert!(opened.texts.iter().any(|text| text == "Unpaired electrons"));
    assert!(matches!(
        ui.activate(app.with_ts_review(text("").into()), "ts.review.accept"),
        Some(Message::Xtb(XtbAction::Ts(TsAction::ReviewSheet(
            Action::Accept
        ))))
    ));

    let (doc, arrow) = drawn_claisen();
    let mut tied = self::app(doc, arrow);
    let _ = tied.xtb_review_show(Mode::TransitionState, Some(arrow));
    finish_mapping_with(&mut tied, |proposal| proposal.multiple_best = true);
    let proposal = ui.inspect(tied.with_ts_review(text("").into()));
    assert_eq!(
        proposal
            .texts
            .iter()
            .filter(|text| *text == "Proposed changes")
            .count(),
        1
    );
    assert!(
        proposal
            .texts
            .iter()
            .any(|text| text.starts_with("Several atom maps fit"))
    );
    assert!(
        ui.activate(tied.with_ts_review(text("").into()), "ts.review.edit")
            .is_some()
    );
    assert!(
        proposal
            .texts
            .iter()
            .all(|text| !text.contains("Two mechanisms"))
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn explicit_review_waits_even_with_a_complete_sure_map() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    app.tab.selected.clear();
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    assert!(app.ts_review_visible());
    assert!(!app.xtb_running());
    assert_eq!(app.tab.selected, [arrow]);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert!(app.tab.xtb.ts_review.as_ref().unwrap().validate().is_ok());
    let _ = app.update(send(Action::PathPoints("21".into())));
    assert!(app.tab.xtb.ts_review.as_ref().unwrap().validate().is_err());
    let _ = app.update(send(Action::PathPoints("18".into())));
    assert_eq!(
        app.tab.xtb.ts_review.as_ref().unwrap().images().unwrap(),
        18
    );
    let _ = app.update(send(Action::Cancel));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_complete_sure_map_starts_from_the_sole_target_without_review() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    app.tab.selected.clear();
    open(&mut app, Mode::TransitionState);
    assert!(!app.ts_review_open());
    assert!(app.xtb_running());
    assert_eq!(app.tab.selected, [arrow]);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert_eq!(app.xtb.page, Page::Xtb);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_stale_silent_map_never_edits_or_starts_the_changed_drawing() {
    let (doc, arrow) = drawn_claisen();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    let id = dialog.id;
    let source = with_roles(&app.tab.doc, arrow).unwrap();
    let proposal = reshiki::reaction_mapping::propose(&source, arrow, Default::default()).unwrap();
    app.tab.revision += 1;
    assert!(
        !app.ts_review_open(),
        "a stale hidden request must not lock the card"
    );
    let _ = app.update(send(Action::Ready(id, Ok(Arc::new(proposal)))));
    assert!(!app.ts_review_open());
    assert!(!app.xtb_running());
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.doc.reactions.is_empty());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_sure_automatic_map_keeps_a_newer_selection() {
    let (doc, arrow) = drawn_claisen();
    let atom = doc.atoms[0].id;
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    app.tab.selected = vec![atom];
    finish_mapping(&mut app);
    assert!(app.xtb_running());
    assert_eq!(app.tab.selected, [atom]);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn cancelling_silent_mapping_discards_a_late_proposal() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    let id = dialog.id;
    let source = with_roles(&app.tab.doc, arrow).unwrap();
    let proposal = reshiki::reaction_mapping::propose(&source, arrow, Default::default()).unwrap();
    let _ = app.update(send(Action::Cancel));
    assert!(!app.xtb_automatic_mapping_pending(arrow));
    assert_eq!(app.status, "Atom matching canceled");
    let _ = app.update(send(Action::Ready(id, Ok(Arc::new(proposal)))));
    assert!(!app.ts_review_open());
    assert!(!app.xtb_running());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn reviewed_charges_set_the_run_state_and_incompatible_unpaired_counts_block_it() {
    let (doc, arrow) = crate::app::xtb::ts::card::tests::hydrogen_transfer();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    let _ = app.update(send(Action::Charge(0, "−2".into())));
    assert!(
        app.tab
            .xtb
            .ts_review
            .as_ref()
            .unwrap()
            .validate()
            .unwrap_err()
            .contains("charge +2")
    );
    let _ = app.update(send(Action::Charge(1, "-2".into())));
    let _ = app.update(send(Action::Unpaired(1, "2".into())));
    assert!(
        app.tab
            .xtb
            .ts_review
            .as_ref()
            .unwrap()
            .validate()
            .unwrap_err()
            .contains("cannot share one spin state")
    );
    let _ = app.update(send(Action::Unpaired(1, "0".into())));
    let _ = app.update(send(Action::Accept));
    assert_eq!(app.tab.xtb.run.as_ref().unwrap().config.charge, -2);
    assert!(app.tab.doc.atoms.iter().all(|atom| atom.charge == 0));
}

fn ions(extra_chloride: bool) -> (Document, u64) {
    let mut doc = Document::default();
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(50., 0.),
        Point::new(150., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    for (product, atoms) in [
        (false, &mut reaction.reactants),
        (true, &mut reaction.products),
    ] {
        let x = if product { 185. } else { 0. };
        let na = doc.add_atom("Na", Point::new(x, 0.));
        let sodium = doc.atom_mut(na).unwrap();
        sodium.charge = 1;
        sodium.map_num = 1;
        atoms.push(Participant {
            atoms: vec![na],
            coefficient: 1,
        });
        if product || !extra_chloride {
            let cl = doc.add_atom("Cl", Point::new(x + 22., 0.));
            let chloride = doc.atom_mut(cl).unwrap();
            chloride.charge = -1;
            chloride.map_num = 2;
            atoms.push(Participant {
                atoms: vec![cl],
                coefficient: 1,
            });
        }
    }
    doc.reactions.push(reaction);
    (doc, arrow)
}

#[test]
fn imbalance_names_the_element_and_charge_difference() {
    let (doc, _) = ions(true);
    let (inputs, _) = species::inputs(&doc, &doc.reactions[0]).unwrap();
    assert_eq!(
        species::validate(&inputs).unwrap_err(),
        "Products have one more Cl and charge −1 relative to reactants"
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn imbalance_blocks_accept_and_two_to_two_allows_reaction_energy() {
    let (doc, arrow) = ions(true);
    let mut blocked = app(doc, arrow);
    open(&mut blocked, Mode::Energy);
    assert!(blocked.ts_review_visible());
    let dialog = blocked.tab.xtb.ts_review.as_ref().unwrap();
    assert!(
        dialog
            .validate()
            .unwrap_err()
            .contains("one more Cl and charge −1")
    );
    let _ = blocked.update(send(Action::Accept));
    assert!(!blocked.xtb_running());

    let (mut doc, arrow) = ions(false);
    for atom in &mut doc.atoms {
        atom.element = if atom.map_num == 1 { "O" } else { "C" }.into();
        atom.charge = 0;
    }
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.ts_review_open());
    assert_eq!(app.status, SHAPE_NOTE);
    assert!(!app.xtb_running());
    open(&mut app, Mode::Energy);
    assert!(!app.ts_review_open());
    assert!(app.xtb_running(), "{}", app.status);
}
