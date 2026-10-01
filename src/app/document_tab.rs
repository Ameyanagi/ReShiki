//! Per-document state. `App` keeps the active tab inline as `App::tab`; everything
//! else on `App` is app-wide (tool, settings, libraries, chrome, dialogs).
use super::{
    CleanupPreview, arcs, arrows, atom_labels, atom_text, autosave, document_styles, inline_text,
    inspector, joining, label_refresh, molecule_shortcuts, numeric_transforms, pages, pictures,
    reactions, typography,
};
use crate::canvas::Camera;
use reshiki::{
    document::{Document, History, Point},
    engine::Analysis,
    graphics::{BracketSides, GraphicStyle},
    recovery::Recovery,
};
use std::path::PathBuf;

/// Names a tab for the lifetime of the window; results of work started in a
/// tab carry it, so they can never land in another tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabId(pub(super) u64);

pub(super) struct DocumentTab {
    pub(super) id: TabId,
    /// Whether this tab had unsaved changes when it last left the front.
    pub(super) edited: bool,
    /// Parked status; the current tab uses `App::status` and `App::error`.
    pub(super) status: String,
    pub(super) error: bool,
    // Document, history and file identity.
    pub(super) doc: Document,
    pub(super) saved: Document,
    pub(super) history: History,
    pub(super) revision: u64,
    pub(super) file_epoch: u64,
    pub(super) recent_molecules: molecule_shortcuts::Recent,
    pub(super) path: Option<PathBuf>,
    pub(super) untitled_name: Option<&'static str>,
    // Selection and view.
    pub(super) selected: Vec<u64>,
    pub(super) hover: Option<(Point, u64)>,
    pub(super) camera: Camera,
    pub(super) fit_to_view: bool,
    pub(super) pages: pages::State,
    // In-progress edits and previews.
    pub(super) inline_text: Option<inline_text::State>,
    pub(super) atom_text: Option<atom_text::State>,
    pub(super) joining: Option<joining::State>,
    pub(super) erase_stroke: bool,
    pub(super) erase_committed: bool,
    pub(super) cleanup: Option<CleanupPreview>,
    pub(super) cleanup_serial: u64,
    // Derived chemistry and in-flight work.
    pub(super) analysis: Option<Analysis>,
    pub(super) busy: bool,
    pub(super) clipboard_busy: bool,
    pub(super) labels_dirty: bool,
    pub(super) label_refresh: label_refresh::State,
    pub(super) chemistry_notice: Option<String>,
    pub(super) reactions: reactions::State,
    // Current styles and inputs: follow the selection and the document's drawing style.
    pub(super) bond_drawing: reshiki::chains::BondDrawing,
    pub(super) chain_drawing: reshiki::chains::ChainDrawing,
    pub(super) drawing_length_input: String,
    pub(super) chain_atoms_input: String,
    pub(super) chain_angle_input: String,
    pub(super) arc_editor: arcs::Editor,
    pub(super) graphic_style: GraphicStyle,
    pub(super) orbital_phase: reshiki::scientific::Phase,
    pub(super) phase_flipped: bool,
    pub(super) attach_symbols: bool,
    pub(super) graphic_width_input: String,
    pub(super) graphic_stroke_input: String,
    pub(super) graphic_fill_input: String,
    pub(super) bracket_sides: BracketSides,
    pub(super) arrow_style: reshiki::arrows::Preset,
    pub(super) arrows: arrows::State,
    pub(super) caption: String,
    pub(super) caption_editor: iced::widget::text_editor::Content,
    pub(super) caption_format: reshiki::typography::TextFormat,
    pub(super) caption_target: Option<u64>,
    pub(super) font_size_input: String,
    pub(super) text_color_input: String,
    pub(super) color_scope: typography::ColorScope,
    pub(super) text_width_input: String,
    pub(super) bond_color_input: String,
    pub(super) isotope: String,
    pub(super) labels: atom_labels::State,
    pub(super) pictures: pictures::State,
    pub(super) numeric_transforms: numeric_transforms::State,
    pub(super) styles: document_styles::State,
    pub(super) inspector_ui: inspector::State,
    // This document's recovery draft.
    pub(super) recovery: Option<Recovery>,
    pub(super) autosave: autosave::State,
    pub(super) autosaved_revision: Option<u64>,
    pub(super) autosave_status: String,
}

impl DocumentTab {
    pub(super) fn new(recovery: Option<Recovery>) -> Self {
        Self {
            id: TabId(0),
            edited: false,
            status: super::READY.into(),
            error: false,
            doc: Document::default(),
            saved: Document::default(),
            history: History::default(),
            revision: 0,
            file_epoch: 0,
            recent_molecules: Default::default(),
            path: None,
            untitled_name: None,
            selected: vec![],
            hover: None,
            camera: Camera::default(),
            fit_to_view: false,
            pages: pages::State::default(),
            inline_text: None,
            atom_text: None,
            joining: None,
            erase_stroke: false,
            erase_committed: false,
            cleanup: None,
            cleanup_serial: 0,
            analysis: None,
            busy: false,
            clipboard_busy: false,
            labels_dirty: false,
            label_refresh: Default::default(),
            chemistry_notice: None,
            reactions: Default::default(),
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            drawing_length_input: reshiki::style::DEFAULT.bond_length_pt.to_string(),
            chain_atoms_input: String::new(),
            chain_angle_input: "120".into(),
            arc_editor: arcs::Editor::default(),
            graphic_style: GraphicStyle::default(),
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_width_input: "0.6".into(),
            graphic_stroke_input: "#000000".into(),
            graphic_fill_input: String::new(),
            bracket_sides: BracketSides::Both,
            arrow_style: Default::default(),
            arrows: Default::default(),
            caption: "Reaction conditions".into(),
            caption_editor: iced::widget::text_editor::Content::with_text("Reaction conditions"),
            caption_format: Default::default(),
            caption_target: None,
            font_size_input: "10".into(),
            text_color_input: "#000000".into(),
            color_scope: Default::default(),
            text_width_input: String::new(),
            bond_color_input: "#000000".into(),
            isotope: String::new(),
            labels: Default::default(),
            pictures: pictures::State::default(),
            numeric_transforms: numeric_transforms::State::default(),
            styles: Default::default(),
            inspector_ui: inspector::State::default(),
            recovery,
            autosave: autosave::State::default(),
            autosaved_revision: None,
            autosave_status: String::new(),
        }
    }

    /// The file name, or the name of an unsaved drawing.
    pub(super) fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.untitled_name.unwrap_or("Untitled").into())
    }

    pub(super) fn dirty(&self) -> bool {
        self.inline_changed() || !super::same_drawing(&self.doc, &self.saved)
    }

    /// An unchanged empty Untitled drawing with nothing in progress, which New,
    /// Open and Restore take over instead of adding a tab.
    pub(super) fn reusable(&self) -> bool {
        self.path.is_none()
            && self.untitled_name.is_none()
            && self.doc.all_ids().is_empty()
            && !self.history.can_undo()
            && !self.history.can_redo()
            && !self.dirty()
            && !self.busy
            && !self.clipboard_busy
            && self.cleanup.is_none()
            && self.inline_text.is_none()
            && self.atom_text.is_none()
            && self.joining.is_none()
            && self.pictures.active.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::App, app::Message, canvas::Tool};

    #[test]
    fn a_replaced_tab_keeps_its_document_selection_view_and_undo() {
        let (mut app, _) = App::new();
        let before = app.tab.doc.clone();
        let atom = app.tab.doc.add_atom("N", Point::default());
        app.changed(before);
        app.tab.selected = vec![atom];
        app.tab.camera.zoom = 2.0;
        app.tool = Tool::Erase;

        let first = std::mem::replace(&mut app.tab, DocumentTab::new(None));
        assert!(app.tab.doc.all_ids().is_empty());
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("O", Point::default());
        app.changed(before);
        let second = std::mem::replace(&mut app.tab, first);

        assert_eq!(app.tab.selected, vec![atom]);
        assert_eq!(app.tab.camera.zoom, 2.0);
        assert_eq!(app.tool, Tool::Erase, "The tool is app-wide");
        let _ = app.update(Message::Undo);
        assert!(app.tab.doc.all_ids().is_empty(), "Undo edits its own tab");
        assert_eq!(second.doc.atoms.len(), 1);
        assert_eq!(second.doc.atoms[0].element, "O");
    }
}
