//! Modal gates that run before message dispatch. Their order decides which modal surface wins.
use super::{
    App, InspectorTab, Message, atom_text, color_popover, document_styles, import, inline_text,
    joining, object_toolbar, optimization, pictures, printing, tabs, template_library,
    theme_generator, updates,
};
use crate::canvas::{self, Edit, Tool};
use iced::Task;
use reshiki::document::Point;
use std::ops::ControlFlow::{self, Break, Continue};

pub(super) type Gate = ControlFlow<Task<Message>, Message>;

impl App {
    pub(super) fn gate(&mut self, message: Message) -> Gate {
        let message = self.early_results_gate(message)?;
        let message = self.exit_gate(message)?;
        let message = self.labels_gate(message)?;
        if let Some(task) = self.optimization_gate(&message) {
            return Break(task);
        }
        let message = self.file_prepared_gate(message)?;
        let message = self.updates_dialog_gate(message)?;
        let Some(message) = self.prepare_molecule_shortcut(message) else {
            return Break(Task::none());
        };
        let message = self.restart_gate(message)?;
        let message = self.style_menu_gate(message)?;
        let message = self.import_drag_gate(message)?;
        let message = self.atom_text_gate(message)?;
        let message = self.overlay_escape_gate(message)?;
        self.close_transient_menus(&message);
        let message = self.panel_routing_gate(message)?;
        let message = self.joining_gate(message)?;
        let message = self.inline_text_routing_gate(message)?;
        let message = self.escape_gate(message)?;
        let message = self.inline_text_gate(message)?;
        let message = self.palette_assistant_gate(message)?;
        let message = self.cleanup_preview_gate(message)?;
        self.abbreviation_member_gate(message)
    }

    fn early_results_gate(&mut self, message: Message) -> Gate {
        if let Message::Autosaved(key, result) = message {
            return Break(self.autosaved(key, result));
        }
        if let Message::Saved(epoch, snapshot, result) = message {
            return Break(self.file_saved(epoch, snapshot, result));
        }
        if let Message::Templates(template_library::Action::Finished(serial, result)) = message {
            return Break(self.template_finished(serial, result));
        }
        if let Message::Templates(template_library::Action::WarningAcknowledged(serial)) = message {
            return Break(self.template_warning_acknowledged(serial));
        }
        if let Message::Templates(action @ template_library::Action::Imported(_)) = message {
            return Break(self.template_message(action));
        }
        Continue(message)
    }

    fn exit_gate(&mut self, message: Message) -> Gate {
        if self.exit.frozen()
            && !matches!(
                message,
                Message::Updates(updates::Action::RecoveryCleared | updates::Action::Restarted(_))
            )
        {
            if tabs::document_result(&message) {
                self.defer_document_result(self.tab.id, message);
            }
            return Break(Task::none());
        }
        Continue(message)
    }

    fn labels_gate(&mut self, message: Message) -> Gate {
        if let Message::LabelsReady(key, result) = message {
            self.labels_ready(key, result);
            return Break(Task::none());
        }
        Continue(message)
    }

    fn file_prepared_gate(&mut self, message: Message) -> Gate {
        if let Message::FilePrepared(opened) = message {
            return Break(self.file_prepared(opened));
        }
        Continue(message)
    }

    fn updates_dialog_gate(&mut self, message: Message) -> Gate {
        if self.updates.open {
            match message {
                Message::Updates(action) => return Break(self.update_action(action)),
                Message::Escape => return Break(self.update_action(updates::Action::Show(false))),
                _ if !updates::background(&message) && !self.answers_save_dialog(&message) => {
                    return Break(Task::none());
                }
                _ => {}
            }
        }
        Continue(message)
    }

    fn restart_gate(&mut self, message: Message) -> Gate {
        if self.updates.restarting && !matches!(message, Message::Updates(_)) {
            return Break(Task::none());
        }
        Continue(message)
    }

    /// Before the atom label editor, which ignores other messages: it must
    /// never open under the popover.
    fn style_menu_gate(&mut self, message: Message) -> Gate {
        if self.style_menu.is_some() {
            if matches!(message, Message::Escape) {
                return Break(self.style_menu_action(self.style_menu_escape()));
            }
            if !color_popover::keeps_open(&message) {
                self.close_style_menu();
            }
        }
        Continue(message)
    }

    fn import_drag_gate(&mut self, message: Message) -> Gate {
        if let Message::Imports(
            action @ (import::Action::Hovered(_)
            | import::Action::Dropped(_)
            | import::Action::Left),
        ) = message
        {
            return Break(self.import_drag(action));
        }
        Continue(message)
    }

    fn atom_text_gate(&mut self, message: Message) -> Gate {
        if let Message::AtomText(action) = message {
            return Break(self.atom_text_action(action));
        }
        if self.tab.atom_text.is_some() {
            if matches!(message, Message::Escape) {
                return Break(self.atom_text_action(atom_text::Action::Cancel));
            }
            if !atom_text::background(&message) && !self.answers_save_dialog(&message) {
                return Break(Task::none());
            }
        }
        Continue(message)
    }

    fn overlay_escape_gate(&mut self, message: Message) -> Gate {
        if self.help_open && matches!(message, Message::Escape | Message::ToggleHelp) {
            self.help_open = false;
            return Break(Task::none());
        }
        if let Message::ContextMenu(action) = message {
            return Break(self.context_action(action));
        }
        if self.context_menu.is_some() && matches!(message, Message::Escape) {
            self.context_menu = None;
            return Break(Task::none());
        }
        if (self.imports.menu || self.imports.examples_menu) && matches!(message, Message::Escape) {
            self.imports.menu = false;
            self.imports.examples_menu = false;
            return Break(Task::none());
        }
        if self.tabs.menu && matches!(message, Message::Escape) {
            self.tabs.menu = false;
            return Break(Task::none());
        }
        Continue(message)
    }

    fn close_transient_menus(&mut self, message: &Message) {
        if !matches!(
            message,
            Message::Canvas(Edit::Hover(_))
                | Message::Tick
                | Message::InspectorScroll(_)
                | Message::EngineDone { .. }
                | Message::InspectorAction(_)
                | Message::Viewport(_)
                | Message::Updates(_)
                | Message::Imports(import::Action::Loaded(..))
        ) {
            self.context_menu = None;
            self.tab.inspector_ui.close_menu();
            if !matches!(
                message,
                Message::Imports(import::Action::Menu(_) | import::Action::ExamplesMenu(_))
            ) {
                self.imports.menu = false;
                self.imports.examples_menu = false;
            }
            if !matches!(message, Message::Tabs(tabs::Action::Menu(_))) {
                self.tabs.menu = false;
            }
        }
    }

    fn panel_routing_gate(&mut self, message: Message) -> Gate {
        if let Message::InspectorAction(action) = message {
            return Break(self.inspector_action(action));
        }
        if let Message::Updates(action) = message {
            return Break(self.update_action(action));
        }
        Continue(message)
    }

    fn joining_gate(&mut self, message: Message) -> Gate {
        if let Message::Join(action) = message {
            return Break(self.join_action(action));
        }
        if self.tab.joining.is_some()
            && matches!(message, Message::Escape | Message::TemplateNavigate(false))
        {
            return Break(self.join_action(joining::Action::Cancel));
        }
        if self.tab.joining.is_some() && joining::cancels_draft(&message) {
            self.cancel_join();
        }
        Continue(message)
    }

    fn inline_text_routing_gate(&mut self, message: Message) -> Gate {
        if let Message::InlineText(action) = message {
            return Break(self.inline_action(action));
        }
        Continue(message)
    }

    fn escape_gate(&mut self, message: Message) -> Gate {
        if matches!(message, Message::Escape) && self.inspector_tab == InspectorTab::ThemeGenerator
        {
            return Break(self.theme_generator_action(theme_generator::Action::Back));
        }
        if matches!(message, Message::Escape)
            && self.inspector_tab == InspectorTab::DrawingStyle
            && self.tab.styles.editor.is_some()
        {
            return Break(self.drawing_style_action(document_styles::Action::Cancel));
        }
        if matches!(message, Message::Escape) {
            return Break(if self.tab.inline_text.is_some() {
                self.inline_action(inline_text::Action::Finish(false))
            } else if self.tab.optimization.is_some() {
                self.optimization_action(optimization::Action::Cancel)
            } else {
                self.update(Message::Tool(Tool::Select))
            });
        }
        Continue(message)
    }

    fn inline_text_gate(&mut self, message: Message) -> Gate {
        if self.tab.inline_text.is_some() && matches!(message, Message::Undo | Message::Redo) {
            return Break(
                self.inline_action(inline_text::Action::Undo(matches!(message, Message::Redo))),
            );
        }
        if let Message::Canvas(Edit::BeginText(id)) = message {
            return Break(
                self.inline_action(inline_text::Action::Begin(Some(id), Point::default())),
            );
        }
        if let Message::Canvas(Edit::Click(p)) = message
            && self.tool == Tool::Text
        {
            if let Some(id) = canvas::hit_object(&self.tab.doc, p, 8. / self.tab.camera.zoom)
                .filter(|id| self.tab.doc.atom(*id).is_some())
            {
                return Break(self.atom_text_action(atom_text::Action::Begin(Some(id))));
            }
            let id = canvas::hit_object(&self.tab.doc, p, 8. / self.tab.camera.zoom)
                .filter(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id));
            return Break(self.inline_action(inline_text::Action::Begin(id, p)));
        }
        if inline_text::commits_draft(&message) && !self.finish_inline(true) {
            return Break(Task::none());
        }
        Continue(message)
    }

    fn palette_assistant_gate(&mut self, message: Message) -> Gate {
        if let Message::Palette(action) = message {
            return Break(self.palette_action(action));
        }
        if self.palette.is_some() && matches!(message, Message::Tool(Tool::Select)) {
            self.palette = None;
            return Break(Task::none());
        }
        if self.assistant.menu.is_some() && matches!(message, Message::Tool(Tool::Select)) {
            self.assistant.menu = None;
            return Break(Task::none());
        }
        if let Message::Assistant(action) = message {
            return Break(self.assistant_action(action));
        }
        Continue(message)
    }

    fn cleanup_preview_gate(&mut self, message: Message) -> Gate {
        if self.tab.cleanup.is_some() {
            if matches!(message, Message::Tool(Tool::Select)) {
                return Break(self.update(Message::CancelCleanup));
            }
            if !(matches!(
                &message,
                Message::ApplyCleanup
                    | Message::CancelCleanup
                    | Message::CleanupOriginal(_)
                    | Message::CleanupScope(_)
                    | Message::CleanupOrientation(_)
                    | Message::Printing(
                        printing::Action::Prepared(..) | printing::Action::Finished(..)
                    )
                    | Message::Pictures(pictures::Action::Loaded(..))
                    | Message::Imports(import::Action::Loaded(..))
            ) || preview_passthrough(&message))
                && !self.answers_save_dialog(&message)
            {
                if !matches!(message, Message::Canvas(_)) {
                    self.status = "Apply or cancel the cleanup preview to continue editing".into();
                }
                return Break(Task::none());
            }
        }
        Continue(message)
    }

    fn abbreviation_member_gate(&mut self, message: Message) -> Gate {
        if matches!(
            &message,
            Message::Charge(_) | Message::AtomRadical(_) | Message::ApplyIsotope
        ) && self
            .tab
            .doc
            .abbreviations
            .iter()
            .any(|g| g.members.iter().any(|id| self.tab.selected.contains(id)))
        {
            self.status =
                "Expand the selected abbreviation before changing individual atom properties"
                    .into();
            self.error = true;
            return Break(Task::none());
        }
        Continue(message)
    }
}

/// View, window, file and async-result messages that both modal previews (cleanup, 3D) let through.
pub(super) fn preview_passthrough(message: &Message) -> bool {
    matches!(
        message,
        Message::Canvas(Edit::Pan(..) | Edit::Zoom(..) | Edit::Hover(_))
            | Message::InspectorScroll(_)
            | Message::Viewport(_)
            | Message::Fit
            | Message::Zoom(_)
            | Message::ToggleInspector
            | Message::Inspector(_)
            | Message::Appearance(_)
            | Message::ToggleView
            | Message::ObjectToolbar(object_toolbar::Action::Visible(_))
            | Message::Grid
            | Message::SmartGuides(_)
            | Message::Rulers(_)
            | Message::Crosshair(_)
            | Message::RulerUnit(_)
            | Message::Tick
            | Message::EngineDone { .. }
            | Message::Close(_)
            | Message::Discard
            | Message::Cancel
            | Message::New
            | Message::Open
            | Message::Tabs(_)
            | Message::Saved(..)
            | Message::Exported(_)
            | Message::FigureExported(_)
            | Message::Opened(_)
            | Message::ClipboardRead { .. }
            | Message::ClipboardWritten { .. }
            | Message::CopyAsPrepared(..)
            | Message::CopyAsWritten(..)
    )
}
