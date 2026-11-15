//! Snapshots of the context row in keyboard drawing, the Check drawing
//! findings and the status bar at the minimum and default window sizes.
//! Opt in with `--ignored`; the PNGs go to `RESHIKI_UI_QA_DIR`.
use super::*;
use reshiki::keyboard_drawing::Target;

/// Two stereo findings: a reversed wedge and an undefined stereocentre.
fn findings(app: &mut App) {
    let mut doc = Document::default();
    for (x, display, reversed) in [(-60., "wedge", true), (60., "plain", false)] {
        let c = doc.add_atom("C", World::new(x, 0.));
        let f = doc.add_atom("F", World::new(x + 28., 0.));
        let cl = doc.add_atom("Cl", World::new(x - 14., -24.));
        let br = doc.add_atom("Br", World::new(x - 14., 24.));
        let (a, b) = if reversed { (f, c) } else { (c, f) };
        doc.add_bond(a, b, 1, display);
        doc.add_bond(c, cl, 1, "plain");
        doc.add_bond(c, br, 1, "plain");
    }
    doc.reconcile_molecule_groups();
    open(app, doc);
    app.tab.busy = false;
    app.checked_stereo(vec![]);
}

/// Keyboard drawing with nothing targeted: the commands that need an atom.
fn keyboard_blank(app: &mut App) {
    open(app, benzene());
    let center = app.tab.camera.center;
    app.tab
        .keyboard_drawing
        .set_target(Target::Blank(center), &app.tab.doc);
}

/// Keyboard drawing with one atom marked and another targeted.
fn keyboard_marked(app: &mut App) {
    open(app, benzene());
    let ids: Vec<_> = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    app.tab.keyboard_drawing.mark(ids[0]);
    app.tab
        .keyboard_drawing
        .set_target(Target::Atom(ids[2]), &app.tab.doc);
    app.tab.selected = vec![ids[2]];
}

/// The findings list open from the Check drawing button.
fn findings_open(app: &mut App) {
    findings(app);
    let _ = app.update(Message::Stereo(crate::app::stereo::Action::Results(true)));
}

/// A finding chosen in the list: its atoms are selected and pulse.
fn finding_pulse(app: &mut App) {
    findings_open(app);
    let _ = app.update(Message::Stereo(crate::app::stereo::Action::Select(1)));
}

const SCENES: &[(&str, Setup)] = &[
    ("keyboard-blank", keyboard_blank),
    ("keyboard-marked", keyboard_marked),
    ("check-findings", findings),
    ("check-findings-open", findings_open),
    ("check-finding-pulse", finding_pulse),
];

#[tokio::test]
#[ignore = "Opt-in GPU snapshots of the context row and status bar; set RESHIKI_UI_QA_DIR"]
async fn context_row_and_status_snapshots() {
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let directory = std::env::var_os("RESHIKI_UI_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-ui-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    for (width, height) in [(1280, 820), (1040, 680)] {
        for (name, setup) in SCENES {
            let (mut app, _) = App::new();
            app.appearance.mode = crate::appearance::Mode::Light;
            setup(&mut app);
            let output = directory.join(format!("{name}-{width}.png"));
            snapshot(
                &mut app,
                &mut renderer,
                Size::new(width as f32, height as f32),
                &output,
            );
            eprintln!("{}", output.display());
        }
    }
}
