use super::*;
use crate::app::xtb;

fn app() -> App {
    let (mut app, _) = App::new();
    app.tool = Tool::Select;
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    app
}

#[test]
fn xtb_context_row_ends_in_one_3d_model_menu_for_any_selection() {
    if !xtb::available() {
        return;
    }
    let mut app = app();
    for selected in [vec![], vec![app.tab.doc.atoms[0].id], app.tab.doc.all_ids()] {
        app.tab.selected = selected;
        let commands = app.context_commands();
        let last = commands.last().unwrap();
        assert_eq!(last.label, "3D model ▾");
        assert_eq!(last.menu, "3D model menu");
        assert!(last.model_menu());
        assert!(last.enabled);
        assert_eq!(commands.iter().filter(|c| c.model_menu()).count(), 1);
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c.label, "Optimize with GFN2-xTB" | "3D optimize…"))
        );
    }
    app.tab.busy = true;
    assert!(!app.context_commands().last().unwrap().enabled);
    app.tab.busy = false;
    app.tab.doc = Default::default();
    app.tab.selected.clear();
    assert!(!app.context_commands().last().unwrap().enabled);
}

#[test]
fn non_xtb_context_row_keeps_the_plain_3d_optimize_action() {
    if xtb::available() {
        return;
    }
    let mut app = app();
    for selected in [vec![], app.tab.doc.all_ids()] {
        app.tab.selected = selected;
        let commands = app.context_commands();
        let last = commands.last().unwrap();
        assert_eq!((last.label, last.menu), ("3D optimize…", "3D optimize…"));
        assert!(matches!(
            last.message,
            Message::Optimization(super::super::optimization::Action::Begin)
        ));
        assert!(!commands.iter().any(RowCommand::model_menu));
        assert!(last.enabled);
    }
    app.tab.busy = true;
    assert!(!app.context_commands().last().unwrap().enabled);
    app.tab.busy = false;
    app.tab.doc = Default::default();
    assert!(!app.context_commands().last().unwrap().enabled);
}

#[test]
fn fitting_folds_the_3d_model_menu_as_one_trailing_command() {
    if !xtb::available() {
        return;
    }
    let app = app();
    let commands = app.context_commands();
    assert_eq!(
        commands.iter().map(|c| c.label).collect::<Vec<_>>(),
        ["Keyboard drawing", "3D model ▾",]
    );
    let widths: Vec<_> = commands
        .iter()
        .map(|c| text_width(c.label, 12.) + 18.)
        .collect();
    let row = Fold {
        name: (40., 40.),
        options: 0.,
        summary: 0.,
        commands: &widths,
        arrange: None,
    };
    let width = 40. + widths.iter().map(|w| w + CONTEXT_GAP).sum::<f32>();
    assert_eq!(fit(width, &row).folded, 0);
    assert_eq!(fit(width - 1., &row).folded, 1);
}

#[test]
fn model_menu_actions_keep_order_and_wait_for_busy_work() {
    use super::super::xtb::{Action, CalcTask, Entry};
    let mut app = app();
    let commands = app.model_commands();
    assert_eq!(
        commands.iter().map(|c| c.menu).collect::<Vec<_>>(),
        [
            "Optimize with xTB (quantum, GFN2)",
            "Opt + freq (xTB)",
            "Force-field 3D model (MMFF94s)…",
            "More xTB settings…",
        ]
    );
    assert!(matches!(
        commands[0].message,
        Message::Xtb(Action::Entry(Entry::OptimizeGfn2))
    ));
    assert!(matches!(
        commands[1].message,
        Message::Xtb(Action::Entry(Entry::Task(CalcTask::OptFreq)))
    ));
    assert!(matches!(
        commands[2].message,
        Message::Optimization(super::super::optimization::Action::Begin)
    ));
    assert!(matches!(
        commands[3].message,
        Message::ContextMenu(super::super::context_menu::Action::XtbSettings)
    ));
    assert!(commands.iter().all(|c| c.enabled));
    app.tab.busy = true;
    assert_eq!(
        app.model_commands()
            .iter()
            .map(|c| c.enabled)
            .collect::<Vec<_>>(),
        [false, false, false, true]
    );
    app.tab.busy = false;
    app.tab.doc = Default::default();
    assert_eq!(
        app.model_commands()
            .iter()
            .map(|c| c.enabled)
            .collect::<Vec<_>>(),
        [false, false, false, true]
    );
}

#[cfg(all(feature = "xtb", not(windows), not(target_arch = "wasm32")))]
#[test]
fn model_menu_xtb_actions_wait_for_an_existing_xtb_run() {
    use super::super::xtb::Action;
    let mut app = app();
    app.tab.doc = Default::default();
    let oxygen = app
        .tab
        .doc
        .add_atom("O", reshiki::document::Point::default());
    app.tab.selected = vec![oxygen];
    let _ = app.update(Message::Xtb(Action::Run));
    assert!(app.xtb_running());
    assert_eq!(
        app.model_commands()
            .iter()
            .map(|c| c.enabled)
            .collect::<Vec<_>>(),
        [false, false, true, true]
    );
}
