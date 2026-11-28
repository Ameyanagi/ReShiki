use super::*;
use crate::app::xtb::{
    self,
    ts::scheme::{
        State,
        plan::Plan,
        run::{self, DiscoveredGap, DiscoveredMinimum, DiscoveredSegment, StepOptions, StepValue},
    },
};
use reshiki::qcdata::{CalcResult, Status, Task, XtbMethod};
use std::{cell::Cell, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
#[path = "evidence.rs"]
mod evidence;

fn water(shift: f64) -> Arc<Frame> {
    Arc::new(Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: vec![
            [shift, 0., 0.],
            [shift + 0.97, 0., 0.],
            [shift - 0.24, 0.93, 0.],
        ],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![Some(101), None, None],
    })
}

fn result(frame: &Frame) -> Arc<CalcResult> {
    let mut result = xtb::example_computed(-5., 1.)
        .to_calc_result(frame, Task::Optimize, &xtb::example_engine(), None)
        .unwrap();
    result.task = Task::TransitionState;
    result.endpoints = Some(reshiki::qcdata::Endpoints {
        reactant: frame.hash(),
        product: frame.hash(),
    });
    result.status = Status::Converged;
    Arc::new(result)
}

fn path(count: usize) -> DiscoveredPath {
    let minima: Vec<_> = (0..count)
        .map(|i| DiscoveredMinimum {
            frame: water(i as f64 * 0.01),
            energy_hartree: -5. - i as f64 * 0.01,
            gibbs_correction_hartree: None,
        })
        .collect();
    let segments = (0..count.saturating_sub(1))
        .map(|i| DiscoveredSegment {
            reactant: i,
            product: i + 1,
            ts: result(&minima[i].frame),
            frames: vec![minima[i].frame.clone(), minima[i + 1].frame.clone()],
        })
        .collect();
    DiscoveredPath {
        minima,
        order: (0..count).collect(),
        segments,
        gaps: vec![],
        diagnostics: vec![],
    }
}

fn report(path: DiscoveredPath, arrow: u64) -> Report {
    Report {
        species: vec![],
        steps: vec![Some(StepValue {
            arrow,
            method: XtbMethod::Gfn2Xtb,
            transition_state_requested: true,
            ts: None,
            discovered: Some(path),
            frames: vec![],
            launch: None,
            energy: None,
            gibbs: None,
            barrier: None,
            free_barrier: None,
            error: None,
        })],
    }
}

#[test]
fn ordered_minima_become_editable_structures_with_shared_explicit_roles() {
    let (original, _) = Plan::example();
    let mut discovered = path(3);
    discovered.order = vec![2, 0, 1];
    discovered.segments[0].reactant = 2;
    discovered.segments[0].product = 0;
    discovered.segments[1].reactant = 0;
    discovered.segments[1].product = 1;
    let expected: Vec<_> = discovered
        .order
        .iter()
        .map(|&i| discovered.minima[i].frame.clone())
        .collect();
    let report = report(discovered, original.arrows[0].id);
    let placed = build(&original, &report).unwrap();
    let doc = &placed.document;
    doc.validate().unwrap();
    assert!(placed.warnings.is_empty());
    assert_eq!(&doc.atoms[..original.atoms.len()], original.atoms);
    assert_eq!(&doc.bonds[..original.bonds.len()], original.bonds);
    assert_eq!(&doc.arrows[..original.arrows.len()], original.arrows);
    assert_eq!(
        &doc.reactions[..original.reactions.len()],
        original.reactions
    );
    assert_eq!(doc.models.len(), 3);
    assert_eq!(doc.arrows.len(), original.arrows.len() + 2);
    for (model, frame) in doc.models.iter().zip(expected) {
        assert_eq!(model.display, Display::TwoD);
        assert_eq!(
            model.frame_positions(),
            frame
                .positions_angstrom
                .iter()
                .map(|p| p.map(|x| x as f32))
                .collect::<Vec<_>>()
        );
        assert_eq!(model.frame_sources().iter().flatten().count(), 1);
    }
    let reactions = &doc.reactions[original.reactions.len()..];
    assert_eq!(reactions[0].products, reactions[1].reactants);
    assert_ne!(reactions[0].reactants, reactions[0].products);
    for reaction in reactions {
        assert_eq!(
            reshiki::reactions::preview(doc, reaction.arrow).unwrap(),
            *reaction
        );
        assert!(reaction.ready());
    }
    let (lo, _) = reshiki::scene::selection_bounds(doc, &placed.ids).unwrap();
    let (_, hi) = reshiki::scene::selection_bounds(&original, &original.all_ids()).unwrap();
    assert!(lo.y > hi.y);
    assert!(placed.ids.iter().all(|id| !original.all_ids().contains(id)));
    let reopened = Document::from_json(&serde_json::to_vec(doc).unwrap()).unwrap();
    assert_eq!(reopened, *doc);
}

#[test]
fn unresolved_and_nonadjacent_connections_never_become_reaction_arrows() {
    let mut discovered = path(4);
    discovered.gaps.push(DiscoveredGap {
        reactant: 1,
        product: 2,
        reason: "Both IRC branches return to the same minimum".into(),
    });
    // Even an accidentally retained candidate for the same pair cannot fill a gap.
    discovered.segments.push(DiscoveredSegment {
        reactant: 0,
        product: 3,
        ts: result(&discovered.minima[0].frame),
        frames: vec![],
    });
    let placed = build(&Document::default(), &report(discovered, 1)).unwrap();
    assert_eq!(placed.document.models.len(), 4);
    assert_eq!(placed.document.arrows.len(), 2);
    assert_eq!(placed.document.reactions.len(), 2);
    assert!(
        placed
            .document
            .annotations
            .iter()
            .any(|a| a.text == "Unresolved\nconnection")
    );
    let first = &placed.document.reactions[0];
    let second = &placed.document.reactions[1];
    assert_ne!(first.products, second.reactants);
    let mut missing = path(2);
    missing.segments.clear();
    let placed = build(&Document::default(), &report(missing, 1)).unwrap();
    assert!(placed.document.arrows.is_empty() && placed.document.reactions.is_empty());
    let mut reversed = path(2);
    reversed.segments[0].reactant = 1;
    reversed.segments[0].product = 0;
    assert!(
        build(&Document::default(), &report(reversed, 1))
            .unwrap()
            .document
            .arrows
            .is_empty()
    );
}

#[test]
fn unsupported_or_lossy_minima_are_named_without_invented_topology_or_arrows() {
    let original = Document::default();
    for kind in 0..4 {
        let mut discovered = path(3);
        let frame = Arc::make_mut(&mut discovered.minima[1].frame);
        match kind {
            0 => frame.positions_angstrom[1] = frame.positions_angstrom[0],
            1 => frame.positions_angstrom[0][0] = f64::NAN,
            2 => {
                frame.charge = 1;
                frame.multiplicity = 2;
            }
            3 => frame.positions_angstrom.pop().map(|_| ()).unwrap(),
            _ => unreachable!(),
        }
        let placed = build(&original, &report(discovered, 1)).unwrap();
        assert_eq!(placed.document.models.len(), 2, "case {kind}");
        assert!(placed.document.arrows.is_empty());
        assert!(placed.document.reactions.is_empty());
        assert_eq!(placed.warnings.len(), 1);
        assert!(
            placed
                .document
                .annotations
                .iter()
                .any(|a| a.text.contains("2D structure unavailable"))
        );
    }
    let mut invalid = path(2);
    invalid.order.push(100);
    assert!(build(&original, &report(invalid, 1)).is_err());
    assert!(original.atoms.is_empty());
}

#[test]
fn calculated_stereo_and_isotopes_survive_the_editable_2d_layout() {
    use reshiki::chemistry::document::{for_drawing, prepare};
    let host = Document::default();
    let mut frame = Frame {
        numbers: vec![6, 9, 17, 35, 1],
        positions_angstrom: vec![
            [0., 0., 0.],
            [0.8, 0.8, 0.8],
            [-1.02, -1.02, 1.02],
            [-1.1, 1.1, -1.1],
            [0.62, -0.62, -0.62],
        ],
        isotopes: vec![13, 0, 0, 0, 2],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let mut labels = Vec::new();
    for _ in 0..2 {
        let doc = minimum(&host, &frame).unwrap();
        assert_eq!(doc.atoms[0].isotope, 13);
        assert!(doc.atoms.iter().any(|a| a.element == "H" && a.isotope == 2));
        assert!(doc.atoms[0].stereo.is_some());
        assert!(
            doc.bonds
                .iter()
                .any(|b| matches!(b.display.as_str(), "wedge" | "hash"))
        );
        assert_eq!(
            doc.models[0].frame_positions(),
            frame
                .positions_angstrom
                .iter()
                .map(|p| p.map(|x| x as f32))
                .collect::<Vec<_>>()
        );
        let mut visible = doc.clone();
        visible.models.clear();
        for atom in &mut visible.atoms {
            atom.stereo = None;
            atom.cip_label = None;
        }
        let drawn = for_drawing(&prepare(&visible).unwrap(), &visible).unwrap();
        labels.push(drawn.labels().unwrap().atoms[0].clone());
        for p in &mut frame.positions_angstrom {
            p[2] = -p[2];
        }
    }
    assert!(labels.iter().all(Option::is_some));
    assert_ne!(labels[0], labels[1]);
}

#[test]
fn each_minimum_perceives_its_transferred_hydrogens_in_frame_order() {
    let mut frame = Frame {
        numbers: vec![8, 1, 1, 8, 1, 1],
        positions_angstrom: vec![
            [0., 0., 0.],
            [0.97, 0., 0.],
            [-0.24, 0.93, 0.],
            [4., 0., 0.],
            [4.97, 0., 0.],
            [3.76, 0.93, 0.],
        ],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let first = minimum(&Document::default(), &frame).unwrap();
    frame.positions_angstrom.swap(1, 4);
    let second = minimum(&Document::default(), &frame).unwrap();
    for doc in [&first, &second] {
        assert_eq!(doc.atoms.len(), 2);
        assert_eq!(doc.models[0].hydrogens.len(), 4);
        assert!(doc.atoms.iter().all(|a| a.explicit_h == 2));
    }
    let parent_at = |doc: &Document, x: f32| {
        doc.models[0]
            .hydrogens
            .iter()
            .find(|h| h.xyz == [x, 0., 0.])
            .unwrap()
            .parent
    };
    assert_eq!(parent_at(&first, 0.97), first.atoms[0].id);
    assert_eq!(parent_at(&second, 4.97), second.atoms[1].id);
    assert_eq!(
        second.models[0].frame_positions(),
        frame
            .positions_angstrom
            .iter()
            .map(|p| p.map(|x| x as f32))
            .collect::<Vec<_>>()
    );
}

#[test]
fn mapped_ammonium_and_open_shell_methyl_keep_authored_electronic_state() {
    for (element, number, charge, radicals, hydrogen_positions) in [
        (
            "N",
            7,
            1,
            0,
            vec![
                [0.59, 0.59, 0.59],
                [-0.59, -0.59, 0.59],
                [-0.59, 0.59, -0.59],
                [0.59, -0.59, -0.59],
            ],
        ),
        (
            "C",
            6,
            0,
            1,
            vec![[1.08, 0., 0.], [-0.54, 0.935, 0.], [-0.54, -0.935, 0.]],
        ),
    ] {
        let mut frame = Frame {
            numbers: std::iter::once(number)
                .chain(std::iter::repeat_n(1, hydrogen_positions.len()))
                .collect(),
            positions_angstrom: std::iter::once([0., 0., 0.])
                .chain(hydrogen_positions)
                .collect(),
            isotopes: vec![],
            masses_amu: vec![],
            charge,
            multiplicity: 1 + u32::from(radicals),
            drawing_atoms: vec![],
        };
        let (mut host, _) = frames_import::drawing(&frame).unwrap();
        host.atoms[0].charge = charge;
        host.atoms[0].radical_electrons = radicals;
        // Both explicit mapped hydrogens and a known model's implicit H parents
        // establish the full unchanged graph without inventing charge sites.
        for folded in [false, true] {
            if folded {
                frames_import::fold_calculation_hydrogens(&mut host, &frame);
            }
            frame.drawing_atoms = host.models[0].frame_sources();
            let doc = minimum(&host, &frame).unwrap();
            assert_eq!(doc.atoms.len(), 1);
            assert_eq!(doc.atoms[0].element, element);
            assert_eq!(doc.atoms[0].charge, charge);
            assert_eq!(doc.atoms[0].radical_electrons, radicals);
            assert_eq!(doc.atoms[0].explicit_h as usize, frame.numbers.len() - 1);
            let mut changed = frame.clone();
            changed.positions_angstrom[1] = [6., 0., 0.];
            assert!(
                minimum(&host, &changed).is_err(),
                "changed ion/radical graph must not reuse assignments"
            );
        }
    }
}

#[test]
fn imported_physical_atom_order_ignores_auxiliary_attachment_anchors() {
    let file = reshiki::qcread::read(
        "caffeine.xyz",
        include_bytes!("../../../../../../tests/fixtures/xtb/caffeine.xyz"),
    )
    .unwrap();
    let frame = &file.jobs[0].steps[0].frame;
    let (imported, _) = frames_import::drawing(frame).unwrap();
    assert_eq!(
        physical_atoms(&imported, frame).unwrap().len(),
        frame.numbers.len()
    );
    minimum(&Document::default(), frame)
        .unwrap()
        .validate()
        .unwrap();

    let frame = water(0.);
    let (mut imported, _) = frames_import::drawing(&frame).unwrap();
    let physical = physical_atoms(&imported, &frame).unwrap();
    let anchor = reshiki::attachments::add(
        &mut imported,
        &physical[..2],
        reshiki::attachments::Kind::MultiCenter,
    )
    .unwrap();
    assert_eq!(imported.atoms.len(), frame.numbers.len() + 1);
    assert_eq!(physical_atoms(&imported, &frame).unwrap(), physical);
    let mut wrong_order = imported.clone();
    wrong_order.models[0].frame_atom_order = vec![1, 0, 2];
    assert!(physical_atoms(&wrong_order, &frame).is_err());
    let doc = finish_minimum(&Document::default(), &frame, imported).unwrap();
    doc.validate().unwrap();
    assert_eq!(doc.atom(anchor).unwrap().map_num, 0);
    for (index, id) in physical.iter().enumerate() {
        assert_eq!(doc.atom(*id).unwrap().map_num, (index + 1) as u32);
    }
    assert_eq!(doc.models[0].frame_positions().len(), frame.numbers.len());
}

fn app() -> App {
    let (doc, plan) = Plan::example();
    let result = report(path(3), plan.arrows[0]);
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc.clone();
    app.tab.saved = doc;
    app.tab.selected = plan.arrows.clone();
    let mut work = run::State::default();
    work.report = result;
    app.tab.xtb.scheme = Some(State {
        options: vec![StepOptions::default(); plan.steps.len()],
        plan,
        method: XtbMethod::Gfn2Xtb,
        sheet: false,
        original: None,
        reviews: None,
        work,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        checked: Cell::new(None),
        name_requests: vec![],
        refresh_names: false,
        #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
        pending_save: None,
    });
    app
}

#[test]
fn placement_preserves_authored_layout_and_is_one_undoable_edit() {
    let mut app = app();
    let before = app.tab.doc.clone();
    app.xtb_place_discovered_scheme();
    assert!(!app.error, "{}", app.status);
    let placed = app.tab.doc.clone();
    assert_eq!(&placed.atoms[..before.atoms.len()], before.atoms);
    assert_eq!(&placed.bonds[..before.bonds.len()], before.bonds);
    assert_eq!(&placed.arrows[..before.arrows.len()], before.arrows);
    assert_eq!(
        &placed.reactions[..before.reactions.len()],
        before.reactions
    );
    assert!(
        app.tab
            .selected
            .iter()
            .all(|id| !before.all_ids().contains(id))
    );
    assert_eq!(app.tool, crate::canvas::Tool::Select);
    let _ = app.update(crate::app::Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(crate::app::Message::Redo);
    assert_eq!(app.tab.doc, placed);
    // The command explicitly places copies. A repeat adds another complete
    // editable chain and never updates or replaces the previously placed one.
    app.xtb_place_discovered_scheme();
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.models.len(), placed.models.len() + 3);
    assert_eq!(&app.tab.doc.atoms[..placed.atoms.len()], placed.atoms);
    assert_eq!(&app.tab.doc.arrows[..placed.arrows.len()], placed.arrows);
    let _ = app.update(crate::app::Message::Undo);
    assert_eq!(app.tab.doc, placed);
}

#[test]
fn no_discovery_and_busy_or_stale_source_cannot_place_a_scheme() {
    assert!(!available(&Report::default()));
    assert!(build(&Document::default(), &Report::default()).is_err());
    let mut app = app();
    let original = app.tab.doc.clone();
    app.tab.busy = true;
    app.xtb_place_discovered_scheme();
    assert_eq!(app.tab.doc, original);
    app.tab.busy = false;
    app.tab.file_epoch += 1;
    app.xtb_place_discovered_scheme();
    assert_eq!(app.tab.doc, original);
}
