//! Shared search for templates, chemical names and SMILES in the Add tab.
use super::engine_jobs::InsertPlacement;
use super::icons::{Glyph, Icon};
use super::naming;
use super::template_library::Action;
use super::workspace::{accessible_checkbox, accessible_command, control, muted_text};
use super::{App, InspectorTab, Job, Message};
use iced::advanced::widget::{Id, Operation, operation::Focusable};
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length, Rectangle, Task};
use reshiki::engine::Request;

const MAX_QUERY_BYTES: usize = 8192;
pub(super) const INPUT: &str = "add-search";
const INSERT: &str = "add-insert";

/// Release Add's insertion controls without disturbing a later focus owner.
/// Name resolution may finish after the user has moved into Help or a field.
pub(super) fn focus_drawing_after_insert() -> Task<Message> {
    struct ReleaseInsertFocus;
    impl Operation for ReleaseInsertFocus {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
            if state.is_focused()
                && id.is_some_and(|id| *id == Id::new(INPUT) || *id == Id::new(INSERT))
            {
                state.unfocus();
            }
        }
    }
    iced::advanced::widget::operate(ReleaseInsertFocus).discard()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructureQuery<'a> {
    Name(&'a str),
    Smiles(&'a str),
}

impl<'a> StructureQuery<'a> {
    fn read(text: &'a str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.len() > MAX_QUERY_BYTES {
            return None;
        }
        Some(
            if reshiki::chemistry::smiles::parse_input_for_import(text).is_ok() {
                Self::Smiles(text)
            } else {
                Self::Name(text)
            },
        )
    }

    fn result(self) -> String {
        let (value, source) = match self {
            Self::Name(value) => (value, "name"),
            Self::Smiles(value) => (value, "SMILES"),
        };
        let mut snippet: String = value.chars().take(40).collect();
        if snippet.len() < value.len() {
            snippet.push('…');
        }
        format!("Insert {snippet} · from {source}")
    }
}

impl App {
    fn can_insert_add_query(&self) -> bool {
        self.inspector_open
            && !self.templates.active
            && !self.templates.editing
            && !self.tab.busy
            && !self.tab.naming.is_pending()
            && self.naming_can_edit()
    }

    pub(super) fn insert_add_query(&mut self) -> Task<Message> {
        if self.inspector_tab != InspectorTab::Templates || !self.can_insert_add_query() {
            return Task::none();
        }
        match StructureQuery::read(&self.templates.query) {
            Some(StructureQuery::Smiles(value)) => {
                self.run(Request::import_smiles(value), Job::InsertAtView)
            }
            Some(StructureQuery::Name(value)) => {
                let value = value.to_owned();
                let _ = self.naming_action(naming::Action::PeptideSequence(false));
                let _ = self.naming_action(naming::Action::Name(value));
                self.imports.kind = super::import::InputKind::Name;
                self.insert_name_import(InsertPlacement::ViewCenter)
            }
            None => Task::none(),
        }
    }

    fn add_more(&self) -> Element<'_, Message> {
        let action = |id, label, message| {
            accessible_command(id, label, label)
                .width(Length::Fill)
                .on_press(message)
        };
        column![
            action(
                "add-advanced-import",
                "More import options…",
                Message::Inspector(InspectorTab::Import),
            ),
            (!self.tab.selected.is_empty()).then(|| action(
                "add-save-template",
                "Save selection as template…",
                Message::Templates(Action::BeginSave),
            )),
            action(
                "add-import-library",
                "Import template library…",
                Message::Templates(Action::Import),
            ),
            action(
                "add-export-library",
                "Export template library…",
                Message::Templates(Action::Export),
            ),
            action(
                "add-reload-library",
                "Reload template library",
                Message::Templates(Action::Reload),
            ),
        ]
        .spacing(3)
        .into()
    }

    pub(super) fn add_search(&self) -> Element<'_, Message> {
        let value = &self.templates.query;
        let query = StructureQuery::read(value);
        let submit = (query.is_some() && self.can_insert_add_query())
            .then_some(Message::Templates(Action::InsertQuery));
        let field = reshiki::accessibility::text_input(
            INPUT,
            "Name, SMILES or template",
            "Name, SMILES or template",
            value,
        )
        .on_input(|input| Message::Templates(Action::Search(input)))
        .on_submit(Message::Templates(Action::InsertQuery))
        .padding(8)
        .size(13)
        .style(crate::appearance::input_style);
        let menu_open = self.templates.add_menu_open;
        let more = reshiki::accessibility::button(
            "add-more",
            "Add options",
            iced::widget::canvas(Glyph(Icon::More, true))
                .width(24)
                .height(24),
        )
        .padding(3)
        .style(control(false))
        .expanded(menu_open)
        .on_press(Message::Templates(Action::AddMenu(!menu_open)));
        let caption = self.wants_name_caption();
        let can_caption = !matches!(query, Some(StructureQuery::Smiles(_)));
        let toggle_caption = can_caption && !self.tab.naming.is_pending();
        let mut content = column![
            row![field, more].spacing(6).align_y(Alignment::Center),
            menu_open.then(|| self.add_more()),
            accessible_checkbox(
                "add-name-caption",
                "Add name below",
                can_caption && caption,
                toggle_caption.then_some(Message::Naming(naming::Action::AddNameBelow(!caption))),
            ),
        ]
        .spacing(6);
        if let Some(query) = query {
            let label = query.result();
            content = content.push(
                accessible_command(INSERT, label.clone(), label)
                    .width(Length::Fill)
                    .on_press_maybe(submit),
            );
        }
        if let Some(StructureQuery::Name(value)) = query
            && let Some(notice) = self.imported_name_notice(value)
        {
            content = content.push(
                row![
                    text(notice).size(11).width(Length::Fill).style(muted_text),
                    accessible_command("add-import-details", "More import options", "Details")
                        .on_press(Message::Inspector(InspectorTab::Import)),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        content.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::StreamExt;
    use reshiki::document::Point;
    use reshiki::editing;

    #[test]
    fn add_search_distinguishes_chemical_names_from_structures() {
        assert_eq!(StructureQuery::read(" "), None);
        assert_eq!(
            StructureQuery::read("  caffeine ").unwrap().result(),
            "Insert caffeine · from name"
        );
        assert_eq!(
            StructureQuery::read("CCO").unwrap().result(),
            "Insert CCO · from SMILES"
        );
        for input in ["CCO ethanol", "CCO\tethanol", "CCO |u:0| ethanol"] {
            assert_eq!(
                StructureQuery::read(input),
                Some(StructureQuery::Smiles(input))
            );
        }
    }

    #[test]
    fn the_add_options_close_on_escape_and_keep_the_gallery() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::Inspector(InspectorTab::Templates));
        let _ = app.update(Message::Templates(Action::Collection("Aromatics".into())));
        let _ = app.update(Message::Templates(Action::Search("benzene".into())));
        let _ = app.update(Message::Templates(Action::AddMenu(true)));
        assert!(app.templates.add_menu_open);

        let _ = app.update(Message::Escape);
        assert!(!app.templates.add_menu_open);
        assert_eq!(app.templates.collection, "Aromatics");
        assert_eq!(app.templates.query, "benzene");

        let _ = app.update(Message::Templates(Action::AddMenu(true)));
        let _ = app.update(Message::Inspector(InspectorTab::Import));
        assert!(!app.templates.add_menu_open);
        assert_eq!(app.templates.query, "benzene");
    }

    #[test]
    fn replacing_the_add_query_cancels_its_pending_name_and_structure() {
        for query in ["ethanol", "CCO"] {
            for new_intent in [
                Message::Templates(Action::Search("caffeine".into())),
                Message::Templates(Action::Browse),
                Message::InsertTemplate(0),
                Message::Escape,
            ] {
                let (mut app, _) = App::new();
                app.tab.busy = false;
                let original = app.tab.doc.clone();
                let _ = app.update(Message::Inspector(InspectorTab::Templates));
                let _ = app.update(Message::Templates(Action::Search(query.into())));
                let _pending = app.update(Message::Templates(Action::InsertQuery));
                assert!(app.tab.naming.is_pending() || app.tab.busy);

                let _ = app.update(new_intent);
                assert!(!app.tab.naming.is_pending() && !app.tab.busy);
                assert_eq!(app.tab.doc, original);
                assert!(!app.tab.history.can_undo());
            }
        }
    }

    #[tokio::test]
    async fn enter_in_add_inserts_smiles_at_the_view_center_in_one_edit() {
        let (mut app, _) = App::new();
        app.engine = reshiki::engine::LocalEngine::in_process();
        app.tab.busy = false;
        app.tab.doc.add_atom("Cl", Point::new(18., 24.));
        app.tab.camera.center = Point::new(-135., 62.);
        app.tab.camera.zoom = 1.75;
        let before = app.tab.doc.clone();
        let camera = app.tab.camera;
        let _ = app.update(Message::Inspector(InspectorTab::Templates));
        let _ = app.update(Message::Templates(Action::Search("CCO".into())));
        let task = app.update(Message::Templates(Action::InsertQuery));
        let mut stream = iced_runtime::task::into_stream(task).unwrap();
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(message) = action {
                let _ = app.update(message);
            }
        }
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.atoms.len(), 4);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
        let inserted = editing::center(&app.tab.doc, &app.tab.selected);
        assert!((inserted.x - camera.center.x).abs() < 0.001);
        assert!((inserted.y - camera.center.y).abs() < 0.001);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }

    #[tokio::test]
    async fn add_imports_named_smiles_and_keeps_cx_notices_instead_of_resolving_a_name() {
        for (query, discarded_query) in [("CCO ethanol", false), ("CCO |u:0| ethanol", true)] {
            let (mut app, _) = App::new();
            app.engine = reshiki::engine::LocalEngine::in_process();
            app.tab.busy = false;
            let before = app.tab.doc.clone();
            let _ = app.update(Message::Inspector(InspectorTab::Templates));
            let _ = app.update(Message::Templates(Action::Search(query.into())));
            let task = app.update(Message::Templates(Action::InsertQuery));
            assert!(app.tab.busy && !app.tab.naming.is_pending());
            let mut stream = iced_runtime::task::into_stream(task).unwrap();
            while let Some(action) = stream.next().await {
                if let iced_runtime::Action::Output(message) = action {
                    let _ = app.update(message);
                }
            }
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc.atoms.len(), 3, "{}", app.status);
            assert_eq!(app.templates.query, query);
            assert_eq!(app.status.contains("query constraints"), discarded_query);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
        }
    }
}
