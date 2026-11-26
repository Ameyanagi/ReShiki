//! Conformer search settings, progress and table tests.
use super::*;
use reshiki::qcdata::{
    Frame, Provenance, Settings as QcSettings, Source, Status, Task, Trajectory, XtbSettings,
};

const A: [[f64; 3]; 3] = [[0., 0., 0.], [0.96, 0., 0.], [-0.24, 0.93, 0.]];
const B: [[f64; 3]; 3] = [[0., 0., 0.], [0.97, 0., 0.], [-0.30, 0.90, 0.]];
const C: [[f64; 3]; 3] = [[0., 0., 0.], [0.95, 0.1, 0.], [-0.20, 0.95, 0.1]];

/// A water "search" with three kept conformers, the lowest first.
pub(in crate::app::xtb) fn searched(method: &str) -> CalcResult {
    let frame = Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: A.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task: Task::Conformers,
        endpoints: None,
        settings: QcSettings::Xtb(XtbSettings {
            method: XtbMethod::Gfn2Xtb,
            conventions: Default::default(),
            scf_retries: vec![],
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            gfnff: None,
            threads: None,
            convergence: None,
            fixed_atoms: vec![],
        }),
        provenance: Provenance {
            program: "axtb".into(),
            program_version: Some("0.2.0".into()),
            method: Some(method.into()),
            basis: None,
            source: Source::Calculated {
                engine_revision: "9d0388c".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: Some(-5.070176),
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: vec![],
        hessian_hartree_per_bohr2: vec![],
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: None,
        thermo: None,
        trajectory: Some(Trajectory {
            numbers: vec![8, 1, 1],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: [A, B, C].concat().as_flattened().to_vec(),
            energies_hartree: vec![-5.070176, -5.069176, -5.065176],
            labels: vec!["1".into(), "2".into(), "3".into()],
            minimizer_energies_hartree: vec![0.010, 0.0101, 0.0105],
            cluster_sizes: vec![12, 5, 1],
        }),
        keep_full: false,
        omitted_bytes: None,
        run: None,
    }
}

#[test]
fn settings_check_every_field_and_the_atom_limits() {
    let settings = Settings::default();
    let config = settings.config(Some(14)).unwrap();
    assert_eq!(
        (config.starts, config.window_kcal, config.representatives),
        (100, 15., 20)
    );
    assert_eq!(config.diversity.angstrom(), 0.75);

    let typed = |f: fn(&mut Settings)| {
        let mut settings = Settings::default();
        f(&mut settings);
        settings.config(Some(14))
    };
    assert!(typed(|s| s.starts = "301".into()).is_err());
    assert!(typed(|s| s.starts = "0".into()).is_err());
    assert!(typed(|s| s.starts = " 300 ".into()).is_ok());
    assert!(typed(|s| s.window = "0".into()).is_err());
    assert!(typed(|s| s.window = "abc".into()).is_err());
    assert!(typed(|s| s.representatives = "61".into()).is_err());
    assert!(typed(|s| s.representatives = "60".into()).is_ok());

    assert!(settings.config(Some(MAX_ATOMS)).is_ok());
    let too_many = settings.config(Some(MAX_ATOMS + 1)).unwrap_err();
    assert!(too_many.contains("200 atoms"), "{too_many}");
    let gfn2 = Settings {
        minimizer: Minimizer::Gfn2,
        ..Settings::default()
    };
    assert!(gfn2.config(Some(MAX_GFN2_START_ATOMS)).is_ok());
    assert!(gfn2.config(Some(MAX_GFN2_START_ATOMS + 1)).is_err());
    // Before the 3D model exists the atom count is unknown.
    assert!(gfn2.config(None).is_ok());
}

#[test]
fn the_method_line_names_the_search_and_the_column_its_force_field() {
    let config = Settings::default().config(None).unwrap();
    assert_eq!(
        config.describe(XtbMethod::Gfn2Xtb),
        "MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank"
    );
    let gfnff = Config {
        minimizer: Minimizer::GfnFf,
        diversity: Diversity::Coarse,
        ..config.clone()
    };
    assert_eq!(
        gfnff.describe(XtbMethod::Gfn2Xtb),
        "GFN-FF ETKDG 100 starts · Butina 1.25 Å"
    );

    let mut result = searched(&config.describe(XtbMethod::Gfn2Xtb));
    assert_eq!(start_field(&result), Some("MMFF94s"));
    result.provenance.method = Some(gfnff.describe(XtbMethod::Gfn2Xtb));
    assert_eq!(start_field(&result), None);
    result.provenance.method = Some("GFN2-xTB".into());
    assert_eq!(start_field(&result), None);
}

#[test]
fn progress_reads_as_the_search_goes() {
    let mut live = Live {
        config: Settings::default().config(None).unwrap(),
        method: XtbMethod::Gfn2Xtb,
        phase: Phase::Starts,
        stopping: false,
    };
    assert_eq!(live.line(0.42), "Embedding 100 starts · MMFF94s · 0.4 s");
    assert_eq!(live.fraction(), None);
    live.phase = Phase::Sampled {
        done: 64,
        total: 100,
    };
    assert_eq!(
        live.line(3.),
        "Embedding 100 · minimizing 64/100 (MMFF94s) · 3.0 s"
    );
    assert_eq!(live.fraction(), Some(0.32));
    live.phase = Phase::Reranking { done: 7, total: 20 };
    assert_eq!(live.line(9.), "Clustered · GFN2-xTB 7/20 · 9.0 s");
    assert_eq!(live.fraction(), Some(0.675));
    live.stopping = true;
    assert_eq!(live.line(9.), "Stopping · 7 of 20 finished · 9.0 s");
}

#[test]
fn gfn2_estimates_grow_with_size_and_shrink_with_threads() {
    let caffeine = gfn2_estimate(24, 100, 1);
    assert!((caffeine - 80.).abs() < 1e-9, "{caffeine}");
    assert!((gfn2_estimate(24, 100, 8) - 10.).abs() < 1e-9);
    assert!(gfn2_estimate(113, 100, 8) > 10. * caffeine / 8.);
}

#[test]
fn rows_rank_by_xtb_with_start_energies_populations_and_clusters() {
    let result = searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
    assert!(
        result
            .validate(&Frame {
                numbers: vec![8, 1, 1],
                positions_angstrom: A.to_vec(),
                isotopes: vec![],
                masses_amu: vec![],
                charge: 0,
                multiplicity: 1,
                drawing_atoms: vec![],
            })
            .is_ok()
    );
    let doc = reshiki::document::Document::default();
    let rows = rows(&doc, &[None, None, None], &result, true);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].delta_kcal, 0.);
    assert!((rows[1].delta_kcal - 0.001 * KCAL_PER_HARTREE).abs() < 1e-6);
    assert!((rows[1].start_kcal.unwrap() - 0.0001 * KCAL_PER_HARTREE).abs() < 1e-6);
    let total: f64 = rows.iter().map(|r| r.percent).sum();
    assert!((total - 100.).abs() < 1e-9);
    assert!(rows[0].percent > rows[1].percent && rows[1].percent > rows[2].percent);
    assert_eq!(
        rows.iter().map(|r| r.cluster).collect::<Vec<_>>(),
        [Some(12), Some(5), Some(1)]
    );
    // A stale result skips the stereo check.
    assert!(rows.iter().all(|r| r.stereo.is_none()));
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Water has one heavy atom: every heavy-atom RMSD is zero.
        assert!(rows.iter().all(|r| r.rmsd.is_some_and(|x| x.abs() < 1e-12)));
    }
}

#[test]
fn saved_fixture_keeps_the_existing_table_display() {
    let result = searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
    // The saved compact coordinates follow the same path as an existing file.
    let result: CalcResult =
        serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
    let rows = rows(
        &reshiki::document::Document::default(),
        &[None; 3],
        &result,
        true,
    );
    let display: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                format!("{:.2}", row.delta_kcal),
                format!("{:.2}", row.start_kcal.unwrap()),
                format!("{:.1} %", row.percent),
                row.cluster.unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        display,
        [
            ("0.00".into(), "0.00".into(), "74.0 %".into(), "12".into()),
            ("0.63".into(), "0.06".into(), "25.7 %".into(), "5".into()),
            ("3.14".into(), "0.31".into(), "0.4 %".into(), "1".into()),
        ]
    );
    #[cfg(not(target_arch = "wasm32"))]
    assert_eq!(
        rows.iter()
            .map(|row| format!("{:.2} Å", row.rmsd.unwrap()))
            .collect::<Vec<_>>(),
        ["0.00 Å", "0.00 Å", "0.00 Å"]
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod app {
    use super::{A, B, C};
    use crate::app::xtb::conformers::{Action, Event, Found, Phase};
    use crate::app::xtb::palette::Action as PaletteAction;
    use crate::app::xtb::run::{self, Report};
    use crate::app::xtb::tests::{f32s, model, send, water};
    use crate::app::xtb::{Action as XtbAction, CalcTask, Outcome};
    use crate::app::{App, Message};
    use reshiki::qcdata;
    use reshiki_optimize::conformers::Ranked;
    use std::sync::Arc;

    fn search(app: &mut App, event: Event) {
        let serial = app.tab.xtb.run.as_ref().expect("a search in flight").serial;
        let _ = app.update(Message::Xtb(XtbAction::Search(serial, event)));
    }

    fn ranked(positions: [[f64; 3]; 3], energy: f64, cluster_size: u32) -> Ranked {
        Ranked {
            positions: positions.to_vec(),
            energy_hartree: energy,
            relative_kcal: (energy + 5.070176) * 627.509_474_063,
            start_energy_hartree: 0.01 + (energy + 5.070176),
            cluster_size,
            boltzmann: 0.,
            rmsd_to_best: 0.,
        }
    }

    fn found(stopped: bool) -> Event {
        let mut computed = run::example_computed(-5.070176, 14.01);
        computed.gradient = Some(vec![[0.; 3]; 3]);
        Event::Done(Ok(Found {
            engine: run::example_engine(),
            best: Arc::new(Report {
                engine: run::example_engine(),
                computed,
                energy: -5.070176,
                converged: true,
                positions: A.to_vec(),
                steps: 4,
                energies: vec![-5.06, -5.070176],
                path: vec![],
                evaluations: vec![],
            }),
            ranked: vec![
                ranked(A, -5.070176, 12),
                ranked(B, -5.069176, 5),
                ranked(C, -5.065176, 1),
            ],
            starts: 100,
            clusters: 14,
            reranked: if stopped { 3 } else { 20 },
            planned: 20,
            stopped,
            notes: vec![],
            seconds: 12.5,
        }))
    }

    #[test]
    fn a_search_shows_progress_then_ranks_previews_overlays_and_uses_conformers() {
        let (mut app, _) = water(CalcTask::Conformers);
        assert!(
            app.status
                .starts_with("xTB · conformers · H2O · Embedding 100 starts"),
            "{}",
            app.status
        );
        search(
            &mut app,
            Event::Phase(Phase::Sampled {
                done: 32,
                total: 100,
            }),
        );
        assert!(
            app.status
                .contains("Embedding 100 · minimizing 32/100 (MMFF94s)"),
            "{}",
            app.status
        );

        search(&mut app, found(false));
        assert!(!app.xtb_running());
        let stored = &app.tab.doc.science.results[0];
        assert_eq!(stored.result.task, qcdata::Task::Conformers);
        assert_eq!(stored.result.trajectory.as_ref().unwrap().steps(), 3);
        // The run's settings and time are saved, as for an optimization.
        assert_eq!(
            stored.result.run.as_ref().map(|run| run.seconds),
            Some(12.5)
        );
        assert!(matches!(
            &stored.result.settings,
            qcdata::Settings::Xtb(s) if s.threads.is_some() && s.convergence.is_some()
        ));
        assert_eq!(
            stored.result.provenance.method.as_deref(),
            Some("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank")
        );
        // The lowest conformer is applied to the 3D model as one Undo step.
        assert_eq!(model(&app.tab.doc), f32s(&A));
        assert!(
            matches!(
                &app.tab.xtb.outcome,
                Some(Outcome::Done { line, .. }) if line.starts_with("3 conformers kept · 100 starts · 14 clusters · lowest E −5.070176 Eh")
            ),
            "{:?}",
            app.status
        );
        assert_eq!(
            app.tab.xtb.palette.view,
            crate::app::xtb::results::View::Conformers
        );

        let shown = app.xtb_shown().unwrap();
        let rows = app.xtb_conformer_rows(&shown);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].stereo, Some(true));

        send(&mut app, XtbAction::Palette(PaletteAction::Step(1)));
        assert_eq!(model(app.display_document()), f32s(&B));
        assert!(app.status.starts_with("Conformer 2 of 3"), "{}", app.status);

        send(&mut app, XtbAction::Conformers(Action::Overlay(true)));
        assert_eq!(app.display_document().models[0].overlay.len(), 2);
        assert!(
            app.status.contains("overlay: all conformers"),
            "{}",
            app.status
        );

        send(&mut app, XtbAction::Palette(PaletteAction::UseStep));
        assert_eq!(model(&app.tab.doc), f32s(&B));
        assert!(
            app.status.starts_with("Applied conformer 2"),
            "{}",
            app.status
        );
        let _ = app.update(Message::Undo);
        assert_eq!(model(&app.tab.doc), f32s(&A));
    }

    #[test]
    fn cancel_while_reranking_keeps_the_finished_conformers() {
        let (mut app, _) = water(CalcTask::Conformers);
        // The search thread decides what is kept: with nothing finished it
        // answers that it was cancelled.
        search(&mut app, Event::Phase(Phase::Clustering));
        send(&mut app, XtbAction::Cancel);
        assert!(app.xtb_running());
        assert!(app.status.contains("Stopping"), "{}", app.status);
        search(
            &mut app,
            Event::Done(Err(crate::app::xtb::state::Failure::Other(
                "Calculation canceled".into(),
            ))),
        );
        assert!(!app.xtb_running());
        assert_eq!(app.status, "Calculation canceled");
        assert!(app.tab.xtb.outcome.is_none());

        let (mut app, _) = water(CalcTask::Conformers);
        search(
            &mut app,
            Event::Phase(Phase::Reranking { done: 3, total: 20 }),
        );
        send(&mut app, XtbAction::Cancel);
        assert!(
            app.xtb_running(),
            "the finished conformers are still to come"
        );
        assert!(
            app.status.contains("Stopping · 3 of 20 finished"),
            "{}",
            app.status
        );
        search(&mut app, found(true));
        let result = &app.tab.doc.science.results[0].result;
        assert!(matches!(
            &result.status,
            qcdata::Status::NotConverged { reason } if reason == "Canceled after 3 of 20 representatives"
        ));
        assert!(matches!(
            &app.tab.xtb.outcome,
            Some(Outcome::Done { notes, .. }) if notes.iter().any(|n| n == "Canceled. 3 re-ranked conformers are kept and usable.")
        ));
    }

    #[test]
    fn the_card_rejects_bad_fields_before_running() {
        let (mut app, _) = water(CalcTask::Energy);
        app.tab.xtb.run = None;
        app.xtb.settings.task = CalcTask::Conformers;
        send(&mut app, XtbAction::Conformers(Action::Starts("0".into())));
        send(&mut app, XtbAction::Run);
        assert!(!app.xtb_running());
        assert!(app.status.contains("Starting structures"), "{}", app.status);
    }

    #[test]
    fn the_saved_search_is_compact() {
        let (mut app, _) = water(CalcTask::Conformers);
        search(&mut app, found(false));
        let bytes = app.tab.doc.science.saved_sizes()[0];
        // Saved positions are one compact string, not a list of numbers.
        let saved = serde_json::to_value(&app.tab.doc.science.results[0]).unwrap();
        assert!(
            saved["result"]["trajectory"]["positions_angstrom"].is_string(),
            "{saved}"
        );
        // Three water conformers: a few kilobytes at most.
        assert!(bytes < 4_000, "{bytes}");
        let xyz =
            super::super::export::xyz(&app.tab.doc.science.results[0].result, "water").unwrap();
        assert_eq!(xyz.lines().filter(|l| l.trim() == "3").count(), 3);
    }
}

#[test]
fn an_xtb_minimizer_is_the_method_the_card_checks_and_runs() {
    use crate::app::xtb::CalcTask;
    let card = crate::app::xtb::state::Settings {
        task: CalcTask::Conformers,
        method: XtbMethod::GfnFf,
        unpaired: "1".into(),
        ..Default::default()
    };
    // GFN-FF itself is closed-shell only.
    assert!(card.config((0, 0), true, vec![]).is_err());
    let gfn2 = Settings {
        minimizer: Minimizer::Gfn2,
        ..Settings::default()
    };
    let effective = effective(&card, &gfn2);
    assert_eq!(effective.method, XtbMethod::Gfn2Xtb);
    assert!(effective.config((0, 0), true, vec![]).is_ok());
    // A force-field minimizer re-ranks with the card's method.
    assert_eq!(
        super::effective(&card, &Settings::default()).method,
        XtbMethod::GfnFf
    );
    // Without re-ranking, the representatives field does not apply.
    let ignored = Settings {
        representatives: "0".into(),
        ..gfn2
    };
    assert_eq!(
        ignored.config(None).unwrap().representatives,
        MAX_REPRESENTATIVES
    );
}

#[test]
fn recalculation_follows_the_card_or_refuses() {
    use crate::app::xtb::CalcTask;
    let card = crate::app::xtb::state::Settings {
        task: CalcTask::Conformers,
        solvent: Some("water"),
        ..Default::default()
    };
    let saved = card.config((0, 0), true, vec![]).unwrap();
    let mut config = saved.clone();
    let gfnff = Settings {
        minimizer: Minimizer::GfnFf,
        ..Settings::default()
    };
    let search = recalculation(&mut config, &gfnff).unwrap();
    assert_eq!(search.minimizer, Minimizer::GfnFf);
    assert_eq!((config.method, config.solvent), (XtbMethod::GfnFf, None));
    // Wrong fields never fall back to another task.
    let broken = Settings {
        starts: "0".into(),
        ..Settings::default()
    };
    assert!(recalculation(&mut saved.clone(), &broken).is_err());
    let mut open_shell = crate::app::xtb::state::Config {
        unpaired: 1,
        ..saved
    };
    assert!(recalculation(&mut open_shell, &gfnff).is_err());
}
