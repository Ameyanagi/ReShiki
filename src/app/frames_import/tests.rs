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
    send(&mut app, Action::Close);
    assert_eq!(path(&app), Some(PathBuf::from("b.xyz")));
    send(&mut app, Action::Import);
    assert_eq!(model(&app).len(), 3, "b opened");
    assert_eq!(path(&app), Some(PathBuf::from("c.xyz")));
    send(&mut app, Action::Close);
    assert!(app.frames_import.is_none());
}
