use super::{App, Document, Job, Message, Point, Tool, editing, input_request};
use iced::{Subscription, Task, time::Instant, window};
use reshiki::clipboard::{CopyFormat, CopyOutcome, PasteOutcome, PreparedCopy};

// Plain text goes through iced's clipboard, or the page's in the browser,
// where reshiki::clipboard otherwise copies and pastes every format.
use iced::clipboard::read as read_text;
#[cfg(not(target_arch = "wasm32"))]
use iced::clipboard::write as write_text;

#[cfg(target_arch = "wasm32")]
fn write_text<T: 'static>(text: String) -> Task<T> {
    Task::future(crate::web::write_text(text)).discard()
}

#[derive(Debug, Clone)]
pub struct CopyAsKey {
    pub epoch: u64,
    pub revision: u64,
    pub format: CopyFormat,
    pub selected: Vec<u64>,
    /// What was copied, when it is not the selection or drawing.
    pub what: Option<String>,
}

#[derive(Default)]
pub(super) struct PdfCopy {
    serial: u64,
    pending: Option<PendingPdf>,
}

struct PendingPdf {
    serial: u64,
    tab: super::document_tab::TabId,
    key: CopyAsKey,
    snapshot: Document,
    window: Option<window::Id>,
    armed_at: Instant,
}

#[derive(Debug, Clone)]
pub enum FrameAction {
    Bound(u64, Option<window::Id>),
    Redrawn(u64, window::Id, Instant),
    Closed(u64, window::Id),
}

fn request_copy_frame() -> Task<Message> {
    iced_runtime::task::effect(iced_runtime::Action::Window(
        iced_runtime::window::Action::RedrawAll,
    ))
}

fn normalize_pasted_captions(doc: &mut Document) {
    for caption in &mut doc.annotations {
        let original = caption.text.clone();
        reshiki::typography::normalize_subscript_digits(&mut caption.text, &mut caption.format);
        if caption.text != original {
            for name in &mut doc.molecule_names {
                if name.annotation == caption.id && name.text == original {
                    name.text = caption.text.clone();
                }
            }
        }
    }
}

impl App {
    pub(super) fn clipboard_working(&self) -> bool {
        self.copy_as_busy
            || self.native_copy_busy
            || self.tab.clipboard_busy
            || self.tabs.background.iter().any(|tab| tab.clipboard_busy)
    }

    pub(super) fn copy_as(&mut self, format: CopyFormat) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        let snapshot = reshiki::clipboard::selection_or_drawing(&self.tab.doc, &self.tab.selected);
        let snapshot = if format.is_chemical() {
            match reshiki::clipboard::chemical_snapshot(&self.tab.doc, &snapshot) {
                Ok(std::borrow::Cow::Owned(chemical)) => chemical,
                Ok(std::borrow::Cow::Borrowed(_)) => snapshot,
                Err(reason) => {
                    self.status = reason.into();
                    self.error = true;
                    return Task::none();
                }
            }
        } else {
            snapshot
        };
        self.copy_as_snapshot(snapshot, format, false)
    }

    pub(super) fn copy_name(&mut self, name: String) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        let key = self.start_copy_as(CopyFormat::Name);
        Task::done(Message::CopyAsPrepared(
            key,
            Box::new(Ok(reshiki::clipboard::prepare_name(name))),
        ))
    }

    pub(super) fn copy_chemical_text(&mut self) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        let snapshot = reshiki::clipboard::selection_or_drawing(&self.tab.doc, &self.tab.selected);
        let (format, snapshot) =
            match reshiki::clipboard::chemical_text_snapshot(&self.tab.doc, &snapshot) {
                Ok(chemical) => chemical,
                Err(reason) => {
                    self.status = reason.into();
                    self.error = true;
                    return Task::none();
                }
            };
        self.copy_as_snapshot(snapshot, format, true)
    }

    /// Copy a drawing made for one purpose, such as a cycle step's
    /// reaction, as `format`; `what` names it in the status.
    pub(super) fn copy_made(
        &mut self,
        snapshot: Document,
        format: CopyFormat,
        what: String,
    ) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if let Some(reason) = format.unavailable_reason(&snapshot) {
            self.status = reason.into();
            self.error = true;
            return Task::none();
        }
        let mut key = self.start_copy_as(format);
        key.what = Some(what);
        self.queue_copy_as(key, snapshot, false)
    }

    fn copy_as_snapshot(
        &mut self,
        snapshot: Document,
        format: CopyFormat,
        chemical_text: bool,
    ) -> Task<Message> {
        if let Some(reason) = format.unavailable_reason_for_selection(&self.tab.doc, &snapshot) {
            self.status = reason.into();
            self.error = true;
            return Task::none();
        }
        let key = self.start_copy_as(format);
        self.queue_copy_as(key, snapshot, chemical_text)
    }

    fn queue_copy_as(
        &mut self,
        key: CopyAsKey,
        snapshot: Document,
        chemical_text: bool,
    ) -> Task<Message> {
        let format = key.format;
        if format == CopyFormat::Pdf {
            self.pdf_copy.serial = self.pdf_copy.serial.wrapping_add(1);
            let serial = self.pdf_copy.serial;
            let window = self.accessibility_window();
            self.pdf_copy.pending = Some(PendingPdf {
                serial,
                tab: self.tab.id,
                key,
                snapshot,
                window,
                armed_at: Instant::now(),
            });
            return if window.is_some() {
                request_copy_frame()
            } else {
                window::latest().map(move |id| Message::CopyFrame(FrameAction::Bound(serial, id)))
            };
        }
        self.prepare_copy_as(key, snapshot, chemical_text)
    }

    /// Admission and platform availability are checked before acquiring work.
    fn start_copy_as(&mut self, format: CopyFormat) -> CopyAsKey {
        self.copy_as_busy = true;
        self.tab.clipboard_busy = true;
        self.error = false;
        self.status = format!("Preparing {} for the clipboard…", format.label());
        CopyAsKey {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            format,
            selected: self.tab.selected.clone(),
            what: None,
        }
    }

    fn prepare_copy_as(
        &self,
        key: CopyAsKey,
        snapshot: Document,
        chemical_text: bool,
    ) -> Task<Message> {
        let format = key.format;
        let engine = self.engine.clone();
        let pictures = self.copy_pictures(None);
        Task::perform(
            async move {
                if chemical_text {
                    reshiki::clipboard::prepare_chemical_text(engine, snapshot, format).await
                } else {
                    reshiki::clipboard::prepare_as(engine, snapshot, format, pictures).await
                }
            },
            move |result| Message::CopyAsPrepared(key.clone(), Box::new(result)),
        )
    }

    pub(super) fn pdf_copy_subscription(&self) -> Subscription<Message> {
        let Some(pending) = self
            .pdf_copy
            .pending
            .as_ref()
            .filter(|p| p.window.is_some())
        else {
            return Subscription::none();
        };
        // `frames()` drops the window ID and `listen_with` excludes redraws.
        // Preserve the real event ID; only its owner's post-request draw may
        // release preparation. The runtime dispatches this message after the
        // current redraw handler has drawn and attempted presentation.
        iced::event::listen_raw(|event, _, id| match event {
            iced::Event::Window(window::Event::RedrawRequested(at)) => Some((id, Some(at))),
            iced::Event::Window(window::Event::Closed) => Some((id, None)),
            _ => None,
        })
        .with(pending.serial)
        .map(|(serial, (id, at))| {
            Message::CopyFrame(match at {
                Some(at) => FrameAction::Redrawn(serial, id, at),
                None => FrameAction::Closed(serial, id),
            })
        })
    }

    pub(super) fn copy_frame(&mut self, action: FrameAction) -> Task<Message> {
        let Some(pending) = self.pdf_copy.pending.as_mut() else {
            return Task::none();
        };
        match action {
            FrameAction::Bound(serial, window) if serial == pending.serial => {
                // Bind once; an old binding result cannot re-arm this request.
                if pending.window.is_some() {
                    return Task::none();
                }
                if let Some(window) = window {
                    pending.window = Some(window);
                    pending.armed_at = Instant::now();
                    return request_copy_frame();
                }
                self.cancel_pending_pdf("The drawing window is unavailable")
            }
            FrameAction::Closed(serial, window)
                if serial == pending.serial && pending.window == Some(window) =>
            {
                self.cancel_pending_pdf("The drawing window closed")
            }
            FrameAction::Redrawn(serial, window, at)
                if serial == pending.serial
                    && pending.window == Some(window)
                    && at > pending.armed_at =>
            {
                let Some(pending) = self.pdf_copy.pending.take() else {
                    return Task::none();
                };
                let task = self.in_tab(pending.tab, |app| {
                    if pending.key.epoch != app.tab.file_epoch
                        || pending.key.revision != app.tab.revision
                        || pending.key.selected != app.tab.selected
                    {
                        return app.copy_as_prepared(pending.key, Err(String::new()));
                    }
                    app.prepare_copy_as(pending.key, pending.snapshot, false)
                });
                match task {
                    Some(task) => super::tagged(task, pending.tab),
                    None => {
                        self.copy_as_busy = false;
                        Task::none()
                    }
                }
            }
            _ => Task::none(),
        }
    }

    fn cancel_pending_pdf(&mut self, reason: &str) -> Task<Message> {
        let Some(pending) = self.pdf_copy.pending.take() else {
            return Task::none();
        };
        let _ = self.in_tab(pending.tab, |app| {
            app.copy_as_prepared(pending.key, Err(reason.into()))
        });
        self.copy_as_busy = false;
        Task::none()
    }

    fn finish_copy_as(&mut self) {
        self.copy_as_busy = false;
        self.tab.clipboard_busy = false;
    }

    pub(super) fn copy_as_prepared(
        &mut self,
        key: CopyAsKey,
        result: Result<PreparedCopy, String>,
    ) -> Task<Message> {
        if key.epoch != self.tab.file_epoch {
            self.finish_copy_as();
            return Task::none();
        }
        if key.revision != self.tab.revision || key.selected != self.tab.selected {
            self.finish_copy_as();
            self.status = "Drawing changed · Copy again · Clipboard unchanged".into();
            self.error = false;
            return Task::none();
        }
        let copy = match result {
            Ok(copy) => copy,
            Err(error) => {
                self.finish_copy_as();
                // Engine errors carry generic Copy advice. Name the platform's
                // shortcut here, after the original snapshot checks have passed.
                let error = error
                    .strip_suffix(reshiki::exchange::drawing::TEXT_LABEL_COPY_ADVICE)
                    .map(|reason| {
                        format!(
                            "{reason}Try {} for an editable copy · Save as .rsk keeps everything",
                            super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "C")
                        )
                    })
                    .unwrap_or(error);
                self.status = format!(
                    "Could not copy {}: {error} · Clipboard unchanged",
                    key.format.label()
                );
                self.error = true;
                return Task::none();
            }
        };
        if !reshiki::clipboard::available() {
            let Some(text) = copy.text().map(str::to_owned) else {
                self.finish_copy_as();
                self.status = "Native clipboard is unavailable; use Export for this format".into();
                self.error = true;
                return Task::none();
            };
            return write_text(text)
                .chain(Task::done(Message::CopyAsWritten(key, Ok(copy.notices))));
        }
        Task::perform(reshiki::clipboard::write_prepared(copy), move |result| {
            Message::CopyAsWritten(key.clone(), result)
        })
    }

    pub(super) fn copy_as_written(&mut self, key: CopyAsKey, result: Result<Vec<String>, String>) {
        self.finish_copy_as();
        // A document replacement cannot receive an old operation's receipt.
        if key.epoch != self.tab.file_epoch {
            return;
        }
        match result {
            Ok(notices) => {
                let scope = key.what.as_deref().unwrap_or(
                    match (
                        !key.selected.is_empty(),
                        key.revision == self.tab.revision && key.selected == self.tab.selected,
                    ) {
                        (true, true) => "Selection",
                        (false, true) => "Drawing",
                        (true, false) => "Previous selection",
                        (false, false) => "Previous drawing snapshot",
                    },
                );
                self.status = format!("{scope} copied as {}", key.format.label());
                if !notices.is_empty() {
                    self.status.push_str(" · Review details\n");
                    self.status.push_str(&notices.join("\n"));
                }
                self.error = false;
            }
            Err(error) => {
                self.status = format!(
                    "Could not write {} to the clipboard: {error}",
                    key.format.label()
                );
                self.error = true;
            }
        }
    }

    /// How one copy's pictures look: on the Copy defaults, or on `once`'s
    /// background and editable drawing choice.
    pub(super) fn copy_pictures(
        &self,
        once: Option<(reshiki::export::Background, bool)>,
    ) -> reshiki::clipboard::Pictures {
        let a = &self.appearance;
        let (background, editable) =
            once.unwrap_or_else(|| (a.copy_paper(), a.export_editable_drawing));
        reshiki::clipboard::Pictures {
            background,
            dpi: a.copy_dpi.get(),
            editable,
        }
    }

    /// Copy, or Copy image when `image_only`; `once` overrides the Copy
    /// background and editable drawing defaults for this copy only.
    pub(super) fn copy_native(
        &mut self,
        cut: bool,
        image_only: bool,
        once: Option<(reshiki::export::Background, bool)>,
    ) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if self.tab.selected.is_empty() && !image_only {
            self.status = "Select objects to copy".into();
            return Task::none();
        }
        let snapshot = if self.tab.selected.is_empty() {
            self.tab.doc.clone()
        } else {
            editing::selection(&self.tab.doc, &self.tab.selected)
        };
        let cut_ids = if cut {
            reshiki::shape_text::with_texts(
                &self.tab.doc,
                &reshiki::arrow_labels::with_labels(&self.tab.doc, &self.tab.selected),
            )
        } else {
            vec![]
        };
        let (epoch, revision) = (self.tab.file_epoch, self.tab.revision);
        self.native_copy_busy = true;
        self.tab.clipboard_busy = true;
        self.error = false;
        self.status = "Preparing clipboard…".into();
        Task::perform(
            reshiki::clipboard::copy(
                self.engine.clone(),
                snapshot,
                image_only,
                self.copy_pictures(once),
            ),
            move |result| Message::ClipboardWritten {
                epoch,
                revision,
                cut_ids: cut_ids.clone(),
                result,
            },
        )
    }

    pub(super) fn paste_native(&mut self, image_only: bool) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        self.tab.clipboard_busy = true;
        self.error = false;
        self.status = "Reading clipboard…".into();
        let (epoch, revision) = (self.tab.file_epoch, self.tab.revision);
        Task::perform(
            reshiki::clipboard::paste_with_warnings(self.engine.clone(), image_only),
            move |result| Message::ClipboardRead {
                epoch,
                revision,
                result: Box::new(result),
            },
        )
    }

    pub(super) fn clipboard_written(
        &mut self,
        epoch: u64,
        revision: u64,
        cut_ids: Vec<u64>,
        result: Result<CopyOutcome, String>,
    ) {
        self.native_copy_busy = false;
        self.tab.clipboard_busy = false;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.error = true;
                self.status = format!("Could not copy: {error}");
                return;
            }
        };
        let cut = !cut_ids.is_empty();
        let stale_cut = cut && (epoch != self.tab.file_epoch || revision != self.tab.revision);
        if cut && !stale_cut {
            let before = self.tab.doc.clone();
            self.tab.doc.delete(&cut_ids);
            self.tab.selected.retain(|id| !cut_ids.contains(id));
            self.changed(before);
            self.sync_typography();
            self.sync_graphics();
            self.sync_arrows();
            self.sync_bonds();
        }
        let action = if stale_cut {
            "Drawing changed · Cut canceled · Previous selection copied"
        } else if cut {
            if cfg!(windows) {
                "Selection cut · editable drawing copied"
            } else {
                "Selection cut · editable drawing and images copied"
            }
        } else if outcome.image_only {
            "Image copied"
        } else if cfg!(windows) && outcome.external_editable {
            "Copied editable drawing · use Copy Image for a picture"
        } else if cfg!(windows) {
            "Copied · editable in ReShiki; use Copy Image for other apps"
        } else if outcome.external_editable {
            "Editable drawing and images copied"
        } else {
            "Copied · editable in ReShiki; picture in other apps"
        };
        self.status = if outcome.notices.is_empty() {
            action.to_owned()
        } else {
            format!("{action} · Review details\n{}", outcome.notices.join("\n"))
        };
        // A successfully written clipboard may have format limitations. Keep
        // their details available without presenting a successful copy as failure.
        self.error = false;
    }

    pub(super) fn clipboard_read(
        &mut self,
        epoch: u64,
        revision: u64,
        result: Result<PasteOutcome, String>,
    ) {
        self.tab.clipboard_busy = false;
        if epoch != self.tab.file_epoch
            || revision != self.tab.revision
            || self.tab.inline_text.is_some()
            || self.tab.joining.is_some()
        {
            self.status = "Drawing changed during paste · Paste again".into();
            return;
        }
        let outcome = match result.and_then(|mut outcome| {
            outcome.document.validate()?;
            normalize_pasted_captions(&mut outcome.document);
            Ok(outcome)
        }) {
            Ok(doc) => doc,
            Err(error) => {
                self.status = format!("Could not paste: {error}");
                self.error = true;
                return;
            }
        };
        let part = if outcome.native {
            reshiki::canvas_theme::for_native_paste(outcome.document, self.tab.doc.canvas_theme)
        } else {
            // Another program's structure has no label settings of its own:
            // it follows this drawing's instead of overriding them per atom.
            let mut part =
                reshiki::canvas_theme::for_paste(outcome.document, self.tab.doc.canvas_theme);
            part.atom_labels = self.tab.doc.atom_labels.clone();
            part
        };
        let known: std::collections::HashSet<u64> =
            self.tab.doc.graphics.iter().map(|g| g.id).collect();
        if !self.append_pasted(&part) {
            self.status = "Could not insert the clipboard drawing".into();
            self.error = true;
            return;
        }
        self.tool = Tool::Select;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        if part.atoms.is_empty()
            && part.annotations.is_empty()
            && part.arrows.is_empty()
            && part.graphics.iter().all(|g| g.picture.is_some())
        {
            // Only the pictures just pasted: pasted into a panel, the
            // selection is the panel's whole group.
            if let Some((lo, hi)) = self
                .tab
                .doc
                .graphics
                .iter()
                .filter(|g| g.picture.is_some() && !known.contains(&g.id))
                .map(|g| g.bounds())
                .reduce(|(a, b), (c, d)| {
                    (
                        Point::new(a.x.min(c.x), a.y.min(c.y)),
                        Point::new(b.x.max(d.x), b.y.max(d.y)),
                    )
                })
            {
                self.reveal_bounds(lo, hi);
            }
            self.status = "Picture pasted · Drag the corner handles to resize".into();
        } else {
            self.status = "Editable drawing pasted".into();
        }
        if !outcome.warnings.is_empty() {
            self.status.push_str(" · ");
            self.status.push_str(&outcome.warnings.join(" · "));
        }
    }

    /// Adds a pasted `part`, selects it and records the edit; false when
    /// nothing could be added. A small part lands just off the view's centre.
    /// One wider than a quarter of the view would cover what is there, so it
    /// lands one bond length right of the drawing and is revealed, as an
    /// inserted structure is. Pages keep the small offset: right of the
    /// drawing may be off the page. Into one selected empty panel, such as
    /// a cleared one, the part lands in the panel's middle.
    pub(super) fn append_pasted(&mut self, part: &Document) -> bool {
        let before = self.tab.doc.clone();
        let center = editing::center(part, &part.all_ids());
        let view = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let edges = reshiki::scene::selection_bounds;
        let panel = match self.tab.selected.as_slice() {
            [id] => before
                .graphics
                .iter()
                .find(|g| g.id == *id && g.panel.is_some())
                .filter(|g| reshiki::graphics::panel::contents(&before, g.id).is_empty())
                .map(|g| {
                    let (lo, hi) = g.shape_bounds();
                    (g.id, Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.))
                }),
            _ => None,
        };
        let wide = panel.is_none()
            && self.tab.doc.page_layout.is_none()
            && edges(part, &part.all_ids())
                .is_some_and(|(lo, hi)| (hi.x - lo.x) * self.tab.camera.zoom > view.width / 4.);
        let mut offset = match panel {
            Some((_, middle)) => Point::new(middle.x - center.x, middle.y - center.y),
            None => Point::new(
                self.tab.camera.center.x - center.x + 24.,
                self.tab.camera.center.y - center.y + 24.,
            ),
        };
        // A wide part is measured on its own in this drawing's settings, with
        // the hydrogens it will be drawn with, so that the visible edges end
        // one bond length apart. `Document::bounds` would pad them for framing.
        let mut beside = false;
        if wide && let Some((_, drawing)) = edges(&before, &before.all_ids()) {
            let mut blank = before.clone();
            blank.clear_content();
            let mut alone = blank.clone();
            let ids = editing::append_preserving_layout(&mut alone, part, Point::default());
            reshiki::atom_labels::refresh::refresh_hydrogens(&blank, &mut alone);
            if let Some((lo, _)) = edges(&alone, &ids) {
                offset = Point::new(
                    drawing.x + before.drawing_style.bond_length_world - lo.x,
                    self.tab.camera.center.y - center.y,
                );
                beside = true;
            }
        }
        let mut selected = editing::append_preserving_layout(&mut self.tab.doc, part, offset);
        if selected.is_empty() || self.tab.doc.validate().is_err() {
            self.tab.doc = before;
            return false;
        }
        // What fills a panel moves with it, as wrapped contents do.
        if let Some((id, _)) = panel {
            selected.push(id);
            if let Ok(grouped) = self.tab.doc.group_selection(&selected) {
                selected = grouped;
            }
        }
        if beside {
            self.commit_structure_insert(before, selected);
        } else {
            self.tab.selected = selected;
            if !self.adopt_changed_document(before) {
                return false;
            }
        }
        true
    }

    pub(super) fn copy_selection(&mut self, cut: bool) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if reshiki::clipboard::available() {
            return self.copy_native(cut, false, None);
        }
        if self.tab.selected.is_empty() {
            self.status = "Select objects to copy".into();
            return Task::none();
        }
        let selection = editing::selection(&self.tab.doc, &self.tab.selected);
        let Ok(json) = serde_json::to_string(&selection.current()) else {
            return Task::none();
        };
        let text = format!("{}{json}", editing::CLIPBOARD_PREFIX);
        if cut {
            let before = self.tab.doc.clone();
            let ids = reshiki::shape_text::with_texts(
                &self.tab.doc,
                &reshiki::arrow_labels::with_labels(&self.tab.doc, &self.tab.selected),
            );
            self.tab.doc.delete(&ids);
            self.tab.selected.clear();
            self.changed(before);
        }
        self.status = if cut {
            "Selection cut"
        } else {
            "Selection copied"
        }
        .into();
        write_text(text)
    }

    pub(super) fn paste_clipboard(&mut self) -> Task<Message> {
        if reshiki::clipboard::available() {
            return self.paste_native(false);
        }
        read_text().map(Message::Pasted)
    }

    pub(super) fn copy_smiles(&mut self) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if let Some(a) = self.property_analysis() {
            let text = a.smiles.clone();
            if let Some(notice) = a.smiles_notice.clone() {
                self.status = notice;
                self.error = false;
            }
            return write_text(text);
        }
        Task::none()
    }

    pub(super) fn copy_analysis(&mut self) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if let Some(analysis) = self
            .property_analysis()
            .and_then(|a| self.elemental_analysis(a))
        {
            // Journal text uses plain digits for counts: C2H6O.
            return write_text(
                analysis
                    .chars()
                    .map(|c| match c {
                        '₀'..='₉' => char::from_digit(c as u32 - '₀' as u32, 10).unwrap_or(c),
                        _ => c,
                    })
                    .collect(),
            );
        }
        Task::none()
    }

    pub(super) fn pasted(&mut self, contents: Option<String>) -> Task<Message> {
        if let Some(contents) = contents.filter(|s| !s.trim().is_empty()) {
            if let Some(json) = editing::clipboard_json(&contents) {
                match Document::from_json(json.as_bytes()) {
                    Ok(mut part) => {
                        normalize_pasted_captions(&mut part);
                        let part = reshiki::canvas_theme::for_native_paste(
                            part,
                            self.tab.doc.canvas_theme,
                        );
                        self.append_pasted(&part);
                        self.tool = Tool::Select;
                        self.status = "Selection pasted".into();
                    }
                    Err(e) => {
                        self.status = format!("Could not paste: {e}");
                        self.error = true;
                    }
                }
            } else {
                return self.run(input_request(&contents), Job::Insert);
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod latency_qa;

#[cfg(test)]
mod frame_tests;
