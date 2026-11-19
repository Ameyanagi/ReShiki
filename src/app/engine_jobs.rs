//! Chemistry engine jobs: launching requests and applying their results.
use super::{App, Job, Message, export_file, input_request};
use crate::canvas::Tool;
use iced::{Subscription, Task};
use reshiki::{
    document::{Document, Point},
    editing,
    engine::{Analysis, ChemistryEngine, Request, Response},
};

const INSERT_STRUCTURE_FAILED: &str = "Could not insert structure";
const PROGRESS_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// A receipt belongs to one request, drawing and tab, even after cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    tab: super::document_tab::TabId,
    epoch: u64,
    revision: u64,
    serial: u64,
}

/// Dropping the task handle aborts its future, including an active native helper.
/// A blocking calculation already running may finish, but cannot publish a result.
pub(super) struct Pending {
    key: Key,
    kind: Job,
    started: iced::time::Instant,
    _handle: iced::task::Handle,
}

impl App {
    pub(super) fn run(&mut self, mut request: Request, kind: Job) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        if request.operation == "import" {
            request.drawing_options = Some((&self.tab.doc.drawing_style).into());
        }
        let engine = self.engine.clone();
        let aromatic_selection = matches!(kind, Job::AromaticDisplay);
        self.start_engine_job(kind, async move {
            if aromatic_selection {
                super::shortcuts::aromatic_selection(engine, request).await
            } else {
                engine.execute(request).await
            }
        })
    }

    fn start_engine_job(
        &mut self,
        kind: Job,
        future: impl Future<Output = Result<Response, String>> + Send + 'static,
    ) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        self.tab.busy = true;
        self.error = false;
        self.status = "Working…".into();
        self.tab.engine_serial = self.tab.engine_serial.wrapping_add(1);
        let key = Key {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            serial: self.tab.engine_serial,
        };
        let (task, handle) = Task::perform(future, move |result| Message::EngineFinished {
            key,
            result: Box::new(result),
        })
        .abortable();
        self.tab.engine_job = Some(Pending {
            key,
            kind,
            started: iced::time::Instant::now(),
            _handle: handle.abort_on_drop(),
        });
        task
    }

    pub(super) fn engine_subscription(&self) -> Subscription<Message> {
        if self.strip().any(|tab| tab.engine_job.is_some()) {
            iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    pub(super) fn engine_progress(&self) -> Option<(Key, String)> {
        let job = self.tab.engine_job.as_ref()?;
        let elapsed = job.started.elapsed();
        (elapsed >= PROGRESS_DELAY).then(|| (job.key, format!("Working · {} s", elapsed.as_secs())))
    }

    /// An activation queued on a previous job must never cancel a newer one.
    pub(super) fn cancel_engine_request(&mut self, key: Key) {
        if key.tab == self.tab.id
            && key.epoch == self.tab.file_epoch
            && self
                .tab
                .engine_job
                .as_ref()
                .is_some_and(|job| job.key == key)
        {
            self.cancel_engine_job();
        }
    }

    /// Cancel affects only transient work. Drawing, history, selection and camera stay put.
    pub(super) fn cancel_engine_job(&mut self) {
        let Some(job) = self.tab.engine_job.take() else {
            return;
        };
        self.tab.busy = false;
        #[cfg(target_arch = "wasm32")]
        if matches!(job.kind, Job::ImportFile(_)) {
            self.tab.file_handle = None;
        }
        #[cfg(any(windows, target_os = "linux"))]
        if matches!(job.kind, Job::ImportFile(_)) {
            self.desktop_import_finished();
        }
        drop(job);
        self.status = "Operation canceled".into();
        self.error = false;
    }

    pub(super) fn engine_finished(
        &mut self,
        key: Key,
        result: Result<Response, String>,
    ) -> Task<Message> {
        if key.tab != self.tab.id || key.epoch != self.tab.file_epoch {
            return Task::none();
        }
        let Some(job) = self.tab.engine_job.take_if(|job| job.key == key) else {
            return Task::none();
        };
        self.engine_done(key.revision, job.kind, result)
    }
    pub(super) fn request_aromatic_display(&mut self) -> Task<Message> {
        if self.tab.selected.is_empty() {
            self.status = "Select an aromatic ring first".into();
            return Task::none();
        }
        let mut request = Request::molecule("aromatic", self.tab.doc.clone());
        request.selected_ids = Some(self.tab.selected.clone());
        self.run(request, Job::AromaticDisplay)
    }
    pub(super) fn insert_input(&mut self) -> Task<Message> {
        if self.imports.kind == super::import::InputKind::Name {
            return self.naming_action(super::naming::Action::InsertName);
        }
        self.run(input_request(&self.imports.input.text()), Job::Insert)
    }
    pub(super) fn import_input(&mut self) -> Task<Message> {
        self.run(input_request(&self.imports.input.text()), Job::Import)
    }
    pub(super) fn insert_example(&mut self, smiles: &'static str) -> Task<Message> {
        self.imports.kind = super::import::InputKind::Structure;
        self.imports.set_text(smiles);
        self.run(Request::import_smiles(smiles), Job::Insert)
    }
    pub(super) fn analyze_drawing(&mut self) -> Task<Message> {
        self.run(
            // Check chemistry; Illustration molecules are pictures.
            Request::molecule(
                "analyze",
                reshiki::purpose::only(&self.tab.doc, reshiki::purpose::Purpose::is_chemical)
                    .into_owned(),
            ),
            Job::Analyze,
        )
    }
    /// A chemistry job's result for its originating drawing.
    pub(super) fn engine_done(
        &mut self,
        revision: u64,
        kind: Job,
        result: Result<Response, String>,
    ) -> Task<Message> {
        self.tab.busy = false;
        #[cfg(any(windows, target_os = "linux"))]
        if matches!(kind, Job::ImportFile(_)) {
            self.desktop_import_finished();
        }
        if matches!(&kind, Job::Clean(job) if job.serial != self.tab.cleanup_serial || job.epoch != self.tab.file_epoch)
        {
            return Task::none();
        }
        let response = match result {
            Err(e) => {
                #[cfg(target_arch = "wasm32")]
                if matches!(kind, Job::ImportFile(_)) {
                    self.tab.file_handle = None;
                }
                let Job::ImportFile(path) = &kind else {
                    self.error = true;
                    self.status = e;
                    return Task::none();
                };
                // The tab added for this file closes again, so a failed open
                // leaves no empty Untitled tab behind. One that failed behind
                // the front tab stays, with the error, until it is looked at.
                let id = self.tab.id;
                let added = self.tabs.opening.contains(&(id, self.tab.file_epoch));
                self.tabs.opening.retain(|(tab, _)| *tab != id);
                let closed = if added
                    && self.tabs.temporary_depth == 0
                    && self.tab.reusable()
                    && self.file_io.saving_tab != Some(id)
                {
                    self.close_active_tab()
                } else {
                    Task::none()
                };
                self.error = true;
                self.status = format!("Could not open {}: {e}", super::import::name(path));
                return closed;
            }
            Ok(response) => response,
        };
        let id = self.tab.id;
        self.tabs.opening.retain(|(tab, _)| *tab != id);
        if let Job::Export(format) = kind {
            let mut warnings = response.warnings;
            if matches!(format, "cdx" | "cdxml") && self.tab.doc.canvas_theme.is_dark() {
                warnings.insert(0, "Light canvas colors: ChemDraw has no dark canvas".into());
            }
            let (output, stem) = (response.output.unwrap_or_default(), self.export_stem());
            // What a ChemDraw file leaves out is read before it is written.
            #[cfg(not(target_arch = "wasm32"))]
            if matches!(format, "cdx" | "cdxml") && !warnings.is_empty() {
                return read_losses(format, warnings).then(move |go| {
                    if go {
                        export_file(output.clone(), format, stem.clone(), vec![])
                    } else {
                        Task::done(Message::Exported(Ok(None)))
                    }
                });
            }
            return export_file(output, format, stem, warnings);
        }
        if self.tab.revision != revision {
            self.status = "Finished · Newer edits kept · Run again to update".into();
            return Task::none();
        }
        match kind {
            Job::Clean(_) => self.cleanup_ready(response),
            Job::Projection(name) => self.projection_ready(name, response),
            Job::Insert => return self.structure_inserted(response),
            Job::AromaticDisplay => self.aromatic_display_ready(response),
            Job::Abbreviate => self.abbreviations_ready(response),
            Job::Analyze => self.analysis_ready(response),
            kind @ (Job::Import | Job::ImportFile(_) | Job::Export(_)) => {
                self.structure_replaced(kind, response)
            }
        }
        Task::none()
    }
    fn projection_ready(&mut self, name: &str, response: Response) {
        let Some(document) = response.document else {
            return;
        };
        let before = std::mem::replace(&mut self.tab.doc, document);
        self.changed(before);
        if !self.error {
            self.tab.analysis = whole_analysis(&self.tab.doc, response.analysis);
            self.status = format!("{name} applied");
            if !response.warnings.is_empty() {
                self.status.push_str(" · ");
                self.status.push_str(&response.warnings.join(" · "));
            }
        }
    }
    /// The cleaned drawing replaces the original as one Undo step; Undo and
    /// Redo compare the two.
    fn cleanup_ready(&mut self, response: Response) {
        let Some(document) = response.document else {
            return;
        };
        if let Err(error) = document.validate() {
            self.status = error;
            self.error = true;
            return;
        }
        let before = std::mem::replace(&mut self.tab.doc, document);
        self.changed(before);
        if !self.error {
            self.tab.analysis = whole_analysis(&self.tab.doc, response.analysis);
            self.status = format!(
                "Cleaned up · {} undoes",
                super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Z")
            );
            for warning in &response.warnings {
                self.status.push_str(" · ");
                self.status.push_str(warning);
            }
        }
    }
    /// Place a document beside the drawing, centered on the camera's row.
    /// An empty drawing centers it on the camera instead. Returns the inserted
    /// IDs, or `None` when append inserts nothing, leaving the drawing unchanged
    /// if IDs are exhausted or a document is invalid.
    pub(super) fn insert_beside_drawing(&mut self, document: &Document) -> Option<Vec<u64>> {
        let center = editing::center(document, &document.all_ids());
        let offset = if self.tab.doc.all_ids().is_empty() {
            Point::new(
                self.tab.camera.center.x - center.x,
                self.tab.camera.center.y - center.y,
            )
        } else {
            let (_, existing_max) = self.tab.doc.bounds();
            let (insert_min, _) = document.bounds();
            Point::new(
                existing_max.x + self.tab.doc.drawing_style.bond_length_world - insert_min.x,
                self.tab.camera.center.y - center.y,
            )
        };
        // A generated structure follows this drawing's label settings instead
        // of overriding them per atom.
        let mut document = document.clone();
        document.atom_labels = self.tab.doc.atom_labels.clone();
        let ids = editing::append_preserving_layout(&mut self.tab.doc, &document, offset);
        (!ids.is_empty()).then_some(ids)
    }

    /// Reveal an insertion together with the drawing that was already in view.
    pub(super) fn commit_structure_insert(&mut self, before: Document, selected: Vec<u64>) {
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let camera = self.tab.camera;
        // The on-screen part of the drawing: objects that reach into the view,
        // clipped to it, so off-screen objects do not widen the reveal.
        let (w, h) = (paper.width / camera.zoom, paper.height / camera.zoom);
        let view_lo = camera.center.offset(-w / 2., -h / 2.);
        let view_hi = camera.center.offset(w / 2., h / 2.);
        let ids: Vec<_> = before.all_ids().into_iter().map(|id| vec![id]).collect();
        let visible = reshiki::scene::selections_bounds(&before, &ids)
            .into_iter()
            .flatten()
            .map(|(lo, hi)| {
                (
                    Point::new(lo.x.max(view_lo.x), lo.y.max(view_lo.y)),
                    Point::new(hi.x.min(view_hi.x), hi.y.min(view_hi.y)),
                )
            })
            .filter(|(lo, hi)| lo.x <= hi.x && lo.y <= hi.y)
            .reduce(|(a, b), (c, d)| {
                (
                    Point::new(a.x.min(c.x), a.y.min(c.y)),
                    Point::new(b.x.max(d.x), b.y.max(d.y)),
                )
            });
        self.tab.selected = selected;
        if !self.adopt_changed_document(before) {
            return;
        }
        // A rejected edit restores the drawing, leaving nothing to reveal.
        let Some((mut lo, mut hi)) =
            reshiki::scene::selection_bounds(&self.tab.doc, &self.tab.selected)
        else {
            return;
        };
        let viewport = iced::Rectangle::with_size(paper.size());
        if viewport.contains(camera.screen(lo, paper))
            && viewport.contains(camera.screen(hi, paper))
        {
            return;
        }
        if let Some((old_lo, old_hi)) = visible {
            lo = Point::new(lo.x.min(old_lo.x), lo.y.min(old_lo.y));
            hi = Point::new(hi.x.max(old_hi.x), hi.y.max(old_hi.y));
        }
        self.reveal_bounds(lo, hi);
    }

    fn structure_inserted(&mut self, response: Response) -> Task<Message> {
        let Some(document) = response.document else {
            return Task::none();
        };
        let before = self.tab.doc.clone();
        let Some(selected) = self.insert_beside_drawing(&document) else {
            self.tab.doc = before;
            self.error = true;
            self.status = INSERT_STRUCTURE_FAILED.into();
            return Task::none();
        };
        self.commit_structure_insert(before, selected);
        if self.error {
            return Task::none();
        }
        self.tool = Tool::Select;
        self.status = "Structure inserted · Drag to position".into();
        if !response.warnings.is_empty() {
            self.status.push_str(" · ");
            self.status.push_str(&response.warnings.join(" · "));
        }
        super::focus_drawing()
    }
    fn aromatic_display_ready(&mut self, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            self.tab.doc = document.clone();
            self.changed(before);
            if self.error {
                return;
            }
            reshiki::atom_labels::copy_computed(&mut self.tab.doc, &document);
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
            self.tab.labels_dirty = false;
            self.tab.analysis = whole_analysis(&self.tab.doc, response.analysis);
            self.tool = Tool::Select;
            self.status = "Aromatic display changed · Molecular identity retained".into();
        }
    }
    fn abbreviations_ready(&mut self, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            let count = document.abbreviations.len();
            self.tab.doc = document;
            self.tab.selected = self
                .tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected);
            self.changed(before);
            self.tab.analysis = whole_analysis(&self.tab.doc, response.analysis);
            self.tool = Tool::Select;
            self.status = if count == 0 {
                "No matching common groups in this selection".into()
            } else {
                format!(
                    "{count} abbreviation{} · Chemistry kept · Expand to edit",
                    if count == 1 { "" } else { "s" }
                )
            };
        }
    }
    fn analysis_ready(&mut self, response: Response) {
        if response.analysis.is_none() {
            // A valid drawing can have no molecular interpretation, notably
            // when a crystal's remaining edges still carry distance provenance.
            // A skipped check must not refresh labels or move the drawing.
            self.tab.analysis = None;
            self.tab.chemistry_notice = None;
            self.tab.stereo_notes.clear();
            self.status = if response.warnings.is_empty() {
                "Chemical analysis unavailable".into()
            } else {
                response.warnings.join(" · ")
            };
            self.error = false;
            return;
        }
        // Checking is a read-only chemistry operation. Refresh
        // computed H labels without rewriting the user's bond
        // orders/stereo or inserting a step into Undo/Redo.
        if let Some(document) = response.document {
            reshiki::atom_labels::copy_computed(&mut self.tab.doc, &document);
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
        }
        self.tab.analysis = response
            .analysis
            .filter(|_| self.tab.doc.atoms.iter().all(|a| a.map_num == 0));
        self.tab.chemistry_notice = None;
        self.checked_stereo(response.warnings);
        self.error = false;
    }
    fn structure_replaced(&mut self, kind: Job, response: Response) {
        self.tab.stereo_notes.clear();
        if let Some(document) = response.document {
            self.cancel_reference_stretch();
            let before = self.tab.doc.clone();
            self.tab.doc = document.clone();
            // An imported file has no canvas of its own: it opens on the
            // remembered one, as a new drawing does. Other imports keep the tab's.
            match kind {
                Job::ImportFile(_) => self.tab.doc.canvas_theme = self.appearance.canvas_theme,
                Job::Import => self.tab.doc.canvas_theme = before.canvas_theme,
                _ => {}
            }
            if !self.adopt_changed_document(before) {
                return;
            }
            reshiki::atom_labels::copy_computed(&mut self.tab.doc, &document);
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
            self.tab.labels_dirty = false;
            self.tab.chemistry_notice = None;
            self.tab.selected.clear();
            if matches!(kind, Job::Import | Job::ImportFile(_)) {
                self.fit();
            }
            if let Job::ImportFile(path) = &kind {
                self.tab.keep_format = false;
                if !cfg!(target_arch = "wasm32") && super::files::kept_format(path).is_some() {
                    // A ChemDraw or MOL file opens as itself: unchanged, with
                    // nothing to undo, and Save can write it back.
                    self.tab.path = Some(path.clone());
                    self.tab.untitled_name = None;
                    self.tab.history = Default::default();
                    self.tab.mark_saved(self.tab.doc.clone());
                } else {
                    // Any other format, and every import in the browser, which
                    // has no paths, becomes an unsaved drawing with its name.
                    self.tab.path = None;
                    self.tab.untitled_name = Some(
                        path.file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy()
                            .into_owned(),
                    );
                    self.tab.mark_saved(Document::default());
                }
                self.begin_file_epoch();
            }
        }
        self.tab.analysis = whole_analysis(&self.tab.doc, response.analysis);
        self.status = match kind {
            Job::Clean(_) => "Structure cleaned",
            Job::Analyze => "No chemistry errors found",
            _ => "Structure imported",
        }
        .into();
        if !response.warnings.is_empty() {
            self.status.push_str(" · ");
            self.status.push_str(&response.warnings.join(" · "));
        }
        self.error = false;
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cancel_tests;

#[cfg(test)]
mod crystal_tests;

#[cfg(test)]
mod import_file_tests;

/// A job's analysis of the whole drawing, which counts Illustration
/// molecules; Properties then asks for the chemical part instead.
/// An engine analysis of the whole drawing, unless Properties must work it
/// out itself: without pictures, and without atom maps, which belong to a
/// reaction and not to a molecule's SMILES.
pub(super) fn whole_analysis(doc: &Document, analysis: Option<Analysis>) -> Option<Analysis> {
    analysis.filter(|_| !reshiki::purpose::used(doc) && doc.atoms.iter().all(|a| a.map_num == 0))
}

#[cfg(not(target_arch = "wasm32"))]
const EXPORT: &str = "Export…";

/// Lists, over the window, what a ChemDraw export leaves out or changes; true
/// goes on to choose where to save it.
#[cfg(not(target_arch = "wasm32"))]
fn read_losses(format: &'static str, losses: Vec<String>) -> Task<bool> {
    iced::window::latest().then(move |window| {
        let list: Vec<_> = losses.iter().map(|loss| format!("• {loss}")).collect();
        let dialog = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(format!("Export {}?", format.to_uppercase()))
            .set_description(format!(
                "What changes in this file:\n\n{}",
                list.join("\n\n")
            ))
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                EXPORT.into(),
                "Cancel".into(),
            ));
        match window {
            Some(id) => iced::window::run(id, move |window| dialog.set_parent(&window).show())
                .then(|answer| Task::perform(answer, goes_on)),
            None => Task::perform(dialog.show(), goes_on),
        }
    })
}

/// Windows and GTK answer Ok; macOS and Zenity name the button.
#[cfg(not(target_arch = "wasm32"))]
fn goes_on(answer: rfd::MessageDialogResult) -> bool {
    match answer {
        rfd::MessageDialogResult::Ok => true,
        rfd::MessageDialogResult::Custom(label) => label == EXPORT,
        _ => false,
    }
}
