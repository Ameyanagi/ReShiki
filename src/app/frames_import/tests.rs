//! The import sheet for files of several frames: detection, the choice,
//! and the ensemble kept as a conformer result.
use super::*;
use reshiki::qcdata::Task as QcTask;
use std::path::Path;

/// Three water frames; the second is the lowest in energy (CREST style).
const CREST: &str = "3\n -5.06901\nO 0 0 0\nH 0.97 0 0\nH -0.30 0.90 0\n\
                     3\n -5.070176\nO 0 0 0\nH 0.96 0 0\nH -0.24 0.93 0\n\
                     3\n -5.065176\nO 0 0 0\nH 0.95 0.1 0\nH -0.20 0.95 0.1\n";

fn open(app: &mut App, name: &str, text: &str) {
    let path = PathBuf::from(name);
    let file = detect(&path, text.as_bytes()).expect("a file of several frames");
    let _ = app.file_prepared(Some((path, Ok(Prepared::Frames(Box::new(file))))));
}

fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::FramesImport(action));
}

fn model(app: &App) -> Vec<[f32; 3]> {
    let model = &app.tab.doc.models[0];
    model
        .atoms
        .iter()
        .map(|a| a.xyz)
        .chain(model.hydrogens.iter().map(|h| h.xyz))
        .collect()
}

async fn open_calculation(app: &mut App, fixture: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/qc")
        .join(fixture);
    let _ = app.update(Message::FilePrepared(super::super::files::read(path).await));
    assert!(!app.error, "{}", app.status);
}

async fn import_calculation(app: &mut App) {
    use iced::futures::StreamExt;

    let task = app.update(Message::FramesImport(Action::Import));
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action
            && matches!(&message, Message::Tab(_, inner) if matches!(inner.as_ref(), Message::FilePrepared(_)))
        {
            let _ = app.update(message);
            assert!(!app.error, "{}", app.status);
            return;
        }
    }
    panic!("missing chosen calculation");
}

#[tokio::test]
async fn gaussian_job_rows_choose_the_molecule_and_its_results() {
    use crate::app::xtb::{View, headline, method_line, results_title};
    use reshiki::models3d::Display;

    let (mut app, _) = App::new();
    open_calculation(&mut app, "pymatgen/CH2O_CO2.log").await;
    let dialog = app.frames_import.as_ref().unwrap();
    assert_eq!(
        dialog.file.subtitle(),
        "CH2O_CO2.log · Gaussian 09 · 3 jobs"
    );
    assert_eq!(dialog.index(), 2);
    assert!(dialog.add_2d && !dialog.keep);
    let rows: Vec<_> = dialog.file.file.jobs.iter().map(job_cells).collect();
    assert_eq!(
        rows.iter().map(|row| row[1].as_str()).collect::<Vec<_>>(),
        ["CH₂O", "CO₂", "CO₂"]
    );
    assert_eq!(
        rows[0],
        [
            "1",
            "CH₂O",
            "Frequencies · RB3LYP/6-31G",
            "Normal termination",
            "−114.461136600"
        ]
    );
    assert!(rows[1][2].starts_with("Optimization"));
    assert!(rows[2][2].starts_with("Frequencies"));
    assert_eq!(dialog.preview.as_ref().unwrap().atoms.len(), 3);

    send(&mut app, Action::Move(-1));
    assert_eq!(app.frames_import.as_ref().unwrap().index(), 1);
    send(&mut app, Action::Choose(Choice::Frame(0)));
    let preview = app
        .frames_import
        .as_ref()
        .unwrap()
        .preview
        .as_ref()
        .unwrap();
    assert_eq!(preview.atoms.len(), 4);
    import_calculation(&mut app).await;
    let doc = &app.tab.doc;
    assert_eq!(doc.models[0].display, Display::Both);
    assert_eq!(doc.atoms.len(), 4);
    assert_eq!(doc.bonds.len(), 3);
    assert!(doc.atoms.iter().all(|atom| atom.depth.abs() < 1e-5));
    let [stored] = doc.science.results.as_slice() else {
        panic!("{}", app.status);
    };
    let frame = doc.science.frame(&stored.result.frame).unwrap();
    assert_eq!(frame.numbers.len(), 4);
    assert_eq!(
        model(&app),
        frame
            .positions_angstrom
            .iter()
            .map(|p| p.map(|v| v as f32))
            .collect::<Vec<_>>()
    );
    assert_eq!(stored.result.energy_hartree, Some(-114.4611366));
    assert_eq!(
        stored
            .result
            .vibrations
            .as_ref()
            .unwrap()
            .frequencies_cm1
            .len(),
        6
    );
    let views = View::available(&stored.result);
    assert!(views.contains(&View::Vibrations) && views.contains(&View::Orbitals));
    assert_eq!(
        method_line(&stored.result, frame),
        "Gaussian 09 · RB3LYP/6-31G · from CH2O_CO2.log"
    );
    // The headline names the program and file too, never just "imported".
    assert_eq!(
        headline(&stored.result, frame),
        "Frequencies · Gaussian 09 · RB3LYP/6-31G · from CH2O_CO2.log · neutral singlet"
    );
    assert_eq!(
        results_title(Some(&stored.result)),
        "Results · CH2O_CO2.log"
    );
    assert!(app.xtb_bottom_palette().is_some());
    assert!(
        app.status
            .starts_with("Job 1 of 3 opened · CH₂O · Frequencies · results in the palette"),
        "{}",
        app.status
    );

    open_calculation(&mut app, "pymatgen/CH2O_CO2.log").await;
    send(&mut app, Action::Add2d(false));
    import_calculation(&mut app).await;
    assert_eq!(app.tab.doc.models[0].display, Display::ThreeD);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert!(app.tab.doc.science.results[0].result.vibrations.is_some());
    assert!(app.xtb_bottom_palette().is_some());
    assert!(
        app.status
            .starts_with("Job 3 of 3 opened · CO₂ · Frequencies · results in the palette"),
        "{}",
        app.status
    );
}

#[tokio::test]
async fn gaussian_optimization_opens_at_the_chosen_step() {
    let (mut app, _) = App::new();
    open_calculation(&mut app, "cclib/g16_dvb_gopt.out").await;
    let dialog = app.frames_import.as_ref().unwrap();
    assert_eq!(
        dialog.file.subtitle(),
        "g16_dvb_gopt.out · Gaussian 16 · 6 steps"
    );
    assert_eq!(dialog.file.layout(), Some(Layout::Line));
    assert_eq!(dialog.index(), 5);
    assert!(!dialog.keep);
    let steps = dialog.file.steps();
    let last = &steps[5];
    assert_eq!(eh(dialog.file.step_energy(last), true), "−382.308266602");
    let (input, final_frame) = (steps[0].frame.hash(), last.frame.hash());
    import_calculation(&mut app).await;
    assert_eq!(model(&app)[0], [0.269445, 1.410118, 0.]);
    let science = &app.tab.doc.science;
    let [stored] = science.results.as_slice() else {
        panic!("{}", app.status);
    };
    assert_eq!(stored.result.input_frame, Some(input));
    assert!(science.frame(&input).is_some());
    assert_eq!(stored.result.frame, final_frame);
    assert_eq!(stored.result.energy_hartree, Some(-382.3082666));
    assert!(stored.result.orbitals.is_some());
    assert!(app.xtb_bottom_palette().is_some());

    open_calculation(&mut app, "cclib/g16_dvb_gopt.out").await;
    send(&mut app, Action::Choose(Choice::First));
    import_calculation(&mut app).await;
    assert_eq!(model(&app)[0], [1.205666, 0.676891, 0.]);
    assert!(app.tab.doc.science.results.is_empty());
    assert!(
        app.status.contains("Reopen and choose step 6"),
        "{}",
        app.status
    );
}

#[tokio::test]
async fn geometry_only_molden_steps_open_without_calculation_results() {
    let (mut app, _) = App::new();
    let molden = b"[Molden Format]\n[GEOMETRIES] XYZ\n\
                   2\nfirst\nH 0 0 0\nH 0 0 0.74\n\
                   2\nlast\nH 0 0 0\nH 0 0 0.75\n";
    for (choice, distance) in [(Choice::First, 0.74), (Choice::Last, 0.75)] {
        let opened =
            super::super::files::prepare_contents("hydrogen.molden".into(), Ok(molden.to_vec()))
                .await;
        let _ = app.update(Message::FilePrepared(opened));
        let dialog = app.frames_import.as_ref().unwrap();
        assert_eq!(dialog.file.steps().len(), 2);
        assert_eq!(dialog.file.result_step_index(), None);

        send(&mut app, Action::Choose(choice));
        import_calculation(&mut app).await;
        assert_eq!(model(&app), [[0., 0., 0.], [0., 0., distance]]);
        assert!(app.tab.doc.science.is_empty());
        assert!(app.xtb_bottom_palette().is_none());
        assert!(
            app.status.contains("The file has no calculated results."),
            "{}",
            app.status
        );
        assert!(!app.status.contains("Reopen and choose"), "{}", app.status);
    }
}

#[tokio::test]
async fn interrupted_gaussian_optimization_selects_and_previews_its_result_step() {
    let text = include_str!("../../../tests/fixtures/qc/cclib/g16_dvb_gopt.out");
    // Use the same cut before the fourth geometry's SCF as the Gaussian reader test.
    let (truncated, _) = text
        .split_once(" SCF Done:  E(RB3LYP) =  -382.308265702")
        .unwrap();
    let (mut app, _) = App::new();
    let path = PathBuf::from("interrupted.log");
    let open = |contents: &str| {
        super::super::files::prepare_contents(path.clone(), Ok(contents.as_bytes().to_vec()))
    };
    let preview_xyz = |dialog: &Dialog| dialog.preview.as_ref().unwrap().models[0].atoms[0].xyz;

    let _ = app.update(Message::FilePrepared(open(truncated).await));
    let dialog = app.frames_import.as_ref().unwrap();
    let steps = dialog.file.steps();
    assert_eq!(steps.len(), 4);
    assert_eq!(dialog.choice, Choice::Frame(2));
    let result_step = dialog.file.result_step_index();
    assert_eq!(result_step, Some(2));
    assert!(step_comment(&steps[2], result_step == Some(2)).starts_with("Results"));
    assert!(!step_comment(&steps[3], result_step == Some(3)).starts_with("Results"));
    let bound = steps[2].frame.hash();
    let evaluated = steps[2].frame.positions_angstrom[0].map(|x| x as f32);
    let unevaluated = steps[3].frame.positions_angstrom[0].map(|x| x as f32);
    assert_ne!(evaluated, unevaluated);
    assert_eq!(preview_xyz(dialog), evaluated);
    import_calculation(&mut app).await;
    assert_eq!(model(&app)[0], evaluated);
    assert_eq!(app.tab.doc.science.results[0].result.frame, bound);

    let _ = app.update(Message::FilePrepared(open(truncated).await));
    send(&mut app, Action::Choose(Choice::Last));
    assert_eq!(
        preview_xyz(app.frames_import.as_ref().unwrap()),
        unevaluated
    );
    import_calculation(&mut app).await;
    assert_eq!(model(&app)[0], unevaluated);
    assert!(app.tab.doc.science.results.is_empty());
    assert!(
        app.status.contains("Reopen and choose step 3"),
        "{}",
        app.status
    );

    let water = include_str!("../../../tests/fixtures/qc/cclib/g16_water_mp2.log");
    let linked = format!("{water}\n{truncated}");
    let _ = app.update(Message::FilePrepared(open(&linked).await));
    send(&mut app, Action::Choose(Choice::Frame(1)));
    let dialog = app.frames_import.as_ref().unwrap();
    assert!(dialog.file.job_rows());
    assert_eq!(preview_xyz(dialog), evaluated);
    import_calculation(&mut app).await;
    assert_eq!(model(&app)[0], evaluated);
    assert_eq!(app.tab.doc.science.results[0].result.frame, bound);
}

#[test]
fn only_several_frames_of_one_molecule_ask() {
    assert!(detect(Path::new("w.xyz"), CREST.as_bytes()).is_some());
    let one = CREST.split_inclusive('\n').take(5).collect::<String>();
    assert!(detect(Path::new("w.xyz"), one.as_bytes()).is_none());
    let mixed = format!("{CREST}2\n\nH 0 0 0\nH 0.74 0 0\n");
    assert!(detect(Path::new("w.xyz"), mixed.as_bytes()).is_none());
    // Other formats open as before.
    assert!(detect(Path::new("w.cif"), CREST.as_bytes()).is_none());
}

#[test]
fn the_lowest_frame_opens_with_the_ensemble_kept() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    open(&mut app, "crest_conformers.xyz", CREST);
    let dialog = app.frames_import.as_ref().expect("the sheet asks");
    // Not sorted by energy: an ordered trajectory, which opens its last frame.
    assert_eq!(dialog.file.layout(), Some(Layout::Line));
    assert_eq!(dialog.choice, Choice::Last);
    assert!(!dialog.keep);
    send(&mut app, Action::Choose(Choice::Lowest));
    let dialog = app.frames_import.as_ref().unwrap();
    assert_eq!(dialog.index(), 1);

    send(&mut app, Action::Keep(true));
    send(&mut app, Action::Import);
    assert!(app.frames_import.is_none());
    assert_eq!(
        model(&app),
        [[0., 0., 0.], [0.96, 0., 0.], [-0.24, 0.93, 0.]],
        "the 3D model is the lowest frame: {}",
        app.status
    );
    let stored = &app.tab.doc.science.results;
    assert_eq!(stored.len(), 1, "{}", app.status);
    let result = &stored[0].result;
    assert_eq!(result.task, QcTask::Conformers);
    let path = result.trajectory.as_ref().unwrap();
    assert_eq!(path.energies_hartree, [-5.070176, -5.06901, -5.065176]);
    assert!(matches!(
        &result.provenance.source,
        Source::Imported { file_name, .. } if file_name == "crest_conformers.xyz"
    ));
    assert!(
        app.status
            .contains("Frame 2 of 3 opened · 3 kept as an ensemble"),
        "{}",
        app.status
    );
}

#[test]
fn picking_moves_and_frames_without_energies_cannot_be_kept() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let plain = CREST
        .replace(" -5.06901", "first")
        .replace(" -5.070176", "second")
        .replace(" -5.065176", "third");
    open(&mut app, "md.xyz", &plain);
    let dialog = app.frames_import.as_ref().unwrap();
    // Without energies the last frame of a trajectory opens.
    assert_eq!(dialog.choice, Choice::Last);
    send(&mut app, Action::Choose(Choice::Lowest));
    send(&mut app, Action::Keep(true));
    let dialog = app.frames_import.as_ref().unwrap();
    assert_eq!((dialog.choice, dialog.keep), (Choice::Last, false));

    send(&mut app, Action::Move(-1));
    assert_eq!(app.frames_import.as_ref().unwrap().index(), 1);
    send(&mut app, Action::Move(-5));
    assert_eq!(app.frames_import.as_ref().unwrap().index(), 0);
    send(&mut app, Action::Import);
    assert_eq!(model(&app)[1], [0.97, 0., 0.]);
    assert!(app.tab.doc.science.results.is_empty());

    open(&mut app, "md.xyz", &plain);
    send(&mut app, Action::Close);
    assert!(app.frames_import.is_none());
}

#[test]
fn records_split_on_their_delimiters() {
    let text = "a\nM  END\n$$$$\n\nb\nM  END\n$$$$\nc\nM  END\n";
    assert_eq!(record(text, 0).as_deref(), Some("a\nM  END\n$$$$\n"));
    assert_eq!(record(text, 1).as_deref(), Some("\nb\nM  END\n$$$$\n"));
    assert_eq!(record(text, 2).as_deref(), Some("c\nM  END\n"));
    assert_eq!(record(text, 3), None);
}

#[test]
fn an_sd_file_asks_only_for_3d_records_of_one_molecule() {
    let ethanol = include_str!("../../../tests/fixtures/ethanol.mol");
    let flat = format!("{ethanol}$$$$\n{ethanol}$$$$\n");
    assert!(detect(Path::new("two.sdf"), flat.as_bytes()).is_none());

    let water = |title: &str, z: f64, energy: f64| {
        format!(
            "{title}\n{:<20}3D\n\n  3  2  0  0  0  0  0  0  0  0999 V2000\n\
             \x20   0.0000    0.0000    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0\n\
             \x20   0.9600    0.0000 {z:>9.4} H   0  0  0  0  0  0  0  0  0  0  0  0\n\
             \x20  -0.2400    0.9300    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0\n\
             \x20 1  2  1  0\n  1  3  1  0\nM  END\n> <ENERGY_HARTREE>\n{energy}\n\n$$$$\n",
            "  ReShiki"
        )
    };
    // Titles as ReShiki's own SDF export numbers them.
    let conformers =
        water("water conformer 1", 0.1, -5.06) + &water("water conformer 2", -0.1, -5.07);
    let file = detect(Path::new("w.sdf"), conformers.as_bytes()).expect("conformers ask");
    // ReShiki's own SDF export carries the energies back.
    assert_eq!(file.energies(), Some(vec![-5.06, -5.07]));
    let (mut app, _) = App::new();
    open(&mut app, "w.sdf", &conformers);
    let dialog = app.frames_import.as_ref().unwrap();
    assert_eq!(dialog.file.layout(), Some(Layout::Levels));
    assert_eq!(dialog.choice, Choice::Lowest);
    assert!(!dialog.keep);
    send(&mut app, Action::Keep(true));
    assert!(app.frames_import.as_ref().unwrap().keep);

    let library = water("water", 0.1, -5.06) + &water("deuterated?", -0.1, -5.07);
    assert!(detect(Path::new("w.sdf"), library.as_bytes()).is_none());
    let numbered = water("compound 101", 0.1, -5.06) + &water("compound 102", -0.1, -5.07);
    assert!(detect(Path::new("w.sdf"), numbered.as_bytes()).is_none());
    // A record of another calculated state is not a conformer of the first.
    let cation = water("water conformer 2", -0.1, -5.07).replace(
        "\n\n$$$$",
        "\n\n> <TOTAL_CHARGE>\n1\n\n> <MULTIPLICITY>\n2\n\n$$$$",
    );
    let states = water("water conformer 1", 0.1, -5.06) + &cation;
    assert!(detect(Path::new("w.sdf"), states.as_bytes()).is_none());
}

#[test]
fn files_that_are_not_one_molecules_frames_open_as_before() {
    // A frame number is not an energy, and a cell keeps the crystal path.
    let numbered = CREST
        .replace(" -5.06901", "1")
        .replace(" -5.070176", "2")
        .replace(" -5.065176", "3");
    let file = detect(Path::new("md.xyz"), numbered.as_bytes()).unwrap();
    assert!(file.energies().is_none());
    let exponents = CREST
        .replace(" -5.06901", "1.00e0")
        .replace(" -5.070176", "2.00e0")
        .replace(" -5.065176", "3.00e0");
    let file = detect(Path::new("md.xyz"), exponents.as_bytes()).unwrap();
    assert!(file.energies().is_none());
    let periodic = CREST.replace(
        " -5.06901",
        "Lattice=\"10 0 0 0 10 0 0 0 10\" pbc=\"T T T\"",
    );
    assert!(detect(Path::new("cell.extxyz"), periodic.as_bytes()).is_none());
    // Different charges are different species, not frames of one.
    let ions = CREST.replace(" -5.06901", "charge=1 multiplicity=2");
    assert!(detect(Path::new("ions.extxyz"), ions.as_bytes()).is_none());
    // `.sd` keeps its own route.
    assert!(detect(Path::new("w.sd"), CREST.as_bytes()).is_none());
}

#[test]
fn the_sheets_keys_leave_return_to_the_focused_control() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    let enter = Key::Named(Named::Enter);
    assert!(modal_key(&enter, Modifiers::empty()).is_none());
    assert!(matches!(
        modal_key(&enter, Modifiers::COMMAND),
        Some(Message::FramesImport(Action::Import))
    ));
    assert!(matches!(
        modal_key(&Key::Named(Named::Escape), Modifiers::empty()),
        Some(Message::FramesImport(Action::Close))
    ));
}

#[cfg(debug_assertions)]
#[test]
fn import_button_fits_and_can_be_clicked_in_the_audit_windows() {
    use iced::advanced::{Layout as WidgetLayout, clipboard, layout, widget::Tree};
    use iced::widget::{Space, button};
    use iced::{Event, Rectangle, Size};

    for window in [Size::new(1440., 900.), Size::new(1280., 800.)] {
        // The modal caps the panel to its configured size and pads both the panel and window.
        let size = Size::new(
            SHEET_WIDTH.min(window.width - 48.) - 48.,
            SHEET_HEIGHT.min(window.height - 48.) - 48.,
        );
        let body = Space::new().width(Length::Fill).height(1000.);
        let footer = row![
            Space::new().width(Length::Fill),
            button(Space::new().width(60.).height(20.))
                .on_press(Message::FramesImport(Action::Close)),
            button(Space::new().width(60.).height(20.))
                .on_press(Message::FramesImport(Action::Import)),
        ]
        .spacing(10);
        let mut view = sheet_content::<()>(body.into(), footer.into());
        let mut tree = Tree::new(view.as_widget());
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &(), &layout::Limits::new(Size::ZERO, size));
        let layout = WidgetLayout::new(&node);
        let column = layout.children().next().unwrap();
        let footer = column.children().nth(1).unwrap();
        let import = footer.children().nth(2).unwrap().bounds();
        assert!(
            import.width >= 60. && import.height >= 20.,
            "{window:?}: {import:?}"
        );
        assert!(
            Rectangle::with_size(size).contains(import.center()),
            "{window:?}: {import:?}"
        );
        assert!(
            import.x + import.width <= size.width && import.y + import.height <= size.height,
            "{window:?}: {import:?}"
        );
        let mut ui = iced_runtime::UserInterface::build(
            view,
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut (),
        );
        let mut messages = Vec::new();
        let _ = ui.update(
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(import.center()),
            &mut (),
            &mut clipboard::Null,
            &mut messages,
        );
        assert!(
            matches!(messages.as_slice(), [Message::FramesImport(Action::Import)]),
            "{window:?}"
        );
    }
}

#[test]
fn reshikis_own_xyz_export_reopens_with_energies_charge_and_spin() {
    // As Export XYZ… writes a water cation's conformers.
    let frame = |rank: usize, e: f64, y: f64| {
        format!(
            "3\nwater conformer {rank} · ΔE 0.00 kcal/mol · 50.0 % · cluster 2 · charge=1 mult=2 · E = {e:.10} Eh\n\
             O 0 0 0\nH 0.97 0 0\nH -0.30 {y} 0\n"
        )
    };
    let text = frame(1, -5.0, 0.90) + &frame(2, -4.99, 0.95);
    let file = detect(Path::new("water conformers.xyz"), text.as_bytes()).unwrap();
    assert_eq!(file.energies(), Some(vec![-5.0, -4.99]));
    let first = &file.steps()[0].frame;
    assert_eq!((first.charge, first.multiplicity), (1, 2));
    let dialog = Dialog::new("water conformers.xyz".into(), file);
    assert!(dialog.keep);
}

#[test]
fn a_sorted_file_is_an_ensemble_with_a_turnable_preview_and_the_lowest_overlaid() {
    // CREST writes conformers lowest first.
    let sorted = "3\n -5.070176\nO 0 0 0\nH 0.96 0 0\nH -0.24 0.93 0\n\
                  3\n -5.06901\nO 0 0 0\nH 0.97 0 0\nH -0.30 0.90 0\n\
                  3\n -5.065176\nO 0 0 0\nH 0.95 0.1 0\nH -0.20 0.95 0.1\n";
    let (mut app, _) = App::new();
    app.tab.busy = false;
    open(&mut app, "crest_conformers.xyz", sorted);
    let dialog = app.frames_import.as_ref().unwrap();
    assert_eq!(dialog.file.layout(), Some(Layout::Levels));
    assert_eq!((dialog.choice, dialog.index()), (Choice::Lowest, 0));
    assert!(dialog.keep);
    let preview = |app: &App| app.frames_import.as_ref().unwrap().preview.clone().unwrap();
    assert!(preview(&app).models[0].overlay.is_empty());

    // Picking frame 3 with the overlay shows the lowest behind it.
    send(&mut app, Action::Overlay(true));
    send(&mut app, Action::Choose(Choice::Frame(2)));
    let shown = preview(&app);
    assert_eq!(shown.models[0].overlay.len(), 1);
    let turned = shown.models[0].rotation;
    send(&mut app, Action::Turn(30., 0.));
    assert_ne!(preview(&app).models[0].rotation, turned);
    // The turn stays when another frame is picked.
    let after = preview(&app).models[0].rotation;
    send(&mut app, Action::Move(-1));
    assert_eq!(preview(&app).models[0].rotation, after);

    send(&mut app, Action::Import);
    use crate::app::xtb::{Action as XtbAction, palette::Action as PaletteAction};
    let _ = app.update(Message::Xtb(XtbAction::Palette(PaletteAction::Show)));
    let result = &app.tab.doc.science.results[0].result;
    assert_eq!(
        crate::app::xtb::results_title(Some(result)),
        "Results · crest_conformers.xyz"
    );
    assert_eq!(crate::app::xtb::results_title(None), "xTB results");
    let _ = app.update(Message::Xtb(XtbAction::Palette(PaletteAction::Step(1))));
    assert!(app.status.contains("Conformer 2 of 3"), "{}", app.status);
    let _ = app.update(Message::Xtb(XtbAction::Palette(PaletteAction::StepBack)));
    assert_eq!(app.status, "Showing the 3D model");
}

#[test]
fn files_opened_together_ask_in_turn() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    for name in ["a.xyz", "b.xyz", "c.xyz"] {
        open(&mut app, name, CREST);
    }
    let path = |app: &App| app.frames_import.as_ref().map(|d| d.path.clone());
    assert_eq!(path(&app), Some(PathBuf::from("a.xyz")));
    // Each answer asks about the next file once it is done (the runtime
    // sends Next after the chosen frame has opened).
    send(&mut app, Action::Close);
    assert!(app.frames_import.is_none());
    // A second click landing before Next leaves the waiting files alone.
    send(&mut app, Action::Import);
    send(&mut app, Action::Next);
    assert_eq!(path(&app), Some(PathBuf::from("b.xyz")));
    send(&mut app, Action::Import);
    assert_eq!(model(&app).len(), 3, "b opened");
    // c, opened while b's sheet was up, waits its turn too.
    send(&mut app, Action::Next);
    assert_eq!(path(&app), Some(PathBuf::from("c.xyz")));
    send(&mut app, Action::Close);
    send(&mut app, Action::Next);
    assert!(app.frames_import.is_none());
}
