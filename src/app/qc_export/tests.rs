use super::*;
use crate::app::models3d::Action as ModelAction;
use reshiki::document::Point;
use reshiki::geometry::{Conformer, Point3};
use reshiki::models3d::Display;
use std::sync::Arc;

fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::QcExport(action));
}

/// Ethanol, selected, and a separate oxygen atom.
fn ethanol() -> (App, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let doc = &mut app.tab.doc;
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(36., -21.));
    let c = doc.add_atom("O", Point::new(72., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc.add_atom("O", Point::new(200., 80.));
    app.tab.selected = vec![b];
    (app, vec![a, b, c])
}

/// Finish the 3D model the dialog asked for, with hydrogens on one atom.
fn finish_model(app: &mut App) {
    let job = app.tab.model_job.as_ref().expect("a generation in flight");
    let n = job.prepared.ids().len();
    let positions = (0..n)
        .map(|i| Point3 {
            x: i as f64 * 1.4,
            y: (i % 2) as f64 * 0.8,
            z: i as f64 * 0.1,
        })
        .chain([Point3 {
            x: -0.8,
            y: 0.,
            z: 0.,
        }])
        .collect();
    let optimized = reshiki::geometry::Optimized {
        conformer: Conformer {
            positions,
            original_atom_count: n,
            hydrogen_parents: vec![0],
        },
        ensemble: vec![],
        initial_energy: 1.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 1,
        force_field: job.force_field,
        diagnostics: vec![],
        approximations: vec![],
    };
    let key = job.key;
    let _ = app.update(Message::Models3d(ModelAction::Generated(
        key,
        Ok(Arc::new(optimized)),
    )));
}

fn preview(app: &App) -> Result<String, String> {
    app.qc_output().map(|output| output.main.text.clone())
}

#[test]
fn opens_without_a_model_and_writes_once_the_model_is_generated() {
    let (mut app, atoms) = ethanol();
    send(&mut app, Action::Open);
    assert!(app.qc_export.is_some());
    assert_eq!(
        preview(&app),
        Err("The molecule needs a 3D model first.".into())
    );
    send(&mut app, Action::Generate);
    assert_eq!(app.tab.selected, atoms);
    finish_model(&mut app);
    assert_eq!(app.tab.doc.models[0].display, Display::Both);
    let text = preview(&app).unwrap();
    assert!(text.contains("#p B3LYP/Def2SVP EmpiricalDispersion=GD3BJ Opt Freq"));
    assert!(text.contains(", MMFF94s force-field model (ReShiki)"));
    assert!(text.contains("\n0 1\n"));
    // Three drawn atoms and the one generated hydrogen.
    let atom_lines = text
        .lines()
        .filter(|l| l.starts_with(" C ") || l.starts_with(" O ") || l.starts_with(" H "))
        .count();
    assert_eq!(atom_lines, 4);
}

#[test]
fn charge_and_multiplicity_come_from_the_drawing() {
    let (mut app, atoms) = ethanol();
    let oxygen = app
        .tab
        .doc
        .atoms
        .iter_mut()
        .find(|a| a.id == atoms[2])
        .unwrap();
    oxygen.charge = -1;
    let carbon = app
        .tab
        .doc
        .atoms
        .iter_mut()
        .find(|a| a.id == atoms[0])
        .unwrap();
    carbon.radical_electrons = 1;
    send(&mut app, Action::Open);
    let dialog = app.qc_export.as_ref().unwrap();
    assert_eq!(
        (dialog.charge.as_str(), dialog.multiplicity.as_str()),
        ("-1", "2")
    );
}

#[test]
fn nothing_to_export_reports_instead_of_opening() {
    let (mut app, _) = ethanol();
    app.tab.selected.clear();
    send(&mut app, Action::Open);
    assert!(app.qc_export.is_none());
    assert!(app.error);
    assert_eq!(
        app.status,
        "Select one molecule to export as calculation input."
    );
}

#[test]
fn programs_jobs_and_bases_follow_the_writers() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);

    send(&mut app, Action::Program(Program::Gamess));
    let dialog = app.qc_export.as_ref().unwrap();
    assert_eq!(dialog.basis, "6-31G(d)");
    assert_eq!(
        dialog.basis_note().as_deref(),
        Some("The ReShiki GAMESS exporter does not support def2-SVP. Using 6-31G(d).")
    );
    assert!(
        preview(&app)
            .unwrap()
            .contains("GBASIS=N31 NGAUSS=6 NDFUNC=1")
    );
    send(&mut app, Action::Job(Job::Optimize));
    assert!(app.qc_export.as_ref().unwrap().basis_note().is_some());
    send(&mut app, Action::Program(Program::Gaussian));
    assert_eq!(app.qc_export.as_ref().unwrap().basis, "def2-SVP");

    send(&mut app, Action::Program(Program::Orca));
    send(&mut app, Action::Job(Job::Irc));
    send(&mut app, Action::Program(Program::Gamess));
    let dialog = app.qc_export.as_ref().unwrap();
    assert_eq!(dialog.job, Job::TransitionState);
    assert!(dialog.message.contains("GAMESS does not offer IRC"));

    send(&mut app, Action::Field(Field::Cores, "x".into()));
    assert_eq!(preview(&app), Err("Cores must be a whole number.".into()));

    send(&mut app, Action::Field(Field::Basis, "STO-3G".into()));
    send(&mut app, Action::Program(Program::Orca));
    assert_eq!(app.qc_export.as_ref().unwrap().basis, "STO-3G");
}

#[test]
fn an_explicit_basis_edit_survives_switching_from_gamess() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Program(Program::Gamess));
    send(&mut app, Action::Field(Field::Basis, "6-31G".into()));
    send(&mut app, Action::Field(Field::Basis, "6-31G(d)".into()));
    assert!(app.qc_export.as_ref().unwrap().basis_note().is_none());
    send(&mut app, Action::Program(Program::Orca));
    assert_eq!(app.qc_export.as_ref().unwrap().basis, "6-31G(d)");
}

#[test]
fn gamess_substitutes_unsupported_and_empty_template_bases() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    send(&mut app, Action::Program(Program::Gamess));
    let [default, ..] = qc_input::builtin_templates();
    let gamess = Template {
        name: "GAMESS".into(),
        basis: String::new(),
        only: Some(Program::Gamess),
        ..default.clone()
    };
    for (template, note) in [
        (
            default,
            "The ReShiki GAMESS exporter does not support def2-SVP. Using 6-31G(d).",
        ),
        (
            gamess.clone(),
            "No basis was specified for GAMESS. Using 6-31G(d).",
        ),
    ] {
        send(&mut app, Action::Template(Choice(template)));
        assert!(
            preview(&app)
                .unwrap()
                .contains("GBASIS=N31 NGAUSS=6 NDFUNC=1")
        );
        assert_eq!(
            app.qc_export.as_ref().unwrap().basis_note().as_deref(),
            Some(note)
        );
    }
    let supported = Template {
        basis: "STO-3G".into(),
        ..gamess
    };
    send(&mut app, Action::Template(Choice(supported)));
    assert!(app.qc_export.as_ref().unwrap().basis_note().is_none());
    assert!(preview(&app).unwrap().contains("GBASIS=STO NGAUSS=3"));
}

#[test]
fn orca_only_templates_fall_back_on_other_programs() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Program(Program::Orca));
    let [_, r2scan, _] = qc_input::builtin_templates();
    send(&mut app, Action::Template(Choice(r2scan)));
    assert_eq!(app.qc_export.as_ref().unwrap().method, "r2SCAN-3c");
    send(&mut app, Action::Program(Program::Gaussian));
    let dialog = app.qc_export.as_ref().unwrap();
    assert_eq!(dialog.method, "B3LYP-D3BJ");
    assert_eq!(dialog.template.name, "B3LYP-D3BJ/def2-SVP opt+freq");
    assert_eq!(
        dialog.message,
        "r2SCAN-3c opt+freq is for ORCA only; switched to B3LYP-D3BJ/def2-SVP opt+freq."
    );
}

#[test]
fn user_templates_are_saved_replaced_and_deleted() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Field(Field::Method, "PBE0".into()));
    send(&mut app, Action::NameTemplate);
    send(&mut app, Action::TemplateName(" PBE0 ".into()));
    send(&mut app, Action::SaveTemplate);
    assert_eq!(app.appearance.input_templates.len(), 1);
    assert_eq!(app.appearance.input_templates[0].name, "PBE0");
    assert_eq!(app.appearance.input_templates[0].method, "PBE0");
    assert_eq!(
        app.qc_export.as_ref().unwrap().template,
        app.appearance.input_templates[0]
    );

    send(&mut app, Action::Field(Field::Basis, "def2-TZVP".into()));
    send(&mut app, Action::NameTemplate);
    send(&mut app, Action::TemplateName("PBE0".into()));
    send(&mut app, Action::SaveTemplate);
    assert_eq!(app.appearance.input_templates.len(), 1);
    assert_eq!(app.appearance.input_templates[0].basis, "def2-TZVP");

    send(&mut app, Action::DeleteTemplate);
    assert!(app.appearance.input_templates.is_empty());
    assert_eq!(
        app.qc_export.as_ref().unwrap().template.name,
        "B3LYP-D3BJ/def2-SVP opt+freq"
    );
}

#[test]
fn escape_closes_and_command_return_saves() {
    assert!(matches!(
        modal_key(&Key::Named(Named::Escape), Modifiers::empty()),
        Some(Message::QcExport(Action::Close))
    ));
    assert!(matches!(
        modal_key(&Key::Named(Named::Enter), Modifiers::COMMAND),
        Some(Message::QcExport(Action::Save))
    ));
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Close);
    assert!(app.qc_export.is_none());
}

#[test]
fn file_stems_keep_names_readable() {
    assert_eq!(stem("aspirin"), "aspirin");
    assert_eq!(stem(" 2-methyl propane "), "2-methyl_propane");
    assert_eq!(stem("(+)"), "molecule");
}

#[test]
fn each_program_has_its_own_default_file_name() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    app.qc_export.as_mut().unwrap().name = "ethanol".into();
    for (program, name) in [
        (Program::Gaussian, "ethanol.gjf"),
        (Program::Orca, "ethanol-orca.inp"),
        (Program::Gamess, "ethanol-gamess.inp"),
        (Program::Xyz, "ethanol.xyz"),
    ] {
        send(&mut app, Action::Program(program));
        assert_eq!(app.qc_export.as_ref().unwrap().file_name(), name);
        assert_eq!(app.qc_output().unwrap().main.name, name);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_conformer_changes_the_written_geometry_and_only_the_automatic_title() {
    let (mut app, _) = App::xtb_searched_water();
    send(&mut app, Action::Open);
    send(&mut app, Action::Program(Program::Xyz));
    let first = preview(&app).unwrap();
    assert!(
        first
            .lines()
            .nth(1)
            .unwrap()
            .ends_with(", GFN2-xTB conformer 1 (ReShiki)")
    );

    let (cache, first_choice, second_choice) = {
        let sources = app.qc_sources(app.qc_export.as_ref().unwrap());
        (sources.as_ptr(), sources[0].0.clone(), sources[1].0.clone())
    };
    send(&mut app, Action::Source(second_choice.clone()));
    let second = preview(&app).unwrap();
    assert!(
        second
            .lines()
            .nth(1)
            .unwrap()
            .ends_with(", GFN2-xTB conformer 2 (ReShiki)")
    );
    let written = qc_input::parse_xyz(&second).unwrap();
    let pairs: Vec<_> = written
        .numbers
        .into_iter()
        .zip(written.positions_angstrom)
        .collect();
    assert_eq!(
        pairs,
        [(8, [0., 0., 0.]), (1, [0.97, 0., 0.]), (1, [-0.3, 0.9, 0.])]
    );
    assert_ne!(first.lines().nth(3), second.lines().nth(3));

    send(
        &mut app,
        Action::Field(Field::Title, "DFT follow-up".into()),
    );
    assert_eq!(
        app.qc_sources(app.qc_export.as_ref().unwrap()).as_ptr(),
        cache
    );
    send(&mut app, Action::Source(first_choice));
    let custom = preview(&app).unwrap();
    assert_eq!(custom.lines().nth(1), Some("DFT follow-up"));
    assert_eq!(custom.lines().nth(3), first.lines().nth(3));

    send(&mut app, Action::Source(second_choice));
    app.tab.doc.atoms[0].charge = -1;
    app.tab.revision += 1;
    assert_eq!(preview(&app), Err(GEOMETRY_CHANGED.into()));
    assert_eq!(app.qc_export.as_ref().unwrap().source.get(), 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn an_xtb_geometry_defaults_to_its_calculated_electronic_state() {
    use crate::app::xtb::{Action as XtbAction, CalcTask};

    let (mut app, _) = App::new();
    app.tab.busy = false;
    let doc = &mut app.tab.doc;
    let n = doc.add_atom("N", Point::default());
    let o = doc.add_atom("O", Point::new(36., 0.));
    doc.add_bond(n, o, 2, "plain");
    app.tab.selected = vec![n];
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    assert!(preview(&app).unwrap().contains("\n0 1\n"));

    for action in [
        XtbAction::Task(CalcTask::Optimize),
        XtbAction::Charge("-2".into()),
        XtbAction::Unpaired("2".into()),
        XtbAction::Run,
    ] {
        let _ = app.update(Message::Xtb(action));
    }
    app.xtb_test_finish_optimization(
        &[[0., 0., 0.], [1.4, 0.8, 0.1], [-0.8, 0., 0.]],
        &[[0., 0., 0.], [1.3, 0.8, 0.1], [-0.75, 0., 0.]],
    );
    assert!(
        app.status.starts_with("Applied xTB geometry"),
        "{}",
        app.status
    );
    let gaussian = preview(&app).unwrap();
    assert!(gaussian.contains(", GFN2-xTB optimized (ReShiki)"));
    assert!(gaussian.contains("\n-2 3\n"));

    let start = app.qc_sources(app.qc_export.as_ref().unwrap())[1].0.clone();
    send(&mut app, Action::Source(start));
    let gaussian = preview(&app).unwrap();
    assert!(gaussian.contains(", start geometry before xTB (ReShiki)"));
    assert!(gaussian.contains("\n-2 3\n"));
    send(&mut app, Action::Field(Field::Charge, "0".into()));
    assert!(preview(&app).unwrap().contains("\n0 3\n"));
    let dialog = app.qc_export.as_ref().unwrap();
    let sources = app.qc_sources(dialog);
    let (choice, source) = dialog.selected_source(&sources).unwrap();
    assert_eq!(choice.index, 1);
    assert_eq!(
        dialog.electronic_state_warning(Some(source)).as_deref(),
        Some("This geometry was calculated with charge -2 and multiplicity 3.")
    );
    let optimized = sources[0].0.clone();
    drop(sources);

    send(&mut app, Action::Field(Field::Multiplicity, "1".into()));
    assert!(preview(&app).unwrap().contains("\n0 1\n"));
    send(&mut app, Action::Program(Program::Xyz));
    let xyz = qc_input::parse_xyz(&preview(&app).unwrap()).unwrap();
    let expected = [(7, [0., 0., 0.]), (8, [1.4, 0.8, 0.1]), (1, [-0.8, 0., 0.])];
    assert_eq!(xyz.numbers.len(), expected.len());
    for ((number, xyz), (element, position)) in xyz
        .numbers
        .into_iter()
        .zip(xyz.positions_angstrom)
        .zip(expected)
    {
        assert_eq!(number, element);
        for (actual, expected) in xyz.into_iter().zip(position) {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
    }
    send(&mut app, Action::Source(optimized));
    send(&mut app, Action::Program(Program::Gaussian));
    assert!(preview(&app).unwrap().contains("\n0 1\n"));
}

#[test]
fn xyz_ignores_the_fields_it_disables() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    send(&mut app, Action::Field(Field::Cores, String::new()));
    send(&mut app, Action::Field(Field::Charge, "-".into()));
    send(&mut app, Action::Program(Program::Xyz));
    assert!(preview(&app).unwrap().starts_with("4\n"));
}

#[test]
fn drawn_isotopes_reach_the_writer_warning() {
    let (mut app, atoms) = ethanol();
    let carbon = app
        .tab
        .doc
        .atoms
        .iter_mut()
        .find(|a| a.id == atoms[0])
        .unwrap();
    carbon.isotope = 13;
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    let warnings = app.qc_output().unwrap().warnings;
    assert!(
        warnings
            .contains(&"Isotopes are not written; set them in Gaussian if you need them.".into())
    );
}

#[test]
fn switching_tabs_closes_the_dialog() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    let _ = app.update(Message::New);
    assert!(app.qc_export.is_none());
}

#[test]
fn a_selection_across_molecules_opens_nothing_even_with_one_model() {
    let (mut app, atoms) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    send(&mut app, Action::Close);
    let water = app.tab.doc.atoms.last().unwrap().id;
    app.tab.selected = vec![atoms[0], water];
    send(&mut app, Action::Open);
    assert!(app.qc_export.is_none());
}

#[test]
fn charge_follows_the_drawing_after_the_model_was_made() {
    let (mut app, atoms) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    send(&mut app, Action::Close);
    let oxygen = app
        .tab
        .doc
        .atoms
        .iter_mut()
        .find(|a| a.id == atoms[2])
        .unwrap();
    oxygen.charge = -1;
    app.tab.revision += 1;
    app.tab.selected = vec![atoms[0]];
    send(&mut app, Action::Open);
    assert_eq!(app.qc_export.as_ref().unwrap().charge, "-1");
    send(&mut app, Action::Update);
    finish_model(&mut app);
    assert!(preview(&app).unwrap().contains("\n-1 1\n"));
}

#[test]
fn a_stale_model_is_not_written_until_updated() {
    let (mut app, atoms) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    send(&mut app, Action::Close);
    let doc = &mut app.tab.doc;
    let added = doc.add_atom("C", Point::new(108., -21.));
    doc.add_bond(atoms[2], added, 1, "plain");
    // An edit moves the revision, which the staleness cache is keyed by.
    app.tab.revision += 1;
    app.tab.selected = vec![atoms[0]];
    send(&mut app, Action::Open);
    assert_eq!(
        preview(&app),
        Err("The drawing changed since this 3D model was made: update the model first.".into())
    );
    send(&mut app, Action::Update);
    finish_model(&mut app);
    assert!(preview(&app).unwrap().starts_with("%nprocshared"));
}

#[test]
fn an_earlier_dialogs_save_receipt_leaves_the_open_dialog_alone() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    let first = app.qc_export.as_ref().unwrap().id;
    send(&mut app, Action::Close);
    send(&mut app, Action::Open);
    send(&mut app, Action::Field(Field::Title, "typed".into()));
    send(&mut app, Action::Saved(first, Err("disk full".into())));
    assert_eq!(app.qc_export.as_ref().unwrap().title, "typed");
    assert_eq!((app.status.as_str(), app.error), ("disk full", true));
    send(
        &mut app,
        Action::Saved(first, Ok(Some("/tmp/old.gjf".into()))),
    );
    assert_eq!(app.qc_export.as_ref().unwrap().title, "typed");
    assert_eq!(app.status, "Saved /tmp/old.gjf");
    let current = app.qc_export.as_ref().unwrap().id;
    send(
        &mut app,
        Action::Saved(current, Ok(Some("/tmp/x.gjf".into()))),
    );
    let dialog = app.qc_export.as_ref().unwrap();
    assert!(dialog.saved);
    assert_eq!(
        dialog.message,
        "Saved x.gjf · choose another program or Close"
    );
    assert_eq!(app.status, "Saved /tmp/x.gjf");
    send(&mut app, Action::Close);
    assert!(app.qc_export.is_none());
}

#[test]
fn a_hidden_irc_field_does_not_block_other_tasks() {
    let (mut app, _) = ethanol();
    send(&mut app, Action::Open);
    send(&mut app, Action::Generate);
    finish_model(&mut app);
    send(&mut app, Action::Field(Field::IrcPoints, String::new()));
    assert!(preview(&app).is_ok());
    send(&mut app, Action::Job(Job::Irc));
    assert_eq!(
        preview(&app),
        Err("IRC points must be a whole number.".into())
    );
}
