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
    assert_eq!(dialog.choice, Choice::Lowest);
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
            .contains("Frame 2 of 3 opened · 3 frames kept as an ensemble"),
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
}
