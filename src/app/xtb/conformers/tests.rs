//! Conformer search settings, progress and table tests.
use super::*;
use reshiki::qcdata::{
    Frame, Provenance, Settings as QcSettings, Source, Status, Task, Trajectory, XtbSettings,
};

// The lowest conformer differs from the water model the search started from.
const A: [[f64; 3]; 3] = [[0., 0., 0.], [0.95, 0., 0.], [-0.24, 0.92, 0.]];
const B: [[f64; 3]; 3] = [[0., 0., 0.], [0.97, 0., 0.], [-0.30, 0.90, 0.]];
const C: [[f64; 3]; 3] = [[0., 0., 0.], [0.95, 0.1, 0.], [-0.20, 0.95, 0.1]];

#[test]
fn conformer_headers_show_units_and_fit_the_inspector_subset() {
    let mut result = searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
    let labels = |r: &CalcResult, inspector| {
        view::headers(r, inspector)
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(&result, false),
        [
            "#",
            "ΔE xTB (kcal/mol)",
            "ΔE MMFF94s (kcal/mol)",
            "Pop. 298 K",
            "RMSD to #1 (Å)",
            "Starts",
            "Stereo",
        ]
    );
    assert_eq!(
        labels(&result, true),
        ["#", "ΔE (kcal/mol)", "Pop.", "Stereo"]
    );
    assert_eq!(
        view::headers(&result, true).last().unwrap().1,
        iced::Length::Fill
    );
    result.settings = QcSettings::Imported { keywords: vec![] };
    result.provenance.method = None;
    assert_eq!(
        labels(&result, false),
        [
            "#",
            "ΔE (kcal/mol)",
            "Pop. 298 K",
            "RMSD to #1 (Å)",
            "Starts",
            "Stereo"
        ]
    );
}

#[test]
fn funnel_describes_the_stored_minimizer_and_skips_absent_data() {
    let mut result = searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
    assert_eq!(view::search_funnel(&result), None);
    result.run = Some(qcdata::RunSummary {
        seconds: 12.5,
        scf_iterations: vec![],
        finished: None,
        search: Some(qcdata::SearchSummary {
            requested: 100,
            made: 100,
            minimized: 98,
            used_unconverged_starts: false,
            distinct: 33,
            clusters: 4,
            rerank_planned: 4,
            reranked: 4,
            kept: 3,
        }),
        transition_state: None,
    });
    assert_eq!(
        view::search_funnel(&result).unwrap(),
        "100 starts · 98 minimized with MMFF94s · 33 distinct · 4 clusters · GFN2-xTB re-ranked 4 · 3 kept",
    );
    result.provenance.method = Some("GFN2-xTB ETKDG 100 starts · Butina 0.75 Å".into());
    let counts = result.run.as_mut().unwrap().search.as_mut().unwrap();
    counts.made = 99;
    counts.reranked = 0;
    counts.rerank_planned = 0;
    assert_eq!(
        view::search_funnel(&result).unwrap(),
        "99 of 100 starts made · 98 minimized with GFN2-xTB · 33 distinct · 4 clusters · 3 kept",
    );
    result.settings = QcSettings::Imported { keywords: vec![] };
    assert_eq!(view::search_funnel(&result), None);
}

#[test]
fn stereo_cells_show_all_descriptors_and_distinguish_unassigned_cip() {
    let mut row = Row {
        step: 0,
        delta_kcal: 0.,
        start_kcal: None,
        percent: 100.,
        rmsd: None,
        cluster: None,
        stereo: None,
        tetrahedral: None,
        mirror_step: None,
    };
    assert_eq!(view::stereo_text(&row).0, "—");
    row.tetrahedral = Some(TetrahedralStereo {
        has_defined: false,
        undefined: ["R", "R", "S", "R"]
            .into_iter()
            .enumerate()
            .map(|(atom, cip)| UndefinedCentre {
                atom: atom as u64,
                cip: Some(cip.into()),
                winding: Some(1),
            })
            .collect(),
    });
    row.mirror_step = Some(1);
    row.stereo = Some(false);
    assert_eq!(view::stereo_text(&row).0, "R,R,S,R/S,S,R,S · changed");
    row.tetrahedral.as_mut().unwrap().undefined[0].cip = None;
    assert_eq!(view::stereo_text(&row).0, "?,R,S,R/?,S,R,S · changed");
}

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
            rmsd_to_best_angstrom: vec![],
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
    assert!(!config.invert_undefined_stereo);

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
    assert!(
        typed(|s| s.invert_undefined_stereo = true)
            .unwrap()
            .invert_undefined_stereo
    );

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

#[test]
fn table_uses_the_first_saved_energy_even_when_it_is_higher() {
    let mut result = searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
    let path = result.trajectory.as_mut().unwrap();
    path.positions_angstrom.truncate(2 * path.numbers.len() * 3);
    path.energies_hartree = vec![-5.068, -5.070];
    path.labels.truncate(2);
    path.minimizer_energies_hartree.truncate(2);
    path.cluster_sizes.truncate(2);

    let (_, exported) = reshiki_xtb_jobs::conformers::ranked_trajectory(Some(path)).unwrap();
    assert_eq!(exported[0].step, 1);
    assert_eq!(exported[0].statistics.delta_kcal, 0.0);
    assert_eq!(format!("{:.2}", exported[1].statistics.delta_kcal), "1.26");

    let doc = reshiki::document::Document::default();
    let rows = rows(&doc, &[None; 3], &result, true);
    let table_deltas: Vec<_> = rows
        .iter()
        .map(|row| format!("{:.2}", row.delta_kcal))
        .collect();
    assert_eq!(table_deltas, ["0.00", "-1.26"]);
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
    use reshiki::{
        document::{Document, Point},
        models3d::{Model3d, Placed},
    };
    use reshiki_optimize::conformers::Ranked;
    use std::sync::Arc;

    /// Two conformations of 2-butanol, each followed by its translated mirror.
    /// An optional wedged OH creates a second, defined centre at C2.
    fn chiral_ensemble(
        defined_at_c2: bool,
    ) -> (Document, Vec<u64>, qcdata::Frame, qcdata::CalcResult) {
        let mut doc = Document::default();
        let a = [
            [-2., -1., 1.],
            [-1., -1., 1.],
            [1., 1., 1.],
            [0., 0., 0.],
            [1., -1., -1.],
        ];
        let mut a = a.to_vec();
        let mut elements = vec!["C", "C", "O", "C", "C"];
        if defined_at_c2 {
            a.push([-1., -2., 2.]);
            elements.push("O");
        }
        let ids: Vec<_> = elements
            .iter()
            .zip(&a)
            .map(|(&element, &[x, y, _])| {
                doc.add_atom(element, Point::new(x as f32 * 26., -y as f32 * 26.))
            })
            .collect();
        for (i, j) in [(0, 1), (1, 3), (3, 2), (3, 4)] {
            doc.add_bond(ids[i], ids[j], 1, "plain");
        }
        if defined_at_c2 {
            doc.add_bond(ids[1], ids[5], 1, "wedge");
        }
        let frame = qcdata::Frame {
            numbers: elements
                .iter()
                .map(|&e| if e == "C" { 6 } else { 8 })
                .collect(),
            positions_angstrom: a.clone(),
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        };
        let mirror = |xyz: &[[f64; 3]]| {
            xyz.iter()
                .map(|&[x, y, z]| [2. - x, y + 1., z - 1.])
                .collect::<Vec<_>>()
        };
        let mut b = a.clone();
        b[0] = [-1., -1., 2.];
        let mut result =
            super::searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
        result.frame = frame.hash();
        result.trajectory = Some(qcdata::Trajectory {
            numbers: frame.numbers.clone(),
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: [a.clone(), mirror(&a), b.clone(), mirror(&b)]
                .concat()
                .as_flattened()
                .to_vec(),
            energies_hartree: vec![-5.070176, -5.070176, -5.069176, -5.069176],
            labels: vec![],
            minimizer_energies_hartree: vec![],
            cluster_sizes: vec![],
            rmsd_to_best_angstrom: vec![],
        });
        let placed = ids
            .iter()
            .zip(&a)
            .map(|(&id, &xyz)| Placed {
                id,
                xyz: xyz.map(|p| p as f32),
            })
            .collect();
        doc.models
            .push(Model3d::new(&doc, placed, vec![], Default::default()));
        (doc, ids, frame, result)
    }

    #[test]
    fn mirror_pairs_keep_their_steps_for_preview_overlay_and_use() {
        use super::super::view::stereo_text;
        let (doc, ids, frame, mut result) = chiral_ensemble(false);
        let mut app = App::new().0;
        app.tab.doc = doc;
        let launch = reshiki::science::launch(&app.tab.doc, ids[0], 0, 1).unwrap();
        result.input_frame = Some(launch.frame.hash());
        let trajectory = result.trajectory.as_ref().unwrap();
        let second = trajectory.frame(2).unwrap();
        let mirror = trajectory.frame(1).unwrap();
        reshiki::science::attach(&mut app.tab.doc, &launch, vec![frame], result).unwrap();
        app.xtb_show_result(&app.tab.doc.science.results[0].result.clone());

        let shown = app.xtb_shown().unwrap();
        let table = app.xtb_conformer_table(&shown);
        assert_eq!(
            table
                .rows
                .iter()
                .map(|r| (r.step, r.mirror_step))
                .collect::<Vec<_>>(),
            [(0, Some(1)), (2, Some(3))]
        );
        assert!((table.rows.iter().map(|r| r.percent).sum::<f64>() - 100.).abs() < 1e-10);
        assert!(table.rows[0].percent > 50.);
        assert_eq!(stereo_text(&table.rows[0]).0, "S/R");
        assert_eq!(table.rank(2), Some(1));
        assert_eq!(table.rank(3), Some(1));
        assert_eq!(
            table.stereo_note.as_deref(),
            Some("C4 has no wedge. The Stereo column lists the configurations.")
        );
        assert_eq!(shown.stored.result.trajectory.as_ref().unwrap().steps(), 4);
        assert_eq!(
            super::super::export::xyz(&shown.stored.result, "2-butanol")
                .unwrap()
                .lines()
                .filter(|l| *l == "5")
                .count(),
            4
        );
        drop(table);

        send(&mut app, XtbAction::Palette(PaletteAction::Step(2)));
        assert_eq!(
            model(app.display_document()),
            f32s(&second.positions_angstrom)
        );
        assert!(app.status.starts_with("Conformer 2 of 2"), "{}", app.status);
        send(&mut app, XtbAction::Conformers(Action::Overlay(true)));
        assert_eq!(app.display_document().models[0].overlay.len(), 1);
        send(&mut app, XtbAction::Palette(PaletteAction::UseStep));
        assert_eq!(model(&app.tab.doc), f32s(&second.positions_angstrom));
        assert_eq!(app.xtb_model_step(&app.xtb_shown().unwrap()), Some(2));
        assert!(
            app.status.starts_with("Applied conformer 2"),
            "{}",
            app.status
        );

        // An imported model may already show the hidden half of a pair.
        reshiki::science::move_model(&mut app.tab.doc, &launch.atoms, &mirror).unwrap();
        send(&mut app, XtbAction::Conformers(Action::Overlay(true)));
        assert_eq!(app.xtb_step(), Some(1));
        assert_eq!(
            model(app.display_document()),
            f32s(&mirror.positions_angstrom)
        );
        assert_eq!(app.display_document().models[0].overlay.len(), 1);
    }

    #[test]
    fn stereo_with_a_defined_centre_prevents_merging_and_displays_changes() {
        let (doc, ids, _, result) = chiral_ensemble(true);
        let atoms: Vec<_> = ids.into_iter().map(Some).collect();
        let rows = super::super::rows(&doc, &atoms, &result, false);
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter()
                .all(|r| r.tetrahedral.as_ref().unwrap().has_defined)
        );
        let cells: Vec<_> = rows
            .iter()
            .map(|r| super::super::view::stereo_text(r).0)
            .collect();
        assert!(
            cells
                .iter()
                .all(|cell| matches!(cell.as_str(), "R" | "S" | "R · changed" | "S · changed")),
            "{cells:?}"
        );
        assert!(
            cells.iter().any(|cell| cell.ends_with(" · changed")),
            "{cells:?}"
        );
    }

    #[test]
    fn opposite_configurations_need_both_energy_and_geometry_agreement_to_merge() {
        let (doc, ids, _, mut result) = chiral_ensemble(false);
        let path = result.trajectory.as_mut().unwrap();
        // First pair is too far apart in energy. The other has matching
        // energies but its terminal carbon has moved in the mirror frame.
        path.energies_hartree[1] += 0.04 / super::KCAL_PER_HARTREE;
        path.positions_angstrom[3 * path.numbers.len() * 3] += 2.;
        let atoms: Vec<_> = ids.into_iter().map(Some).collect();
        assert_eq!(super::super::rows(&doc, &atoms, &result, false).len(), 4);
    }

    #[test]
    fn mirror_pair_uses_the_smallest_reflected_rmsd() {
        let (doc, ids, _, mut result) = chiral_ensemble(false);
        let atoms: Vec<_> = ids.into_iter().map(Some).collect();
        let path = result.trajectory.as_mut().unwrap();
        let frame_len = path.numbers.len() * 3;
        let exact_mirror = path.positions_angstrom[frame_len..2 * frame_len].to_vec();
        path.positions_angstrom.truncate(2 * frame_len);
        path.energies_hartree.truncate(2);
        path.positions_angstrom[frame_len] += 0.15;

        let rows = super::super::rows(&doc, &atoms, &result, false);
        assert_eq!(rows[0].mirror_step, Some(1));

        let path = result.trajectory.as_mut().unwrap();
        path.positions_angstrom.extend(exact_mirror);
        path.energies_hartree
            .push(path.energies_hartree[0] + 0.004 / super::KCAL_PER_HARTREE);
        let rows = super::super::rows(&doc, &atoms, &result, false);
        assert_eq!(
            rows.iter()
                .map(|r| (r.step, r.mirror_step))
                .collect::<Vec<_>>(),
            [(0, Some(2)), (1, None)]
        );
    }

    #[test]
    fn unwedged_card_shows_the_model_and_mirror_choices() {
        let (doc, ids, _, _) = chiral_ensemble(false);
        let mut app = App::new().0;
        app.tab.doc = doc;
        let (info, labels) = app.xtb_conformer_stereo(&ids, true).unwrap();
        assert_eq!(info, "C4 has no wedge. The search keeps one configuration:");
        assert!(
            labels == ["As in the 3D model (R)", "Mirror image (S)"]
                || labels == ["As in the 3D model (S)", "Mirror image (R)"],
            "{labels:?}"
        );
        let (info, labels) = app.xtb_conformer_stereo(&ids, false).unwrap();
        assert_eq!(
            info,
            "C4 has no wedge. The search keeps one configuration (the 3D model is made first):"
        );
        assert_eq!(labels, ["As in the 3D model", "Mirror image"]);
        let (doc, ids, _, _) = chiral_ensemble(true);
        app.tab.doc = doc;
        app.tab.revision += 1;
        let (_, labels) = app.xtb_conformer_stereo(&ids, true).unwrap();
        assert_eq!(labels, ["As in the 3D model", "Inverted at C4"]);
    }

    #[test]
    fn stereo_note_names_an_unreadable_centre_in_the_starting_model() {
        let (mut doc, ids, frame, mut result) = chiral_ensemble(false);
        for atom in &mut doc.models[0].atoms {
            atom.xyz[2] = 0.;
        }
        let launch = reshiki::science::launch(&doc, ids[0], 0, 1).unwrap();
        result.input_frame = Some(launch.frame.hash());
        reshiki::science::attach(&mut doc, &launch, vec![frame], result).unwrap();
        let mut app = App::new().0;
        app.tab.doc = doc;
        app.xtb_show_result(&app.tab.doc.science.results[0].result.clone());
        let shown = app.xtb_shown().unwrap();
        let note =
            "C4 has no wedge and its 3D configuration could not be read. Both forms may appear.";
        assert_eq!(app.xtb_conformer_stereo(&ids, true).unwrap().0, note);
        assert_eq!(
            app.xtb_conformer_table(&shown).stereo_note.as_deref(),
            Some(note)
        );
    }

    #[test]
    fn stereo_note_compares_to_the_starting_model_after_applying_a_mirror() {
        let (mut doc, ids, _, mut result) = chiral_ensemble(false);
        let launch = reshiki::science::launch(&doc, ids[0], 0, 1).unwrap();
        let path = result.trajectory.as_mut().unwrap();
        let mirror = path.frame(1).unwrap();
        path.positions_angstrom = mirror.positions_angstrom.as_flattened().to_vec();
        path.energies_hartree.truncate(1);
        result.frame = mirror.hash();
        result.input_frame = Some(launch.frame.hash());
        let attached = reshiki::science::attach(&mut doc, &launch, vec![mirror], result).unwrap();
        let result = doc.science.results[0].result.clone();
        reshiki::science::apply_geometry(&mut doc, attached.attachment, &result.frame).unwrap();
        let mut app = App::new().0;
        app.tab.doc = doc;
        app.xtb_show_result(&result);
        let shown = app.xtb_shown().unwrap();
        assert!(
            app.xtb_conformer_table(&shown)
                .stereo_note
                .as_ref()
                .unwrap()
                .contains("the mirror image of the starting 3D model")
        );
    }

    #[test]
    fn card_stereo_reuses_labels_until_the_revision_file_or_target_changes() {
        let (doc, ids, _, _) = chiral_ensemble(true);
        let mut app = App::new().0;
        app.tab.doc = doc;
        let wedged = app.xtb_conformer_stereo(&ids, false);
        assert_eq!(wedged.as_ref().unwrap().1[1], "Inverted at C4");

        // Simulate repeated views before the tab records the edit.
        app.tab.doc.bonds.last_mut().unwrap().display = "plain".into();
        assert_eq!(app.xtb_conformer_stereo(&ids, false), wedged);
        app.tab.revision += 1;
        let unwedged = app.xtb_conformer_stereo(&ids, false);
        assert_eq!(unwedged.as_ref().unwrap().1[1], "Mirror image");

        app.tab.doc.bonds.last_mut().unwrap().display = "wedge".into();
        app.tab.file_epoch += 1;
        assert_eq!(app.xtb_conformer_stereo(&ids, false), wedged);
        let other = app.tab.doc.add_atom("O", Point::new(100., 0.));
        assert_eq!(app.xtb_conformer_stereo(&[other], false), None);
    }

    #[tokio::test]
    async fn named_ibuprofen_card_and_tab_use_the_generated_models_cip() {
        use reshiki::engine::{LocalEngine, Request};
        use reshiki::geometry::{Client, ForceField, Limits, Prepared};
        let Some(executable) = std::env::var_os("RESHIKI_NAMING_HELPER") else {
            return;
        };
        let record = reshiki::naming::resolve_name("ibuprofen", Default::default())
            .await
            .unwrap();
        let request = record.prepared.map_or_else(
            || Request::import("naming_smiles", &record.smiles),
            Request::import_prepared,
        );
        let mut doc = LocalEngine::in_process()
            .request(request)
            .await
            .unwrap()
            .document
            .unwrap();
        let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
        let prepared = Prepared::new(&doc, &ids).unwrap().without_retained_depth();
        let model = Client::new(executable.into(), Limits::default())
            .unwrap()
            .generate(&prepared, ForceField::Mmff94s)
            .await
            .unwrap();
        doc.models.push(prepared.model(&model).unwrap());
        let mut app = App::new().0;
        app.tab.doc = doc;
        let launch = reshiki::science::launch(&app.tab.doc, ids[0], 0, 1).unwrap();
        let (card, choices) = app.xtb_conformer_stereo(&ids, true).unwrap();
        let (without_model, _) = app.xtb_conformer_stereo(&ids, false).unwrap();
        let mut result =
            super::searched("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank");
        let path = result.trajectory.as_mut().unwrap();
        path.numbers.clone_from(&launch.frame.numbers);
        path.positions_angstrom = launch.frame.positions_angstrom.as_flattened().to_vec();
        path.energies_hartree.truncate(1);
        let rows = super::super::rows(&app.tab.doc, &launch.atoms, &result, false);
        let stereo = rows[0].tetrahedral.as_ref().unwrap();
        let note = super::super::view::stereo_note(&rows, Some(stereo), |id| {
            app.xtb_atom_name(&launch.atoms, id).0
        })
        .unwrap();
        let (cell, _) = super::super::view::stereo_text(&rows[0]);
        assert!(matches!(cell.as_str(), "R" | "S"));
        assert_eq!(choices[0], format!("As in the 3D model ({cell})"));
        assert!(note.contains(&format!(
            "searched the ({cell}) form, as in the starting 3D model."
        )));
        println!(
            "Ibuprofen with model: {card} | {} | {}\nIbuprofen before model: {without_model}\nIbuprofen tab: {note} | Stereo {cell}",
            choices[0], choices[1]
        );
    }

    fn search(app: &mut App, event: Event) {
        let serial = app.tab.xtb.run.as_ref().expect("a search in flight").serial;
        let _ = app.update(Message::Xtb(XtbAction::Search(serial, event)));
    }

    #[test]
    fn mirror_image_choice_is_consumed_by_a_run_and_discarded_on_retarget() {
        let (mut app, oxygen) = water(CalcTask::Conformers);
        search(&mut app, found(false));
        let invert = XtbAction::Conformers(Action::InvertUndefinedStereo(true));
        send(&mut app, invert.clone());
        assert!(app.xtb.conformers.invert_undefined_stereo);
        send(&mut app, XtbAction::Run);
        let configured = |app: &App| {
            app.tab
                .xtb
                .run
                .as_ref()
                .unwrap()
                .search
                .as_ref()
                .unwrap()
                .config
                .invert_undefined_stereo
        };
        assert!(configured(&app));
        assert!(!app.xtb.conformers.invert_undefined_stereo);
        search(&mut app, found(false));

        send(&mut app, XtbAction::Run);
        assert!(!configured(&app));
        search(&mut app, found(false));

        let other = app.tab.doc.add_atom("O", Point::new(100., 0.));
        send(&mut app, invert);
        assert!(app.xtb.conformers.invert_undefined_stereo);
        let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![other])));
        assert_eq!(app.xtb_target(), Some(vec![other]));
        assert!(!app.xtb.conformers.invert_undefined_stereo);
        let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![oxygen])));
        send(&mut app, XtbAction::Run);
        assert!(!configured(&app));
    }

    fn ranked(positions: [[f64; 3]; 3], energy: f64, cluster_size: u32, rmsd: f64) -> Ranked {
        Ranked {
            positions: positions.to_vec(),
            energy_hartree: energy,
            relative_kcal: (energy + 5.070176) * 627.509_474_063,
            start_energy_hartree: 0.01 + (energy + 5.070176),
            cluster_size,
            boltzmann: 0.,
            rmsd_to_best: rmsd,
        }
    }

    fn found(stopped: bool) -> Event {
        let mut computed = run::example_computed(-5.070176, 14.01);
        computed.gradient = Some(vec![[0.; 3]; 3]);
        Event::Done(Ok(Box::new(Found {
            engine: run::example_engine(),
            minimizer: crate::app::xtb::conformers::Minimizer::Mmff94s,
            optimizers: vec![reshiki_optimize::Method::Internal],
            best: Arc::new(Report {
                engine: run::example_engine(),
                computed,
                energy: -5.070176,
                converged: true,
                positions: A.to_vec(),
                steps: 4,
                optimizers: vec![reshiki_optimize::Method::Internal],
                energies: vec![-5.06, -5.070176],
                path: vec![],
                evaluations: vec![],
            }),
            ranked: vec![
                ranked(A, -5.070176, 12, 0.),
                ranked(B, -5.069176, 5, 0.35),
                ranked(C, -5.065176, 1, 0.50),
            ],
            starts: 98,
            clusters: 14,
            reranked: if stopped { 3 } else { 20 },
            planned: 20,
            search: qcdata::SearchSummary {
                requested: 100,
                made: 100,
                minimized: 98,
                used_unconverged_starts: false,
                distinct: 33,
                clusters: 14,
                rerank_planned: 20,
                reranked: if stopped { 3 } else { 20 },
                kept: 3,
            },
            stopped,
            notes: vec![],
            seconds: 12.5,
        })))
    }

    impl App {
        pub(in crate::app) fn xtb_searched_water() -> (Self, u64) {
            let (mut app, oxygen) = water(CalcTask::Conformers);
            search(&mut app, found(false));
            (app, oxygen)
        }
    }

    #[test]
    fn searches_save_the_effective_minimizer_and_optimizer_algorithms() {
        let (mut app, _) = water(CalcTask::Conformers);
        let Event::Done(Ok(mut found)) = found(false) else {
            unreachable!()
        };
        found.minimizer = crate::app::xtb::conformers::Minimizer::Uff;
        found.optimizers = vec![
            reshiki_optimize::Method::Internal,
            reshiki_optimize::Method::AncRfo,
            reshiki_optimize::Method::Lbfgs,
        ];
        search(&mut app, Event::Done(Ok(found)));
        let result = &app.tab.doc.science.results[0].result;
        assert_eq!(
            result.provenance.method.as_deref(),
            Some(
                "UFF ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank · optimizer TRIC, ANC, L-BFGS"
            )
        );
        assert_eq!(
            crate::app::xtb::conformers::start_field(result),
            Some("UFF")
        );
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
            Some("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank · optimizer TRIC")
        );
        // The lowest conformer is applied to the 3D model as one Undo step.
        assert_eq!(model(&app.tab.doc), f32s(&A));
        assert!(
            matches!(
                &app.tab.xtb.outcome,
                Some(Outcome::Done { line, .. }) if line == "3 conformers kept · 98 starts · 14 clusters · lowest E −5.070176 Eh"
            ),
            "{:?}",
            app.status
        );
        assert_eq!(
            app.tab.xtb.palette.view,
            crate::app::xtb::results::View::Conformers
        );

        let shown = app.xtb_shown().unwrap();
        let table = app.xtb_conformer_table(&shown);
        let rows = &table.rows;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].stereo, Some(true));
        assert_eq!(rows[1].rmsd, Some(0.35));
        drop(table);

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
        let file = app.tab.doc.file_json().unwrap();
        let restored = reshiki::document::Document::from_native_file(&file).unwrap();
        let result = &restored.science.results[0].result;
        assert_eq!(result.run, app.tab.doc.science.results[0].result.run,);
        assert_eq!(
            super::view::search_funnel(result).unwrap(),
            "100 starts · 98 minimized with MMFF94s · 33 distinct · 14 clusters · GFN2-xTB re-ranked 20 · 3 kept",
        );
        let path = result.trajectory.as_ref().unwrap();
        assert_eq!(path.rmsd_to_best_angstrom, [0., 0.35, 0.50]);
        let xyz =
            super::super::export::xyz(&app.tab.doc.science.results[0].result, "water").unwrap();
        assert_eq!(xyz.lines().filter(|l| l.trim() == "3").count(), 3);
    }

    #[test]
    fn the_search_summary_counts_only_saved_conformers() {
        let (mut app, _) = water(CalcTask::Conformers);
        let Event::Done(Ok(mut found)) = found(false) else {
            unreachable!()
        };
        let found_mut = &mut *found;
        found_mut.ranked.resize(40, ranked(C, -5.065176, 1, 0.5));
        found_mut.search.kept = found_mut.ranked.len();
        search(&mut app, Event::Done(Ok(found)));

        let result = &app.tab.doc.science.results[0].result;
        let saved = result.trajectory.as_ref().unwrap().steps();
        assert_eq!(saved, 32);
        assert_eq!(
            result.run.as_ref().unwrap().search.as_ref().unwrap().kept,
            saved
        );
        assert!(
            super::view::search_funnel(result)
                .unwrap()
                .ends_with("32 kept")
        );
        let mut legacy = (**result).clone();
        legacy.run.as_mut().unwrap().search.as_mut().unwrap().kept = 40;
        assert!(
            super::view::search_funnel(&legacy)
                .unwrap()
                .ends_with("32 kept")
        );
    }

    #[test]
    fn a_copy_sharing_the_result_gets_its_own_table() {
        let (mut app, oxygen) = water(CalcTask::Conformers);
        search(&mut app, found(false));
        app.xtb_show_result(&app.tab.doc.science.results[0].result.clone());
        let copied = crate::app::xtb::tests::duplicate_molecule(&mut app);
        let mut attachments = vec![];
        for selection in [copied, vec![oxygen]] {
            app.tab.selected = selection;
            let shown = app.xtb_shown().unwrap();
            // Same result and revision: only the attachment tells the tables apart.
            assert_eq!(
                app.xtb_conformer_table(&shown).attachment,
                shown.stored.attachment
            );
            attachments.push(shown.stored.attachment);
        }
        assert_ne!(attachments[0], attachments[1]);
    }

    #[test]
    fn export_uses_the_results_caption_then_saved_stem_then_hill_formula() {
        use super::Format;
        use std::path::PathBuf;

        let (mut app, oxygen) = water(CalcTask::Conformers);
        search(&mut app, found(false));
        app.xtb_show_result(&app.tab.doc.science.results[0].result.clone());
        app.tab.untitled_name = Some("Unsaved drawing".into());
        let prepare = |app: &App, format| {
            app.xtb_prepare_conformer_export(&app.xtb_shown().unwrap(), format)
                .unwrap()
        };
        let xyz = prepare(&app, Format::Xyz);
        assert_eq!(xyz.name, "H2O-conformers.xyz");
        assert!(
            String::from_utf8(xyz.bytes)
                .unwrap()
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("H2O conformer 1")
        );
        app.tab.path = Some("Untitled.rshk".into());
        let sdf = prepare(&app, Format::Sdf);
        assert_eq!(sdf.name, "Untitled-conformers.sdf");
        assert!(sdf.bytes.starts_with(b"Untitled conformer 1\n"));

        app.tab.path = Some("/tmp/Saved Water.rshk".into());
        let sdf = prepare(&app, Format::Sdf);
        assert_eq!(sdf.name, "Saved-Water-conformers.sdf");
        assert!(sdf.bytes.starts_with(b"Saved Water conformer 1\n"));

        let outside = app.tab.doc.add_atom("O", Point::new(100., 0.));
        for (atom, name) in [
            (outside, "Another molecule"),
            (oxygen, "Water / sample: 1?"),
        ] {
            reshiki::molecule_names::show(
                &mut app.tab.doc,
                &[atom],
                name.into(),
                "O".into(),
                Default::default(),
                false,
            )
            .unwrap();
        }
        // Another molecule's caption never names this result's export.
        app.tab.selected = vec![];
        let sdf = prepare(&app, Format::Sdf);
        assert_eq!(sdf.name, "Water-sample-1-conformers.sdf");
        assert!(sdf.bytes.starts_with(b"Water / sample: 1? conformer 1\n"));
        let saved = PathBuf::from("/tmp/Water-sample-1-conformers.sdf");
        let _ = app.update(sdf.completed(Ok(Some(saved))));
        assert_eq!(
            app.status,
            "Exported Water-sample-1-conformers.sdf · 3 conformers",
        );

        reshiki::molecule_names::show(
            &mut app.tab.doc,
            &[oxygen],
            " / : ? ".into(),
            "O".into(),
            Default::default(),
            false,
        )
        .unwrap();
        let sdf = prepare(&app, Format::Sdf);
        assert_eq!(sdf.name, "Saved-Water-conformers.sdf");
        assert!(sdf.bytes.starts_with(b"Saved Water conformer 1\n"));

        app.tab.path = Some("???.rshk".into());
        let sdf = prepare(&app, Format::Sdf);
        assert_eq!(sdf.name, "H2O-conformers.sdf");
        assert!(sdf.bytes.starts_with(b"H2O conformer 1\n"));
    }

    #[tokio::test]
    #[ignore = "Opt-in bottom dock layout check with a headless renderer"]
    async fn bottom_dock_shows_five_full_rows_and_counts_the_saved_frames() {
        use iced::advanced::{renderer::Headless, widget::operation::black_box};
        use iced_runtime::{UserInterface, user_interface::Cache};

        let (mut app, _) = water(CalcTask::Conformers);
        let Event::Done(Ok(mut found)) = found(false) else {
            unreachable!()
        };
        let result = &mut *found;
        result
            .ranked
            .extend((3..9).map(|index| ranked(C, -5.065176 + f64::from(index) * 0.0001, 1, 0.5)));
        result.search.kept = result.ranked.len();
        search(&mut app, Event::Done(Ok(found)));
        let size = iced::Size::new(1440., 900.);
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        let mut ui = UserInterface::build(app.view(), size, Cache::new(), &mut renderer);
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut black_box(&mut collect));
        let nodes = &collect.snapshot().nodes;
        let fully_visible = nodes
            .iter()
            .filter(|node| {
                node.id
                    .strip_prefix("xtb-conformer-")
                    .is_some_and(|id| id.parse::<usize>().is_ok())
                    && node
                        .visible_bounds
                        .is_some_and(|visible| visible.height >= node.bounds.height - 0.1)
            })
            .count();
        assert!((5..9).contains(&fully_visible), "{fully_visible} full rows");
        for (id, label) in [
            ("xtb-export-conformers-sdf", "Export all 9 as SDF…"),
            ("xtb-export-conformers-xyz", "Export all 9 as XYZ…"),
        ] {
            assert!(nodes.iter().any(|node| node.id == id && node.name == label));
        }
        eprintln!("{fully_visible} full conformer rows at 1440×900");
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
