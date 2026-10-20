use super::*;
use reshiki::document::{Document, Point};
use reshiki::geometry::{Conformer, Point3};

/// Ethanol, selected, and a separate oxygen; saved, so edits show as unsaved.
fn drawing() -> (App, Vec<u64>, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let doc = &mut app.tab.doc;
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(36., -21.));
    let c = doc.add_atom("O", Point::new(72., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    let water = doc.add_atom("O", Point::new(200., 80.));
    app.tab.selected = vec![b];
    app.tab.saved = app.tab.doc.clone();
    (app, vec![a, b, c], water)
}

fn generated(app: &App) -> (Key, Arc<Optimized>) {
    let job = app.tab.model_job.as_ref().expect("a generation in flight");
    let n = job.prepared.ids().len();
    let positions = (0..n + 1)
        .map(|i| Point3 {
            x: i as f64 * 1.4,
            y: (i % 2) as f64 * 0.8,
            z: i as f64 * 0.1,
        })
        .collect();
    let optimized = Optimized {
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
        force_field: ForceField::Mmff94s,
        diagnostics: vec![],
    };
    (job.key, Arc::new(optimized))
}

fn show(app: &mut App, display: Display) {
    let _ = app.update(Message::Models3d(Action::Show(display)));
}

fn frames(app: &App) -> usize {
    app.tab.history.undo_frames().count()
}

fn displays(doc: &Document) -> Vec<Display> {
    doc.models.iter().map(|m| m.display).collect()
}

#[test]
fn each_switch_is_one_undo_step_and_keeps_chemistry() {
    let (mut app, atoms, _) = drawing();
    let original = app.tab.doc.clone();
    show(&mut app, Display::ThreeD);
    assert!(
        app.tab.doc.models.is_empty(),
        "nothing changes before the worker answers"
    );
    assert_eq!(frames(&app), 0);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(!app.error, "{}", app.status);
    assert_eq!(displays(&app.tab.doc), [Display::ThreeD]);
    assert_eq!(frames(&app), 1);
    assert_eq!(app.tab.doc.version, reshiki::document::MODELS_3D);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let model = &app.tab.doc.models[0];
    assert_eq!(model.atoms.iter().map(|a| a.id).collect::<Vec<_>>(), atoms);
    assert_eq!(model.hydrogens[0].parent, atoms[0]);
    assert!(app.dirty());

    show(&mut app, Display::TwoD);
    assert_eq!(displays(&app.tab.doc), [Display::TwoD]);
    assert!(app.tab.model_job.is_none(), "a kept model needs no worker");
    show(&mut app, Display::Both);
    assert_eq!(displays(&app.tab.doc), [Display::Both]);
    show(&mut app, Display::Both);
    assert_eq!(frames(&app), 3, "repeating a display records nothing");

    // Saved, then switched: the tab shows unsaved changes.
    app.tab.saved = app.tab.doc.clone();
    show(&mut app, Display::ThreeD);
    assert!(app.dirty());

    for expected in [
        vec![Display::Both],
        vec![Display::TwoD],
        vec![Display::ThreeD],
        vec![],
    ] {
        let _ = app.update(Message::Undo);
        assert_eq!(displays(&app.tab.doc), expected);
    }
    assert_eq!(app.tab.doc.atoms, original.atoms);
}

#[test]
fn a_result_for_a_changed_drawing_is_dropped() {
    let (mut app, atoms, _) = drawing();
    show(&mut app, Display::Both);
    let (key, result) = generated(&app);
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(atoms[0]).unwrap().position = Point::new(-5., 3.);
    app.changed(before);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(app.tab.doc.models.is_empty());
    assert!(app.tab.model_job.is_none());

    // A newer request replaces an older one; only the newer result applies.
    show(&mut app, Display::ThreeD);
    let (old, result) = generated(&app);
    show(&mut app, Display::Both);
    let _ = app.update(Message::Models3d(Action::Generated(
        old,
        Ok(result.clone()),
    )));
    assert!(app.tab.doc.models.is_empty());
    let (key, _) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert_eq!(displays(&app.tab.doc), [Display::Both]);
}

#[test]
fn errors_and_other_selections_leave_the_drawing_alone() {
    let (mut app, _, water) = drawing();
    let original = app.tab.doc.clone();
    show(&mut app, Display::ThreeD);
    let (key, _) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Err("no".into()))));
    assert!(app.error);
    assert_eq!(app.tab.doc, original);

    app.tab.selected = vec![original.atoms[0].id, water];
    show(&mut app, Display::ThreeD);
    assert!(app.error && app.tab.model_job.is_none());
    assert!(app.structure_section().is_none());
    app.tab.selected = vec![water];
    assert!(app.structure_section().is_some());
}

#[test]
fn a_generated_model_frame_selects_its_molecule() {
    let (mut app, atoms, _) = drawing();
    show(&mut app, Display::ThreeD);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    let doc = &app.tab.doc;
    assert!(reshiki::models3d::shows_3d(doc, &atoms));
    let (lo, hi) = reshiki::models3d::frame(doc, &doc.models[0]).unwrap();
    let center = reshiki::document::Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert_eq!(reshiki::models3d::frame_hit(doc, center), Some(atoms));
}

#[tokio::test]
#[ignore = "Opt-in real renderer Structure section controls"]
async fn structure_section_buttons_show_the_molecule() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::Operation, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let (app, ..) = drawing();
    let size = iced::Size::new(288., 400.);
    for (id, display) in [
        ("structure-show-2D", Display::TwoD),
        ("structure-show-3D", Display::ThreeD),
        ("structure-show-Both", Display::Both),
    ] {
        let section = app.structure_section().ok_or("No Structure section")?;
        let mut ui = iced_runtime::UserInterface::build(
            section,
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
        ui.operate(&renderer, &mut operation::black_box(&mut activate));
        match activate.finish() {
            operation::Outcome::Some(Message::Models3d(Action::Show(shown))) => {
                assert_eq!(shown, display)
            }
            _ => return Err(format!("{id} did not ask to show the molecule")),
        }
    }
    Ok(())
}

fn with_model(display: Display) -> (App, Vec<u64>) {
    let (mut app, atoms, _) = drawing();
    show(&mut app, display);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert_eq!(displays(&app.tab.doc), [display]);
    (app, atoms)
}

#[test]
fn duplicate_copies_the_model_with_its_molecule() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    app.tab.selected = atoms.clone();
    app.duplicate_selection();
    let [original, copy] = app.tab.doc.models.as_slice() else {
        panic!("Duplicate keeps one model per molecule");
    };
    assert!(copy.atoms.iter().all(|a| app.tab.selected.contains(&a.id)));
    assert_eq!(copy.rotation, original.rotation);
    assert_eq!(copy.display, Display::ThreeD);
}

#[test]
fn a_background_tab_keeps_its_model_and_receives_its_result() {
    let (mut app, _) = with_model(Display::Both);
    let models = app.tab.doc.models.clone();
    let first = app.tab.id;
    let _ = app.update(Message::New);
    assert_ne!(app.tab.id, first);
    app.select_tab(0);
    assert_eq!(app.tab.id, first);
    assert_eq!(app.tab.doc.models, models);

    // A generation started here completes here while another tab is in front.
    let (mut app, _, water) = drawing();
    app.tab.selected = vec![water];
    show(&mut app, Display::ThreeD);
    let (key, result) = generated(&app);
    let source = app.tab.id;
    let _ = app.update(Message::New);
    let _ = app.update(Message::Tab(
        source,
        Box::new(Message::Models3d(Action::Generated(key, Ok(result)))),
    ));
    assert!(app.tab.doc.models.is_empty());
    app.select_tab(0);
    assert_eq!(app.tab.id, source);
    assert_eq!(displays(&app.tab.doc), [Display::ThreeD]);
}

#[test]
fn cancelling_and_stale_results_say_so() {
    let (mut app, atoms, _) = drawing();
    show(&mut app, Display::ThreeD);
    assert_eq!(app.pending(&atoms), Some(Display::ThreeD));
    show(&mut app, Display::TwoD);
    assert!(app.tab.model_job.is_none());
    assert!(app.status.contains("cancelled"), "{}", app.status);

    show(&mut app, Display::ThreeD);
    let (key, result) = generated(&app);
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(atoms[0]).unwrap().position = Point::new(-5., 3.);
    app.changed(before);
    assert_eq!(
        app.pending(&atoms),
        None,
        "a stale job is not shown as pending"
    );
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(app.tab.doc.models.is_empty());
    assert!(app.status.contains("not applied"), "{}", app.status);
}

#[test]
fn a_job_belongs_to_its_molecule() {
    let (mut app, atoms) = with_model(Display::Both);
    let water = app.tab.doc.atoms[3].id;
    app.tab.selected = vec![water];
    show(&mut app, Display::ThreeD);
    assert_eq!(app.pending(&[water]), Some(Display::ThreeD));
    assert_eq!(app.pending(&atoms), None);

    // Switching the other molecule keeps the generation in flight.
    app.tab.selected = vec![atoms[0]];
    show(&mut app, Display::TwoD);
    assert_eq!(app.pending(&[water]), Some(Display::ThreeD));
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert_eq!(displays(&app.tab.doc), [Display::TwoD, Display::ThreeD]);
}

#[test]
fn a_result_during_a_modal_edit_is_dropped_and_not_left_pending() {
    let (mut app, atoms, _) = drawing();
    show(&mut app, Display::ThreeD);
    let (key, result) = generated(&app);
    let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
        Some(atoms[0]),
    )));
    assert!(app.tab.atom_text.is_some());
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(app.tab.model_job.is_none());
    assert!(app.tab.doc.models.is_empty());
    assert!(app.status.contains("not applied"), "{}", app.status);
}
