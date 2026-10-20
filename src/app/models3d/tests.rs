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
        // The shown display's button stays pressable, with its selected
        // look, and changes nothing.
        match activate.finish() {
            operation::Outcome::Some(Message::Models3d(Action::Unchanged))
                if display == Display::TwoD => {}
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
    // The worker answers with the key it was started with.
    let (key, result) = generated(&app);

    // Switching the other molecule keeps the generation in flight.
    app.tab.selected = vec![atoms[0]];
    show(&mut app, Display::TwoD);
    assert_eq!(app.pending(&[water]), Some(Display::ThreeD));
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(app.tab.model_job.is_none());
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

#[test]
fn the_status_names_the_view_undo_restores() {
    let (mut app, ..) = drawing();
    show(&mut app, Display::Both);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert_eq!(
        app.status,
        "Shown as 2D and 3D · Undo restores the 2D drawing"
    );
    show(&mut app, Display::TwoD);
    assert_eq!(app.status, "Shown as 2D · Undo restores 2D and 3D");
    show(&mut app, Display::ThreeD);
    assert_eq!(
        app.status,
        "Shown as 3D model · Undo restores the 2D drawing"
    );
    show(&mut app, Display::Both);
    assert_eq!(
        app.status,
        "Shown as 2D and 3D · Undo restores the 3D model"
    );
}

fn set(app: &mut App, setting: Setting) {
    let _ = app.update(Message::Models3d(Action::Set(setting)));
}

/// The active style, look, hydrogens, background or display chosen again
/// keeps the drawing and adds no Undo step.
#[test]
fn choosing_what_is_active_again_changes_nothing() {
    let (mut app, _) = with_model(Display::ThreeD);
    let (start, doc, revision) = (frames(&app), app.tab.doc.clone(), app.tab.revision);
    let depth = doc.models[0].depth_fade;
    for setting in [
        Setting::Style(Style::BallStick),
        Setting::Look(Look::Flat),
        Setting::Hydrogens(Hydrogens::All),
        Setting::Background(Background::None),
        Setting::DepthFade(depth),
    ] {
        set(&mut app, setting);
    }
    show(&mut app, Display::ThreeD);
    let _ = app.update(Message::Models3d(Action::Unchanged));
    assert_eq!(frames(&app), start);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.doc, doc);
}

#[test]
fn each_model_setting_is_one_undo_step_and_a_slider_drag_is_one() {
    let (mut app, _) = with_model(Display::ThreeD);
    let original = app.tab.doc.models[0].clone();
    let start = frames(&app);
    set(&mut app, Setting::Style(Style::SpaceFilling));
    set(&mut app, Setting::Look(Look::Glossy));
    set(&mut app, Setting::Hydrogens(Hydrogens::Polar));
    set(&mut app, Setting::Background(Background::Card));
    set(&mut app, Setting::Background(Background::Card));
    assert_eq!(
        frames(&app),
        start + 4,
        "repeating a choice records nothing"
    );
    // A drag sends many values, then the release.
    for value in [0.4, 0.5, 0.6] {
        set(&mut app, Setting::DepthFade(value));
    }
    let _ = app.update(Message::Models3d(Action::SlideEnd));
    for value in [0.1, 0.0] {
        set(&mut app, Setting::Perspective(value));
    }
    let _ = app.update(Message::Models3d(Action::SlideEnd));
    assert_eq!(frames(&app), start + 6);
    let model = &app.tab.doc.models[0];
    assert_eq!(
        (
            model.style,
            model.look,
            model.show_hydrogens,
            model.background
        ),
        (
            Style::SpaceFilling,
            Look::Glossy,
            Hydrogens::Polar,
            Background::Card
        )
    );
    assert_eq!((model.depth_fade, model.perspective), (0.6, 0.));
    assert_eq!(app.tab.doc.version, reshiki::document::MODELS_3D);

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].perspective, original.perspective);
    assert_eq!(app.tab.doc.models[0].depth_fade, 0.6);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].depth_fade, original.depth_fade);
    for _ in 0..4 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc.models[0], original);
}

#[test]
fn a_slider_change_without_release_does_not_join_later_steps() {
    let (mut app, _) = with_model(Display::ThreeD);
    let start = frames(&app);
    // Arrow keys and Ctrl+wheel change a slider without a release.
    set(&mut app, Setting::DepthFade(0.4));
    set(&mut app, Setting::Style(Style::Wireframe));
    set(&mut app, Setting::Perspective(0.5));
    let _ = app.update(Message::Models3d(Action::SlideEnd));
    assert_eq!(frames(&app), start + 3);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].style, Style::Wireframe);

    // Nor does Undo leave a drag to join.
    set(&mut app, Setting::DepthFade(0.2));
    let _ = app.update(Message::Undo);
    set(&mut app, Setting::Perspective(0.1));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].style, Style::Wireframe);
}

#[test]
fn different_sliders_without_release_are_separate_steps() {
    let (mut app, _) = with_model(Display::ThreeD);
    let start = frames(&app);
    let (fade, perspective) = (
        app.tab.doc.models[0].depth_fade,
        app.tab.doc.models[0].perspective,
    );
    // Arrow keys on Depth fade, then on Perspective, with no release between.
    set(&mut app, Setting::DepthFade(0.4));
    set(&mut app, Setting::DepthFade(0.5));
    set(&mut app, Setting::Perspective(0.6));
    set(&mut app, Setting::Perspective(0.7));
    assert_eq!(frames(&app), start + 2);
    let _ = app.update(Message::Undo);
    let model = &app.tab.doc.models[0];
    assert_eq!((model.depth_fade, model.perspective), (0.5, perspective));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].depth_fade, fade);
}

#[test]
fn changing_a_model_keeps_another_generation_in_flight() {
    let (mut app, atoms) = with_model(Display::Both);
    let water = app.tab.doc.atoms[3].id;
    app.tab.selected = vec![water];
    show(&mut app, Display::ThreeD);
    let (key, result) = generated(&app);
    app.tab.selected = vec![atoms[0]];
    set(&mut app, Setting::Style(Style::Stick));
    set(&mut app, Setting::DepthFade(0.3));
    assert_eq!(app.pending(&[water]), Some(Display::ThreeD));
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert_eq!(displays(&app.tab.doc), [Display::Both, Display::ThreeD]);
}

#[test]
fn reset_view_restores_the_opening_view() {
    let (mut app, _) = with_model(Display::Both);
    let opening = app.tab.doc.models[0].rotation;
    let before = app.tab.doc.clone();
    app.tab.doc.models[0].rotation = [0., 1., 0., 0.];
    app.changed(before);
    let start = frames(&app);
    set(&mut app, Setting::ResetView);
    assert_eq!(app.tab.doc.models[0].rotation, opening);
    assert_eq!(frames(&app), start + 1);
}

#[test]
fn turning_and_resizing_a_model_are_one_undo_step_each() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let water = app.tab.doc.atoms[3].id;
    let model = app.tab.doc.models[0].clone();
    // A generation for another molecule survives both.
    app.tab.selected = vec![water];
    show(&mut app, Display::Both);
    let (key, result) = generated(&app);
    let start = frames(&app);
    let drawn = app.tab.doc.atoms.clone();

    let rotation = reshiki::models3d::turn(model.rotation, 40., -12., false);
    app.edit(Edit::ModelView {
        atoms: atoms.clone(),
        rotation,
    });
    assert_eq!(app.tab.doc.models[0].rotation, rotation);
    let (lo, _) = reshiki::models3d::frame(&app.tab.doc, &app.tab.doc.models[0]).unwrap();
    app.edit(Edit::ResizeModels {
        ids: atoms.clone(),
        pivot: lo,
        scale: 2.,
    });
    assert_eq!(app.tab.doc.models[0].size, model.size * 2.);
    assert_eq!(app.tab.selected, atoms);
    assert_eq!(frames(&app), start + 2);
    assert_eq!(app.tab.doc.atoms, drawn, "the drawing stays");

    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert_eq!(displays(&app.tab.doc), [Display::ThreeD, Display::Both]);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].size, model.size * 2.);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].size, model.size);
    assert_eq!(app.tab.doc.models[0].rotation, rotation);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0], model);
}

#[test]
fn from_the_3d_view_flattens_the_turned_model_in_one_step() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let original = app.tab.doc.clone();
    let quarter = std::f32::consts::FRAC_1_SQRT_2;
    app.edit(Edit::ModelView {
        atoms: atoms.clone(),
        rotation: [quarter, 0., quarter, 0.],
    });
    let start = frames(&app);
    let _ = app.update(Message::Models3d(Action::Flatten));
    assert!(!app.error, "{}", app.status);
    assert_eq!(frames(&app), start + 1);
    let doc = &app.tab.doc;
    assert_eq!(displays(doc), [Display::TwoD], "the model is kept");
    assert_eq!(doc.bonds, original.bonds);
    // The conformer runs along x; turned a quarter about y, x becomes depth.
    let model = &doc.models[0];
    let depths: Vec<_> = atoms
        .iter()
        .map(|id| doc.atom(*id).unwrap().depth)
        .collect();
    assert!(depths.windows(2).all(|d| d[1] < d[0]), "{depths:?}");
    assert_ne!(doc.atoms, original.atoms);
    // Far bonds are lighter, as after 3D optimize.
    assert!(
        doc.depth_appearance
            .iter()
            .any(|s| atoms.iter().all(|id| s.atoms.contains(id)))
    );
    assert_eq!(model.rotation, [quarter, 0., quarter, 0.]);
    assert!(
        app.status.starts_with("Shown as 2D from the 3D view"),
        "{}",
        app.status
    );
    let _ = app.update(Message::Undo);
    assert_eq!(displays(&app.tab.doc), [Display::ThreeD]);
    assert_eq!(app.tab.doc.atoms, original.atoms);
}

#[test]
fn the_frame_keeps_its_place_through_2d_and_back() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let frame = |app: &App| reshiki::models3d::frame(&app.tab.doc, &app.tab.doc.models[0]);
    let (lo, _) = frame(&app).unwrap();
    app.edit(Edit::ResizeModels {
        ids: atoms.clone(),
        pivot: lo,
        scale: 1.5,
    });
    let quarter = std::f32::consts::FRAC_1_SQRT_2;
    app.edit(Edit::ModelView {
        atoms,
        rotation: [quarter, 0., quarter, 0.],
    });
    let placed = frame(&app).unwrap();
    let near = |app: &App| {
        let (a, b) = frame(app).unwrap();
        [
            a.x - placed.0.x,
            a.y - placed.0.y,
            b.x - placed.1.x,
            b.y - placed.1.y,
        ]
        .iter()
        .all(|d| d.abs() < 1e-3)
    };
    let drawn = app.tab.doc.atoms.clone();
    let _ = app.update(Message::Models3d(Action::Flatten));
    assert_ne!(app.tab.doc.atoms, drawn, "flattened");
    show(&mut app, Display::ThreeD);
    assert!(near(&app), "from the 3D view and back");
    show(&mut app, Display::TwoD);
    assert_eq!(app.tab.doc.atoms, drawn, "the drawing restored");
    show(&mut app, Display::ThreeD);
    assert!(near(&app), "from the original drawing and back");
}

#[test]
fn flattening_keeps_drawn_stereo_and_the_model() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let doc = &mut app.tab.doc;
    let c = doc.add_atom("C", Point::default());
    let f = doc.add_atom("F", Point::new(0., -36.));
    let cl = doc.add_atom("Cl", Point::new(31., 18.));
    let br = doc.add_atom("Br", Point::new(-31., 18.));
    doc.add_bond(c, f, 1, "wedge");
    doc.add_bond(c, cl, 1, "plain");
    doc.add_bond(c, br, 1, "plain");
    app.tab.selected = vec![c];
    show(&mut app, Display::ThreeD);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    let _ = app.update(Message::Models3d(Action::Flatten));
    assert!(!app.error, "{}", app.status);
    // A model is dropped when its molecule's perceived stereo changes.
    assert_eq!(displays(&app.tab.doc), [Display::TwoD]);
}

#[test]
fn the_2d_buttons_follow_the_chosen_layout() {
    let (mut app, atoms) = with_model(Display::Both);
    let original = app.tab.doc.clone();
    assert_eq!(app.tab.doc.models[0].layout, Layout::Original);
    assert!(matches!(
        app.show_as(Display::TwoD),
        Action::Show(Display::TwoD)
    ));
    let start = frames(&app);
    let _ = app.update(Message::Models3d(Action::Layout(Layout::View)));
    assert_eq!(frames(&app), start + 1);
    assert!(matches!(app.show_as(Display::TwoD), Action::Flatten));
    assert!(matches!(
        app.show_as(Display::ThreeD),
        Action::Show(Display::ThreeD)
    ));
    let drawn = app.tab.doc.atoms.clone();
    let _ = app.update(Message::Canvas(Edit::ShowAs(Display::TwoD)));
    assert_eq!(displays(&app.tab.doc), [Display::TwoD]);
    assert_ne!(app.tab.doc.atoms, drawn, "flattened from the view");
    // The choice and the drawn layout are the model's, and saved with it.
    let flat = app.tab.doc.atoms.clone();
    let read = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(read.models, app.tab.doc.models);
    assert_eq!(read.models[0].layout, Layout::View);
    // Without a model, 2D is the drawing whatever the layout.
    app.tab.selected = vec![app.tab.doc.atoms[3].id];
    assert!(matches!(
        app.show_as(Display::TwoD),
        Action::Show(Display::TwoD)
    ));

    // Original drawing brings back the layout as drawn, with its depth.
    app.tab.selected = atoms;
    show(&mut app, Display::ThreeD);
    assert_eq!(app.tab.doc.atoms, flat, "3D keeps the flattened drawing");
    let _ = app.update(Message::Models3d(Action::Layout(Layout::Original)));
    assert_eq!(displays(&app.tab.doc), [Display::ThreeD]);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(app.tab.doc.depth_appearance, original.depth_appearance);
    assert_eq!(app.tab.doc.models[0].drawn, None);
    assert_eq!(app.status, "Original 2D drawing restored");
    let _ = app.update(Message::Canvas(Edit::ShowAs(Display::TwoD)));
    assert_eq!(displays(&app.tab.doc), [Display::TwoD]);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    // So does Show as 2D drawing on a flattened 2D molecule.
    let _ = app.update(Message::Models3d(Action::Flatten));
    assert_ne!(app.tab.doc.atoms, original.atoms);
    show(&mut app, Display::TwoD);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.models[0].layout, Layout::Original);
}

#[test]
fn the_original_drawing_choice_and_the_2d_atoms_never_disagree() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let drawn = app.tab.doc.atoms.clone();
    let _ = app.update(Message::Models3d(Action::Layout(Layout::View)));
    let _ = app.update(Message::Canvas(Edit::ShowAs(Display::TwoD)));
    let flat = app.tab.doc.atoms.clone();
    assert_ne!(flat, drawn);
    show(&mut app, Display::ThreeD);
    // Choosing Original drawing brings the drawing back in that one step, so
    // Undo returns to From the 3D view with the flattened atoms together.
    let start = frames(&app);
    let _ = app.update(Message::Models3d(Action::Layout(Layout::Original)));
    assert_eq!(frames(&app), start + 1);
    assert_eq!(app.tab.doc.atoms, drawn);
    show(&mut app, Display::TwoD);
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    let model = &app.tab.doc.models[0];
    assert_eq!(
        (model.display, model.layout),
        (Display::ThreeD, Layout::View)
    );
    assert_eq!(app.tab.doc.atoms, flat);
    let _ = app.update(Message::Redo);
    let model = &app.tab.doc.models[0];
    assert_eq!(model.layout, Layout::Original);
    assert_eq!(model.drawn, None);
    assert_eq!(app.tab.doc.atoms, drawn, "Both draws the drawn layout");
    // With nothing flattened, the choice alone is the step.
    app.tab.selected = atoms;
    let _ = app.update(Message::Models3d(Action::Layout(Layout::View)));
    let _ = app.update(Message::Models3d(Action::Layout(Layout::Original)));
    assert_eq!(app.tab.doc.atoms, drawn);
    let start = frames(&app);
    let _ = app.update(Message::Models3d(Action::Layout(Layout::Original)));
    assert_eq!(frames(&app), start, "choosing it again changes nothing");
}

#[test]
fn showing_the_current_display_again_changes_nothing() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let (lo, _) = reshiki::models3d::frame(&app.tab.doc, &app.tab.doc.models[0]).unwrap();
    app.edit(Edit::ResizeModels {
        ids: atoms,
        pivot: lo,
        scale: 2.,
    });
    let resized = app.tab.doc.clone();
    let start = frames(&app);
    show(&mut app, Display::ThreeD);
    let _ = app.update(Message::Canvas(Edit::ShowAs(Display::ThreeD)));
    assert_eq!(app.tab.doc, resized, "the resized frame stays");
    assert_eq!(frames(&app), start);
    // Nor does 2D on a drawing that was never flattened, whatever the layout.
    show(&mut app, Display::TwoD);
    let drawn = app.tab.doc.clone();
    show(&mut app, Display::TwoD);
    assert_eq!(app.tab.doc, drawn);
    assert_eq!(frames(&app), start + 1);
}

#[test]
fn the_model_section_shows_only_while_the_model_does() {
    let (mut app, _) = with_model(Display::Both);
    assert!(app.model_section().is_some());
    show(&mut app, Display::TwoD);
    assert!(app.model_section().is_none());
    // Without a selected molecule nothing changes.
    app.tab.selected.clear();
    let start = frames(&app);
    set(&mut app, Setting::Style(Style::Wireframe));
    assert_eq!(frames(&app), start);
    assert_eq!(app.tab.doc.models[0].style, Style::BallStick);
}

#[tokio::test]
#[ignore = "Opt-in real renderer 3D model section controls"]
async fn model_section_controls_change_the_model() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::Operation, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let (app, _) = with_model(Display::ThreeD);
    for (id, setting) in [
        (
            "model-style-space-filling",
            Setting::Style(Style::SpaceFilling),
        ),
        ("model-look-glossy", Setting::Look(Look::Glossy)),
        (
            "model-hydrogens-Polar",
            Setting::Hydrogens(Hydrogens::Polar),
        ),
        (
            "model-background-card",
            Setting::Background(Background::Card),
        ),
        ("model-reset-view", Setting::ResetView),
    ] {
        let section = app.model_section().ok_or("No 3D model section")?;
        let mut ui = iced_runtime::UserInterface::build(
            section,
            iced::Size::new(288., 600.),
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
        ui.operate(&renderer, &mut operation::black_box(&mut activate));
        match activate.finish() {
            operation::Outcome::Some(Message::Models3d(Action::Set(chosen))) => {
                assert_eq!(chosen, setting)
            }
            _ => return Err(format!("{id} did not change the model")),
        }
    }
    Ok(())
}

#[test]
fn the_bar_shows_above_a_whole_selected_molecule_and_the_morph_starts_from_2d() {
    let (mut app, atoms, water) = drawing();
    assert_eq!(app.model_bar(), None, "one atom is not the molecule");
    app.tab.selected = atoms.clone();
    assert_eq!(app.model_bar(), Some(Display::TwoD));
    app.tool = crate::canvas::Tool::Bond(1);
    assert_eq!(app.model_bar(), None);
    app.tool = crate::canvas::Tool::Select;
    app.tab.selected = vec![atoms[0], water];
    assert_eq!(app.model_bar(), None);
    // Nor with any other object in the selection.
    let text = app.tab.doc.next_id();
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: text,
        position: Point::new(0., 80.),
        text: "Note".into(),
        format: Default::default(),
    });
    app.tab.selected = [atoms.clone(), vec![text]].concat();
    assert_eq!(app.model_bar(), None);

    app.tab.selected = atoms.clone();
    let _ = app.update(Message::Canvas(Edit::ShowAs(Display::ThreeD)));
    assert_eq!(app.model_bar(), Some(Display::ThreeD), "pending");
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    let (id, started) = app.model_morph().expect("2D → 3D morphs");
    assert_eq!(id, atoms[0]);
    app.tab.model_morph = None;
    show(&mut app, Display::Both);
    assert_eq!(app.model_morph(), None, "3D → Both does not");
    show(&mut app, Display::TwoD);
    show(&mut app, Display::Both);
    assert!(app.model_morph().is_some_and(|(_, at)| at > started));
    // Undo or Redo ends it, so an earlier model never plays it.
    let _ = app.update(Message::Undo);
    assert_eq!(app.model_morph(), None);
    show(&mut app, Display::Both);
    assert!(app.model_morph().is_some());
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.model_morph(), None);
}

#[test]
fn every_style_flattens_from_the_3d_view() {
    for style in Style::ALL {
        let (mut app, _) = with_model(Display::ThreeD);
        set(&mut app, Setting::Style(style));
        let drawn = app.tab.doc.atoms.clone();
        let _ = app.update(Message::Models3d(Action::Flatten));
        assert!(!app.error, "{style:?}: {}", app.status);
        assert_eq!(displays(&app.tab.doc), [Display::TwoD], "{style:?}");
        assert_ne!(app.tab.doc.atoms, drawn, "{style:?}");
        assert_eq!(app.tab.doc.models[0].style, style);
    }
}

#[test]
fn the_structure_note_names_the_current_style() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let note = |app: &App| subtitle(app.pending(&atoms), app.tab.doc.models.first());
    assert_eq!(
        note(&app),
        "Ball & stick · Chemistry and atom IDs stay the same"
    );
    for (style, label) in [
        (Style::Stick, "Stick"),
        (Style::Wireframe, "Wireframe"),
        (Style::SpaceFilling, "Space-filling"),
    ] {
        set(&mut app, Setting::Style(style));
        assert_eq!(
            note(&app),
            format!("{label} · Chemistry and atom IDs stay the same")
        );
    }
    assert!(app.structure_section().is_some());
}
