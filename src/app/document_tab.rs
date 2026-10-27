//! Per-document state. `App` keeps the active tab inline as `App::tab`; everything
//! else on `App` is app-wide (tool, settings, libraries, chrome, dialogs).
use super::{
    CleanupPreview, arcs, arrows, atom_labels, atom_text, autosave, document_styles, inline_text,
    inspector, joining, label_refresh, molecule_shortcuts, naming, numeric_transforms, pages,
    pictures, reaction_mapping, reactions, typography,
};
use crate::canvas::Camera;
use crate::session::Binding;
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
    /// [`Self::dirty`] as views showed it, at this revision and file epoch.
    /// `App::update` forgets it as it returns, as some changes (previews,
    /// background labels) keep the revision.
    unsaved: std::cell::Cell<Option<((u64, u64), bool)>>,
    pub(super) history: History,
    pub(super) revision: u64,
    pub(super) file_epoch: u64,
    pub(super) recent_molecules: molecule_shortcuts::Recent,
    pub(super) path: Option<PathBuf>,
    pub(super) office: Option<Binding>,
    pub(super) untitled_name: Option<&'static str>,
    // Selection and view.
    pub(super) selected: Vec<u64>,
    pub(super) hover: Option<(Point, u64)>,
    pub(super) keyboard_drawing: reshiki::keyboard_drawing::State,
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
    pub(super) optimization: Option<super::optimization::Session>,
    pub(super) optimization_serial: u64,
    pub(super) model_job: Option<super::models3d::Job>,
    pub(super) model_serial: u64,
    /// The revision, model and slider a 3D-model slider drag has reached;
    /// the drag is one Undo step until release, another slider or any other
    /// commit.
    pub(super) model_sliding:
        Option<(u64, usize, std::mem::Discriminant<super::models3d::Setting>)>,
    /// The model (by an atom) whose 2D → 3D morph is playing, and its start,
    /// for the drawing state it started in.
    pub(super) model_morph: Option<(super::models3d::Key, u64, iced::time::Instant)>,
    // Derived chemistry and in-flight work.
    pub(super) analysis: Option<Analysis>,
    pub(super) busy: bool,
    pub(super) engine_job: Option<super::engine_jobs::Pending>,
    pub(super) engine_serial: u64,
    pub(super) clipboard_busy: bool,
    pub(super) labels_dirty: bool,
    pub(super) label_refresh: label_refresh::State,
    /// Counts computed-label writes that need not bump revision; keys the mapping thumbnail.
    pub(super) label_generation: u64,
    pub(super) chemistry_notice: Option<String>,
    pub(super) reactions: reactions::State,
    pub(super) naming: naming::State,
    pub(super) mapping: reaction_mapping::State,
    // Current styles and inputs: follow the selection and the document's drawing style.
    pub(super) bond_drawing: reshiki::chains::BondDrawing,
    pub(super) chain_drawing: reshiki::chains::ChainDrawing,
    pub(super) drawing_length_input: String,
    pub(super) rear_opacity_draft: Option<super::depth_appearance::Draft>,
    pub(super) chain_atoms_input: String,
    pub(super) chain_angle_input: String,
    pub(super) arc_editor: arcs::Editor,
    pub(super) graphic_style: GraphicStyle,
    pub(super) path_point: Option<(u64, usize)>,
    pub(super) orbital_phase: reshiki::scientific::Phase,
    pub(super) phase_flipped: bool,
    pub(super) attach_symbols: bool,
    pub(super) snap_orbitals: bool,
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
    pub(super) arrow_label_gap_input: String,
    pub(super) arrow_label_gap_edited: bool,
    pub(super) bond_color_input: String,
    pub(super) isotope: String,
    pub(super) labels: atom_labels::State,
    pub(super) pictures: pictures::State,
    pub(super) numeric_transforms: numeric_transforms::State,
    pub(super) nmr: super::nmr::State,
    pub(super) styles: document_styles::State,
    pub(super) inspector_ui: inspector::State,
    // This document's recovery draft.
    pub(super) recovery: Option<Recovery>,
    pub(super) autosave: autosave::State,
    pub(super) autosaved_revision: Option<u64>,
    pub(super) autosave_status: String,
}

impl DocumentTab {
    pub(super) fn detach_office(&mut self) {
        if let Some(binding) = self.office.take() {
            binding.finish();
        }
    }

    pub(super) fn drawing_edit_active(&self) -> bool {
        self.cleanup.is_some()
            || self.joining.is_some()
            || self.optimization.is_some()
            || self.inline_text.is_some()
            || self.atom_text.is_some()
    }

    pub(super) fn new(
        recovery: Option<Recovery>,
        canvas_theme: reshiki::canvas_theme::CanvasTheme,
    ) -> Self {
        let doc = Document {
            canvas_theme,
            ..Default::default()
        };
        Self {
            id: TabId(0),
            edited: false,
            status: super::READY.into(),
            error: false,
            saved: doc.clone(),
            doc,
            unsaved: Default::default(),
            history: History::default(),
            revision: 0,
            file_epoch: 0,
            recent_molecules: Default::default(),
            path: None,
            office: None,
            untitled_name: None,
            selected: vec![],
            hover: None,
            keyboard_drawing: Default::default(),
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
            optimization: None,
            optimization_serial: 0,
            model_job: None,
            model_serial: 0,
            model_sliding: None,
            model_morph: None,
            analysis: None,
            busy: false,
            engine_job: None,
            engine_serial: 0,
            clipboard_busy: false,
            labels_dirty: false,
            label_refresh: Default::default(),
            label_generation: 0,
            chemistry_notice: None,
            reactions: Default::default(),
            naming: Default::default(),
            mapping: Default::default(),
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            drawing_length_input: reshiki::style::DEFAULT.bond_length_pt.to_string(),
            rear_opacity_draft: None,
            chain_atoms_input: String::new(),
            chain_angle_input: "120".into(),
            arc_editor: arcs::Editor::default(),
            graphic_style: GraphicStyle::default(),
            path_point: None,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            snap_orbitals: true,
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
            arrow_label_gap_input: String::new(),
            arrow_label_gap_edited: false,
            bond_color_input: "#000000".into(),
            isotope: String::new(),
            labels: Default::default(),
            pictures: pictures::State::default(),
            numeric_transforms: numeric_transforms::State::default(),
            nmr: super::nmr::State::default(),
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

    /// [`Self::dirty`] for the window title and tab strip, which ask several
    /// times per view: compared once, then remembered until the next update
    /// returns. Only views may ask, since an update can still change `doc`.
    pub(super) fn shown_dirty(&self) -> bool {
        let key = (self.revision, self.file_epoch);
        if let Some((cached, dirty)) = self.unsaved.get()
            && cached == key
        {
            debug_assert_eq!(
                dirty,
                self.dirty(),
                "the drawing changed since it was shown"
            );
            return dirty;
        }
        let dirty = self.dirty();
        self.unsaved.set(Some((key, dirty)));
        dirty
    }

    /// Drops the remembered answer once an update may have changed `doc`.
    pub(super) fn forget_unsaved(&self) {
        self.unsaved.set(None);
    }

    /// Records `saved` as the drawing's saved state.
    pub(super) fn mark_saved(&mut self, saved: Document) {
        self.saved = saved;
        self.forget_unsaved();
    }

    /// An unchanged empty Untitled drawing with nothing in progress, which New,
    /// Open and Restore take over instead of adding a tab.
    pub(super) fn reusable(&self) -> bool {
        self.path.is_none()
            && self.office.is_none()
            && self.untitled_name.is_none()
            && self.doc.all_ids().is_empty()
            && !self.history.can_undo()
            && !self.history.can_redo()
            && !self.dirty()
            && !self.busy
            && !self.clipboard_busy
            && !self.naming.is_pending()
            && !self.mapping.is_pending()
            && !self.nmr.is_pending()
            && self.cleanup.is_none()
            && self.optimization.is_none()
            && self.inline_text.is_none()
            && self.atom_text.is_none()
            && self.joining.is_none()
            && self.pictures.active.is_none()
    }
}

#[cfg(test)]
mod tests;
