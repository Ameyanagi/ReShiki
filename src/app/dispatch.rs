//! The message dispatch table: one handler per Message variant after the modal gates.
use super::{App, InspectorTab, Message, files};
use crate::canvas::{Edit, Tool};
use iced::Task;

impl App {
    pub(super) fn reveals_inspector(&self, message: &Message) -> bool {
        matches!(
            message,
            Message::Inspector(_)
                | Message::InsertTemplate(_)
                | Message::Tool(
                    Tool::Arrow | Tool::Graphic(_) | Tool::EditPoints | Tool::RingPreset(_)
                )
        ) || (self.inspector_tab != InspectorTab::Templates
            && matches!(message, Message::Canvas(Edit::Select(ids)) if ids.iter().any(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id) || self.tab.doc.graphics.iter().any(|g|g.id==*id))))
    }

    pub(super) fn dispatch_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            // Routed by the gates or update_front before dispatch.
            // Autosaved: handled before editor/modal guards.
            Message::Accessibility(_)
            | Message::Assistant(_)
            | Message::Updates(_)
            | Message::Palette(_)
            | Message::InlineText(_)
            | Message::AtomText(_)
            | Message::Join(_)
            | Message::Escape
            | Message::LabelsReady(..)
            | Message::InspectorAction(_)
            | Message::ContextMenu(_)
            | Message::Tab(..)
            | Message::Autosaved(..)
            | Message::FilePrepared(..)
            | Message::Saved(..) => {}
            Message::KeyboardDrawing(action) => return Some(self.keyboard_drawing_action(action)),
            Message::Optimization(action) => return Some(self.optimization_action(action)),
            Message::DepthAppearance(action) => return Some(self.depth_appearance_action(action)),
            Message::DrawingStyle(action) => return Some(self.drawing_style_action(action)),
            Message::Imports(action) => return Some(self.import_action(action)),
            Message::Pages(action) => return Some(self.page_action(action)),
            Message::Printing(action) => return Some(self.print_action(action)),
            Message::Pictures(action) => return Some(self.picture_action(action)),
            Message::ContextKey(key) => return Some(self.context_key(&key)),
            Message::StyleMenu(action) => return Some(self.style_menu_action(action)),
            Message::Shortcut(action) => return Some(self.shortcut_action(action)),
            Message::AromaticDisplay => return Some(self.request_aromatic_display()),
            Message::Abbreviations(action) => return Some(self.abbreviation_action(action)),
            Message::Labels(action) => self.label_action(action),
            Message::InspectorScroll(y) => self.scroll_templates(y),
            Message::TemplateNavigate(forward) => return Some(self.navigate_templates(forward)),
            Message::Reaction(action) => return Some(self.reaction_action(action)),
            Message::Templates(action) => return Some(self.template_message(action)),
            Message::ResetBondDrawing => self.reset_bond_drawing(),
            Message::FixedLength(on) => self.tab.bond_drawing.fixed_length = on,
            Message::FixedAngles(on) => self.tab.bond_drawing.fixed_angles = on,
            Message::DrawingLength(value) => self.set_drawing_length(value),
            Message::ChainAtoms(value) => self.set_chain_atoms(value),
            Message::ChainAngle(value) => self.set_chain_angle(value),
            Message::ApplyBondPreset(preset) => self.apply_bond_preset(preset),
            Message::BondPosition(position) => self.set_double_position(position),
            Message::BondColor(value) => self.tab.bond_color_input = value,
            Message::ApplyBondColor => self.apply_bond_color(),
            Message::AddFrame(kind) => self.add_frame(kind),
            Message::Group => self.group_selected(),
            Message::Ungroup => self.ungroup_selected(),
            Message::IntegralGroup(integral) => self.set_integral_groups(integral),
            Message::InvertSelection => self.invert_selection(),
            Message::Arc(action) => self.update_arc(action),
            Message::Graphics(action) => self.graphic_action(action),
            Message::RotateMark(id, index) => self.rotate_mark(id, index),
            Message::RemoveMark(id, index) => self.remove_mark(id, index),
            Message::AtomRadical(value) => self.set_radical(value),
            Message::ToggleInspector => return Some(self.toggle_inspector()),
            Message::Inspector(tab) => {
                if tab == InspectorTab::Nmr {
                    return Some(self.nmr_action(super::nmr::Action::Dock));
                }
                if let Some(task) = self.show_inspector_tab(tab) {
                    return Some(task);
                }
            }
            Message::InsertInput => return Some(self.insert_input()),
            Message::ToggleHelp => self.toggle_help(),
            Message::OpenShortcutExamples if self.pending.is_some() => {}
            Message::OpenShortcutExamples => return Some(self.open_shortcut_examples()),
            Message::Viewport(size) => self.set_viewport(size),
            Message::Tool(tool) => self.select_tool(tool),
            Message::Element(e) => self.choose_element(e),
            Message::CaptionAction(action) => self.caption_action(action),
            Message::TextStyle(change) => self.apply_text_style(change),
            Message::FontSize(value) => self.tab.font_size_input = value,
            Message::ApplyFontSize => self.apply_font_size_input(),
            Message::ClearRingFill => self.apply_ring_color(None),
            Message::ClearHighlights => self.apply_highlight_color(None),
            Message::ColorScope(scope) => self.set_color_scope(scope),
            Message::TextColor(value) => self.set_text_color_input(value),
            Message::ApplyTextColor => self.apply_text_color_input(),
            Message::TextAlign(alignment) => self.apply_paragraph(Some(alignment), None, None),
            Message::GroupLabelAlign(alignment) => self.apply_group_alignment(alignment),
            Message::TextSpacing(spacing) => self.apply_paragraph(None, Some(spacing), None),
            Message::TextWidth(value) => self.tab.text_width_input = value,
            Message::ApplyTextWidth => self.apply_text_width_input(),
            Message::Isotope(s) => self.tab.isotope = s,
            Message::RingSize(n) => self.set_ring_size(n),
            Message::AromaticRing(value) => self.set_aromatic_ring(value),
            Message::ToggleAromaticRing => return Some(self.toggle_aromatic_ring()),
            Message::ToggleSelectedRing => self.toggle_selected_ring(),
            Message::ArrowStyle(style) => self.set_arrow_style(style),
            Message::ArrowAction(action) => self.arrow_action(action),
            Message::CustomElement(s) => self.custom_element = s,
            Message::ApplyElement => self.apply_custom_element(),
            Message::CopyImage => return Some(self.copy_native(false, true)),
            Message::CopyAs(format) => return Some(self.copy_as(format)),
            Message::Copy(cut) => return Some(self.copy_selection(cut)),
            Message::PastePicture => return Some(self.paste_native(true)),
            Message::Paste => return Some(self.paste_clipboard()),
            Message::Duplicate => self.duplicate_selection(),
            Message::Transform(transform) => self.transform_selection(transform),
            Message::Nmr(action) => return Some(self.nmr_action(action)),
            Message::NumericTransform(action) => {
                return Some(self.numeric_transform_action(action));
            }
            Message::Arrange(arrange) => self.arrange_selection(arrange),
            Message::BondDepth(front) => self.layer_objects(front, false, true),
            Message::ReverseBonds => self.reverse_selected_bonds(),
            Message::InsertTemplate(index) => self.insert_template(index),
            Message::Tick => self.request_drafts(),
            Message::Restore if self.pending.is_some() => {}
            Message::Restore => self.restore_recovered(),
            Message::DismissRecovery => self.dismiss_recovery(),
            Message::Canvas(Edit::BeginTransform(field)) => {
                return Some(self.begin_numeric_transform(field));
            }
            Message::Canvas(edit) => {
                if let Some(task) = self.optimization_edit(edit.clone()) {
                    return Some(task);
                }
                self.edit(edit);
            }
            Message::Appearance(mode) => self.set_appearance(mode),
            Message::ColorTheme(theme) => self.apply_color_theme(theme),
            Message::CanvasTheme(theme) => self.set_canvas_theme(theme),
            Message::ThemeFile(action) => return Some(self.theme_file_action(action)),
            Message::ThemeGenerator(action) => return Some(self.theme_generator_action(action)),
            Message::QuickDrawingStyle(choice) => return Some(self.quick_drawing_style(choice)),
            Message::View(action) => self.view_action(action),
            Message::ObjectToolbar(action) => self.object_toolbar_action(action),
            Message::Fit => self.fit(),
            Message::Zoom(factor) => self.zoom_by(factor),
            Message::Import => return Some(self.import_input()),
            Message::Example(smiles) => return Some(self.insert_example(smiles)),
            Message::Analyze => return Some(self.analyze_drawing()),
            Message::Cleanup(action) => return Some(self.cleanup_action(action)),
            Message::Undo => self.step_history(false),
            Message::Redo => self.step_history(true),
            Message::Delete => self.delete_selection(),
            Message::SelectAll => self.select_all_objects(),
            Message::Charge(delta) => self.change_charge(delta),
            Message::ApplyIsotope => self.apply_isotope(),
            Message::CopySmiles => return Some(self.copy_smiles()),
            // Tabs wait while the save dialog asks about the tab in front.
            Message::New | Message::Open if self.pending.is_some() => {}
            Message::New => return Some(self.new_document()),
            Message::Open => return Some(self.open_dialog()),
            Message::Tabs(action) => return Some(self.tab_action(action)),
            Message::Close(window) => {
                if self.pending.is_none() {
                    return Some(self.close_window(window, vec![]));
                }
            }
            #[cfg(target_os = "linux")]
            Message::LinuxClipboardWindow(window) => {
                return Some(
                    iced::window::run(window, |window| {
                        reshiki_linux::initialize_clipboard(window);
                    })
                    .discard(),
                );
            }
            Message::Cancel => self.pending = None,
            Message::Discard => return Some(self.discard_pending()),
            #[cfg(target_os = "macos")]
            Message::MacFiles(_) => {}
            Message::Opened(file) => return Some(files::open_contents(file)),
            Message::Save => return Some(self.save_drawing(false)),
            Message::SaveAs => return Some(self.save_drawing(true)),
            Message::Export(format) => return Some(self.export_drawing(format)),
            Message::FigureExported(result) => self.figure_exported(result),
            message @ (Message::EngineDone { .. }
            | Message::ClipboardWritten { .. }
            | Message::CopyAsPrepared(..)
            | Message::CopyAsWritten(..)
            | Message::ClipboardRead { .. }
            | Message::Pasted(_)
            | Message::Exported(_)) => return Some(self.update_document_result(message)),
        }
        None
    }

    /// Background delivery calls these handlers without front-tab input or focus handling.
    pub(super) fn update_document_result(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Optimization(action) => return self.optimization_action(action),
            Message::Nmr(action) => return self.nmr_action(action),
            Message::LabelsReady(key, result) => self.labels_ready(key, result),
            Message::FigureExported(result) => self.figure_exported(result),
            Message::Printing(action) => return self.print_action(action),
            Message::DrawingStyle(action) => return self.drawing_style_action(action),
            Message::Assistant(action) => return self.assistant_action(action),
            Message::InspectorAction(action) => return self.inspector_action(action),
            Message::Pictures(action) => return self.picture_action(action),
            Message::Imports(action) => return self.import_action(action),
            Message::Shortcut(action) => return self.shortcut_action(action),
            Message::CopyAsPrepared(key, result) => return self.copy_as_prepared(key, *result),
            Message::CopyAsWritten(key, result) => self.copy_as_written(key, result),
            Message::ClipboardWritten {
                epoch,
                revision,
                cut_ids,
                result,
            } => self.clipboard_written(epoch, revision, cut_ids, result),
            Message::ClipboardRead {
                epoch,
                revision,
                result,
            } => self.clipboard_read(epoch, revision, *result),
            Message::Pasted(contents) => return self.pasted(contents),
            Message::EngineDone {
                revision,
                kind,
                result,
            } => return self.engine_done(revision, kind, *result),
            Message::Exported(result) => self.structure_exported(result),
            _ => {}
        }
        Task::none()
    }
}
