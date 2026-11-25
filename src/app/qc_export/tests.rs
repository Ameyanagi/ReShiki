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
    let positions = (0..n + 1)
        .map(|i| Point3 {
            x: i as f64 * 1.4,
            y: (i % 2) as f64 * 0.8,
            z: i as f64 * 0.1,
        })
        .collect();
    let optimized = reshiki::geometry::Optimized {
        conformer: Conformer {
            positions,
            original_atom_count: n,
            hydrogen_parents: vec![0],
        },
        initial_energy: 1.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 1,
        force_field: job.force_field,
        diagnostics: vec![],
        approximations: vec![],
        ensemble: vec![],
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
    assert_eq!(
        preview(&app),
        Err("GAMESS exports use built-in bases only: 6-31G(d), 6-31G or STO-3G.".into())
    );
    send(&mut app, Action::Field(Field::Basis, "6-31G(d)".into()));
    assert!(
        preview(&app)
            .unwrap()
            .contains("GBASIS=N31 NGAUSS=6 NDFUNC=1")
    );

    send(&mut app, Action::Program(Program::Orca));
    send(&mut app, Action::Job(Job::Irc));
    send(&mut app, Action::Program(Program::Gamess));
    let dialog = app.qc_export.as_ref().unwrap();
    assert_eq!(dialog.job, Job::TransitionState);
    assert!(dialog.message.contains("GAMESS does not offer IRC"));

    send(&mut app, Action::Field(Field::Cores, "x".into()));
    assert_eq!(preview(&app), Err("Cores must be a whole number.".into()));
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
    app.tab.selected = vec![atoms[0]];
    send(&mut app, Action::Open);
    assert_eq!(app.qc_export.as_ref().unwrap().charge, "-1");
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
    assert!(app.qc_export.is_none());
    assert_eq!(app.status, "Saved /tmp/x.gjf");
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
