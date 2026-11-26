use super::super::xtb;
use super::*;
use iced::advanced::widget::{Id, operation};
use iced::futures::StreamExt;

fn model_entries(app: &App, page: Page) -> Vec<(&'static str, Message, bool)> {
    app.context_entries(page)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Item {
                label,
                action: Action::Run(message),
                enabled,
                ..
            } => Some((label, *message, enabled)),
            _ => None,
        })
        .collect()
}

#[test]
fn model_menu_separates_xtb_from_force_field_and_uses_the_shared_actions() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let entries = app.context_entries(Page::Model3d);
    assert_eq!(entries.len(), 5);
    assert!(matches!(entries[2], Entry::Separator));
    let entries = model_entries(&app, Page::Model3d);
    assert_eq!(
        entries.iter().map(|(label, ..)| *label).collect::<Vec<_>>(),
        [
            "Optimize with xTB (quantum, GFN2)",
            "Opt + freq (xTB)",
            "Force-field 3D model (MMFF94s)…",
            "More xTB settings…",
        ]
    );
    assert!(matches!(
        entries[0].1,
        Message::Xtb(xtb::Action::Entry(xtb::Entry::OptimizeGfn2))
    ));
    assert!(matches!(
        entries[1].1,
        Message::Xtb(xtb::Action::Entry(xtb::Entry::Task(xtb::CalcTask::OptFreq)))
    ));
    assert!(matches!(
        entries[2].1,
        Message::Optimization(super::super::optimization::Action::Begin)
    ));
    assert!(matches!(
        entries[3].1,
        Message::ContextMenu(Action::XtbSettings)
    ));
    assert!(entries.iter().all(|(_, _, enabled)| *enabled));
}

#[test]
fn model_menu_shows_rerun_keys_only_for_the_matching_task_and_method() {
    use reshiki::qcdata::XtbMethod;
    let (mut app, _) = App::new();
    let optimize = Action::Run(Box::new(Message::Xtb(xtb::Action::Entry(
        xtb::Entry::OptimizeGfn2,
    ))));
    let frequency = Action::Run(Box::new(Message::Xtb(xtb::Action::Entry(
        xtb::Entry::Task(xtb::CalcTask::OptFreq),
    ))));
    for task in [
        xtb::CalcTask::Energy,
        xtb::CalcTask::Optimize,
        xtb::CalcTask::OptFreq,
        xtb::CalcTask::Frequencies,
        xtb::CalcTask::Conformers,
    ] {
        for method in [XtbMethod::Gfn2Xtb, XtbMethod::GXtb, XtbMethod::GfnFf] {
            app.xtb.settings.task = task;
            app.xtb.settings.method = method;
            let expected = (task == xtb::CalcTask::Optimize && method == XtbMethod::Gfn2Xtb)
                .then(xtb::rerun_keys);
            assert_eq!(
                app.context_shortcut(&optimize),
                expected,
                "{task:?}, {method:?}"
            );
            assert_eq!(app.context_shortcut(&frequency), None);
        }
    }
}

#[test]
fn folded_model_menu_lists_four_ordinary_commands_in_place() {
    let (mut app, _) = App::new();
    app.tool = Tool::Select;
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    // A non-xTB build keeps the single existing overflow command.
    if !xtb::available() {
        let entries = model_entries(&app, Page::More(1));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "3D optimize…");
        return;
    }
    let entries = app.context_entries(Page::More(1));
    assert_eq!(entries.len(), 4);
    assert!(entries.iter().all(|entry| matches!(
        entry,
        Entry::Item {
            action: Action::Run(_),
            ..
        }
    )));
    let expected = model_entries(&app, Page::Model3d);
    let actual = model_entries(&app, Page::More(1));
    assert_eq!(
        actual.iter().map(|(label, ..)| *label).collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(label, ..)| *label)
            .collect::<Vec<_>>()
    );
    for ((_, a, ae), (_, b, be)) in actual.iter().zip(&expected) {
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        assert_eq!(ae, be);
    }
    let all = model_entries(&app, Page::More(2));
    assert_eq!(all[0].0, "Keyboard drawing");
    assert_eq!(all[1].0, "Optimize with xTB (quantum, GFN2)");
}

/// Apply the emitted widget operations to a focused control, as the runtime does.
async fn focus_changes(task: Task<Message>, id: &str) -> Vec<bool> {
    struct Focus(Vec<bool>);
    impl operation::Focusable for Focus {
        fn is_focused(&self) -> bool {
            self.0.last().copied().unwrap_or(false)
        }
        fn focus(&mut self) {
            self.0.push(true);
        }
        fn unfocus(&mut self) {
            self.0.push(false);
        }
    }
    let mut focus = Focus(Vec::new());
    let id = Id::from(id.to_owned());
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Widget(operation) = action {
                let mut current = Some(operation);
                while let Some(mut operation) = current.take() {
                    operation.focusable(Some(&id), iced::Rectangle::default(), &mut focus);
                    if let operation::Outcome::Chain(next) = operation.finish() {
                        current = Some(next);
                    }
                }
            }
        }
    }
    focus.0
}

#[tokio::test]
async fn model_menu_dismissals_and_settings_return_focus_to_drawing() {
    if !xtb::available() {
        return;
    }
    let (mut app, _) = App::new();
    app.tab.busy = false;
    for close in [
        Message::ContextMenu(Action::Close),
        Message::ContextMenu(Action::Key(Named::Escape)),
        Message::Escape,
        Message::ContextMenu(Action::Open(Page::Model3d, 240.)),
        Message::ContextMenu(Action::Run(Box::new(Message::Xtb(xtb::Action::Entry(
            xtb::Entry::OptimizeGfn2,
        ))))),
    ] {
        let _ = app.update(Message::ContextMenu(Action::Open(Page::Model3d, 240.)));
        let menu = app.context_menu.as_ref().unwrap();
        assert_eq!(menu.position, Point::new(240., 0.));
        assert!(menu.hit.is_none() && menu.at.is_none());
        let task = app.update(close);
        assert!(app.context_menu.is_none());
        assert_eq!(focus_changes(task, "context-3d-model").await, [false]);
    }
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Predict;
    app.xtb.page = xtb::Page::Nmr;
    let _ = app.update(Message::ContextMenu(Action::Open(Page::Model3d, 240.)));
    let task = app.update(Message::ContextMenu(Action::Activate(0, 4)));
    assert!(app.context_menu.is_none());
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert_eq!(app.xtb.page, xtb::Page::Xtb);
    assert_eq!(focus_changes(task, "context-3d-model").await, [false]);
}

#[tokio::test]
async fn folded_model_menu_dismissals_return_focus_to_drawing() {
    let (mut app, _) = App::new();
    app.tool = Tool::Select;
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let open = Action::Open(Page::More(1), 240.);
    for close in [
        Message::ContextMenu(Action::Close),
        Message::ContextMenu(Action::Key(Named::Escape)),
        Message::Escape,
        Message::ContextMenu(open.clone()),
    ] {
        let _ = app.update(Message::ContextMenu(open.clone()));
        let task = app.update(close);
        assert!(app.context_menu.is_none());
        let changes = focus_changes(task, "context-overflow").await;
        assert_eq!(changes.contains(&false), xtb::available());
    }
}

#[tokio::test]
async fn model_menu_keeps_focus_at_the_forwarded_shortcut_destination() {
    if !xtb::available() {
        return;
    }
    let (mut app, _) = App::new();
    app.tool = Tool::Select;
    app.tab.busy = false;
    for page in [Page::Model3d, Page::More(1)] {
        let _ = app.update(Message::ContextMenu(Action::Open(page, 240.)));
        let task = app.update(Message::ContextMenu(Action::Run(Box::new(
            Message::Inspector(InspectorTab::Import),
        ))));
        assert!(app.context_menu.is_none());
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Import);
        assert_eq!(focus_changes(task, app.import_input_id()).await, [true]);
    }
}
