//! The xTB results palette: docked under the status bar like NMR (or in the
//! inspector), with Summary, Steps and Log, the stale banner, energy labels,
//! "Saved in file" and the large-results question on Save.
use super::results::{self, RunInfo, View};
use super::{CalcTask, Request, state};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{self, CalcResult, Frame};
use reshiki::science::{self, Freshness, Science, Stored};
use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

#[derive(Clone, Debug)]
pub(crate) enum Action {
    /// Open the palette on the newest result of the card's molecule.
    Show,
    Close,
    Collapse,
    /// Move between the bottom dock and the inspector.
    Dock,
    View(View),
    /// Preview this optimization step on the 3D model.
    Step(usize),
    /// Back to the result's own geometry.
    StepBack,
    /// "Use this geometry": the previewed step, as one Undo step.
    UseStep,
    UseStepAnyway,
    KeepModel,
    CopyTable,
    Export,
    Exported(Result<Option<std::path::PathBuf>, String>),
    CopyLog,
    Labels(bool),
    Recalculate,
    KeepFull(bool),
    /// Save as `name` after the large-results question.
    SaveNamed(String),
}

/// The window's palette placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Dock {
    pub inspector: bool,
    pub collapsed: bool,
}

/// One document's palette.
#[derive(Default)]
pub(crate) struct Palette {
    /// The result shown: an identity that Undo, Redo and other results'
    /// edits keep, unlike an index or an attachment ID.
    result: Option<Weak<CalcResult>>,
    pub(super) open: bool,
    pub(super) view: View,
    pub(super) labels: bool,
    step: Option<Step>,
    /// Why the previewed step was not used: the stereo check's findings.
    confirm: Vec<String>,
    /// Save asked about large results once for this document.
    size_asked: bool,
    /// File sizes for one document state; serializing is too slow per frame.
    sizes: RefCell<Option<Sizes>>,
}

struct Step {
    index: usize,
    /// The document state the preview belongs to; any edit ends it.
    revision: (u64, u64),
    /// The document with the model at this step, when the step's geometry
    /// is known.
    preview: Option<Box<Document>>,
}

struct Sizes {
    /// The calculation data measured (cheap to compare: payloads are
    /// shared) and the shown result's index; drawing edits keep it.
    science: Science,
    index: usize,
    /// Saved bytes per result, in `Science::results` order.
    saved: Vec<usize>,
    /// Extra bytes of the shown result's full data.
    full: usize,
}

impl Palette {
    /// Open on `result`'s Summary.
    pub(super) fn show(&mut self, result: &Arc<CalcResult>) {
        self.result = Some(Arc::downgrade(result));
        self.open = true;
        self.view = View::Summary;
        self.step = None;
        self.confirm.clear();
    }
}

/// The result the palette shows and what goes with it.
pub(super) struct Shown<'a> {
    pub index: usize,
    pub stored: &'a Stored,
    pub input: &'a Arc<Frame>,
    pub frame: &'a Arc<Frame>,
    pub atoms: &'a [Option<u64>],
    pub stale: bool,
    pub info: Option<&'a RunInfo>,
}

impl Shown<'_> {
    fn method(&self) -> String {
        results::method_line(&self.stored.result, self.frame, self.info)
    }

    fn rows(&self) -> Vec<(&'static str, String)> {
        results::summary_rows(&self.stored.result, self.input, self.frame, self.info)
    }

    fn step_positions(&self, step: usize) -> Option<Vec<[f64; 3]>> {
        results::step_positions(&self.stored.result, step, self.input, self.frame)
    }
}

impl App {
    /// The result the palette follows, while the document holds it.
    pub(super) fn xtb_shown(&self) -> Option<Shown<'_>> {
        let result = self.tab.xtb.palette.result.as_ref()?.upgrade()?;
        let science = &self.tab.doc.science;
        let (index, stored) = science
            .results
            .iter()
            .enumerate()
            .find(|(_, s)| Arc::ptr_eq(&s.result, &result))?;
        let link = science.attachment(stored.attachment)?;
        Some(Shown {
            index,
            stored,
            input: science.frame(&link.frame)?,
            frame: science.frame(&stored.result.frame)?,
            atoms: &link.atoms,
            stale: link.freshness(&self.tab.doc) == Freshness::Stale,
            info: self.tab.xtb.info(&stored.result),
        })
    }

    /// The palette is showing a result.
    pub(super) fn xtb_palette_visible(&self) -> bool {
        self.tab.xtb.palette.open && self.xtb_shown().is_some()
    }

    /// The step preview, while the document has not changed since.
    pub(super) fn xtb_step_preview(&self) -> Option<&Document> {
        let step = self.tab.xtb.palette.step.as_ref()?;
        (step.revision == (self.tab.file_epoch, self.tab.revision))
            .then_some(step.preview.as_deref())
            .flatten()
    }

    /// The selected step, while it is current.
    fn xtb_step(&self) -> Option<usize> {
        let step = self.tab.xtb.palette.step.as_ref()?;
        (step.revision == (self.tab.file_epoch, self.tab.revision)).then_some(step.index)
    }

    /// Esc ends a step preview first.
    pub(in crate::app) fn xtb_end_preview(&mut self) -> bool {
        let previewing = self.xtb_step().is_some();
        self.tab.xtb.palette.step = None;
        self.tab.xtb.palette.confirm.clear();
        previewing
    }

    /// "Saved in file" for each result, and the shown result's full extra,
    /// measured again only when the calculation data or the shown result
    /// change.
    fn xtb_sizes(&self, shown: &Shown<'_>) -> (usize, usize) {
        let science = &self.tab.doc.science;
        let mut cache = self.tab.xtb.palette.sizes.borrow_mut();
        let sizes = match cache.take() {
            Some(sizes) if sizes.index == shown.index && sizes.science == *science => {
                cache.insert(sizes)
            }
            _ => cache.insert(Sizes {
                science: science.clone(),
                index: shown.index,
                saved: science.saved_sizes(),
                full: shown.stored.result.heavy_bytes(),
            }),
        };
        (
            sizes.saved.get(shown.index).copied().unwrap_or_default(),
            sizes.full,
        )
    }

    /// The canvas label for "Show as labels": the energy under the molecule.
    pub(in crate::app) fn xtb_label(&self) -> Option<crate::canvas::ResultLabel> {
        if !self.tab.xtb.palette.labels || !self.xtb_palette_visible() {
            return None;
        }
        let shown = self.xtb_shown().filter(|shown| !shown.stale)?;
        let energy = shown.stored.result.energy_hartree?;
        Some(crate::canvas::ResultLabel {
            atoms: shown.atoms.iter().flatten().copied().collect(),
            text: format!("E {} Eh", state::signed(energy, 6)),
        })
    }

    pub(super) fn xtb_palette(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show => {
                let newest = self.xtb_target().and_then(|atoms| {
                    let atom = *atoms.first()?;
                    let views = science::results_for(&self.tab.doc, atom);
                    views.last().map(|view| view.stored.result.clone())
                });
                let shown = self.xtb_shown().map(|shown| shown.stored.result.clone());
                if let Some(result) = newest.or(shown) {
                    self.tab.xtb.palette.show(&result);
                }
            }
            Action::Close => {
                self.tab.xtb.palette.open = false;
                self.xtb_end_preview();
            }
            Action::Collapse => self.xtb.dock.collapsed = !self.xtb.dock.collapsed,
            Action::Dock => {
                let old_width = self.inspector_width();
                self.xtb.dock.inspector = !self.xtb.dock.inspector;
                if self.xtb.dock.inspector {
                    self.inspector_open = true;
                    self.inspector_tab = crate::app::InspectorTab::Predict;
                    self.xtb.page = super::Page::Xtb;
                }
                self.expect_inspector_width_change(old_width);
            }
            Action::View(view) => {
                self.tab.xtb.palette.view = view;
                if view != View::Steps {
                    self.xtb_end_preview();
                }
            }
            Action::Step(index) => self.xtb_preview_step(index),
            Action::StepBack => {
                self.xtb_end_preview();
            }
            Action::UseStep => self.xtb_use_step(false),
            Action::UseStepAnyway => self.xtb_use_step(true),
            Action::KeepModel => {
                self.xtb_end_preview();
                self.status = "Step geometry not used".into();
                self.error = false;
            }
            Action::CopyTable | Action::Export | Action::CopyLog => {
                let Some(shown) = self.xtb_shown() else {
                    return Task::none();
                };
                let method = shown.method();
                if matches!(action, Action::CopyLog) {
                    let log = results::log(&shown.stored.result, shown.frame, shown.info, &method);
                    self.status = "Copied the xTB log".into();
                    self.error = false;
                    return iced::clipboard::write(log);
                }
                let rows = shown.rows();
                let table = results::tsv(&method, &rows);
                if matches!(action, Action::CopyTable) {
                    self.status = format!("Copied {} rows of xTB results", rows.len());
                    self.error = false;
                    return iced::clipboard::write(table);
                }
                let stem = std::path::Path::new(&self.document_name())
                    .file_stem()
                    .map(|stem| format!("{} xTB results", stem.to_string_lossy()));
                return self.export_ready(
                    crate::app::figure_export::Prepared::new(
                        table.into_bytes(),
                        "tsv",
                        stem,
                        vec![],
                        crate::app::figure_export::Kind::Xtb,
                    ),
                    cfg!(target_arch = "wasm32"),
                );
            }
            Action::Exported(result) => match result {
                Ok(Some(path)) => {
                    self.status = format!("Exported xTB results to {}", path.display());
                    self.error = false;
                }
                Ok(None) => {}
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            },
            Action::Labels(on) => self.tab.xtb.palette.labels = on,
            Action::Recalculate => return self.xtb_recalculate(),
            Action::KeepFull(keep) => {
                let Some(index) = self.xtb_shown().map(|shown| shown.index) else {
                    return Task::none();
                };
                let mut doc = self.tab.doc.clone();
                if science::set_keep_full(&mut doc, index, keep) {
                    let before = std::mem::replace(&mut self.tab.doc, doc);
                    self.changed(before);
                    self.status = if keep {
                        "Full results will be saved in this file".into()
                    } else {
                        "Only the light results will be saved in this file".into()
                    };
                    self.error = false;
                }
            }
            Action::SaveNamed(name) => return self.save_drawing_named(name),
        }
        Task::none()
    }

    fn xtb_preview_step(&mut self, index: usize) {
        let Some(shown) = self.xtb_shown() else {
            return;
        };
        if shown.stale || self.tab.xtb.run.is_some() {
            return;
        }
        let count = results::energies(&shown.stored.result).len();
        if index >= count {
            return;
        }
        let preview = shown.step_positions(index).and_then(|positions| {
            let mut doc = self.tab.doc.clone();
            let frame = Frame {
                positions_angstrom: positions,
                ..(**shown.frame).clone()
            };
            science::move_model(&mut doc, shown.atoms, &frame).ok()?;
            // A model shown as 2D only appears beside the drawing.
            let atom = shown.atoms.iter().flatten().next()?;
            let model = doc
                .models
                .iter()
                .position(|m| m.linked_atoms.contains(atom))?;
            if doc.models[model].display == reshiki::models3d::Display::TwoD {
                reshiki::models3d::set_display(&mut doc, model, reshiki::models3d::Display::Both);
            }
            Some(doc)
        });
        self.status = if preview.is_some() {
            format!(
                "Previewing step {index} of {} on the 3D model · Esc returns to the result",
                count - 1
            )
        } else {
            format!(
                "Step {index} is not saved in the file; only the first and last steps are. Recalculate to see every step."
            )
        };
        self.error = false;
        self.tab.xtb.palette.confirm.clear();
        self.tab.xtb.palette.step = Some(Step {
            index,
            revision: (self.tab.file_epoch, self.tab.revision),
            preview: preview.map(Box::new),
        });
    }

    /// Move the 3D model to the previewed step as one Undo step, after the
    /// same stereo check as an optimization unless `forced`.
    fn xtb_use_step(&mut self, forced: bool) {
        let Some(index) = self.xtb_step() else {
            return;
        };
        if self.tab.drawing_edit_active() || self.xtb_running() {
            self.status = "Finish the current operation, then use the step".into();
            self.error = false;
            return;
        }
        let Some(shown) = self.xtb_shown().filter(|shown| !shown.stale) else {
            return;
        };
        let Some(positions) = shown.step_positions(index) else {
            return;
        };
        let atoms = shown.atoms.to_vec();
        let frame = Frame {
            positions_angstrom: positions,
            ..(**shown.frame).clone()
        };
        if !forced {
            let reasons = match science::stereo_changes(&self.tab.doc, &atoms, &frame) {
                Ok(changes) => changes
                    .iter()
                    .map(|change| {
                        state::describe(change, |id| self.xtb_atom_name(&atoms, id)).replacen(
                            "Optimization changed",
                            &format!("Step {index} changes"),
                            1,
                        )
                    })
                    .collect(),
                Err(error) => vec![format!("The stereo check could not run: {error}.")],
            };
            if let Some(first) = reasons.first() {
                self.status = format!("{} · nothing applied", first.trim_end_matches('.'));
                self.error = false;
                self.tab.xtb.palette.confirm = reasons;
                return;
            }
        }
        let mut doc = self.tab.doc.clone();
        if let Err(error) = science::move_model(&mut doc, &atoms, &frame) {
            self.status = format!("{error}; nothing applied");
            self.error = true;
            return;
        }
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
        if !self.error {
            self.status = format!("Applied xTB geometry of step {index} · ⌘Z undoes");
        }
        self.tab.xtb.palette.step = None;
        self.tab.xtb.palette.confirm.clear();
    }

    /// Run the shown result's calculation again on the molecule as it is now.
    fn xtb_recalculate(&mut self) -> Task<Message> {
        if !self.xtb_can_start() {
            return Task::none();
        }
        let Some(shown) = self.xtb_shown() else {
            return Task::none();
        };
        let result = &shown.stored.result;
        let Some(first) = shown
            .atoms
            .iter()
            .flatten()
            .copied()
            .find(|id| self.tab.doc.atom(*id).is_some())
        else {
            self.status = "The molecule was deleted".into();
            self.error = true;
            return Task::none();
        };
        let solvent = match &result.settings {
            qcdata::Settings::Xtb(settings) => settings.solvent.as_ref().and_then(|s| {
                state::SOLVENTS
                    .into_iter()
                    .find(|name| name.eq_ignore_ascii_case(&s.name))
            }),
            qcdata::Settings::Imported { .. } => None,
        };
        let info = shown.info;
        let request = Request {
            atoms: reshiki::molecule_names::component(&self.tab.doc, first),
            task: if result.task == qcdata::Task::Optimize {
                CalcTask::Optimize
            } else {
                CalcTask::Energy
            },
            charge: shown.frame.charge,
            unpaired: shown.frame.multiplicity.saturating_sub(1),
            solvent,
            threads: info.map_or_else(|| self.xtb.settings.threads(), |info| info.threads),
            tight: info.is_some_and(|info| info.tight),
            fixed: info.map_or_else(Vec::new, |info| {
                info.fixed
                    .iter()
                    .copied()
                    .filter(|id| self.tab.doc.atom(*id).is_some())
                    .collect()
            }),
            keep_full: shown.stored.keep_full,
        };
        self.xtb_end_preview();
        self.xtb_begin(request)
    }

    /// Save asks once per document when its results take more than
    /// [`science::LIGHTER_BYTES`]; `then` is the save that goes on after.
    pub(in crate::app) fn xtb_size_question(&mut self, then: Message) -> bool {
        if self.tab.xtb.palette.size_asked {
            return false;
        }
        let bytes = self.tab.doc.science.file_bytes();
        if bytes <= science::LIGHTER_BYTES {
            return false;
        }
        self.tab.xtb.palette.size_asked = true;
        let mut lighter = Document {
            science: self.tab.doc.science.clone(),
            ..Document::default()
        };
        let results = science::make_lighter(&mut lighter);
        self.prompt = Some(crate::app::prompt::Prompt::LargeResults {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            bytes,
            lighter: (results > 0).then(|| (results, lighter.science.file_bytes())),
            then: Box::new(then),
        });
        true
    }
}

impl App {
    /// The large-results question's answer, for the document it asked
    /// about: Make lighter changes it first (one Undo step), then the save
    /// goes on. A different document in front cancels the save.
    pub(in crate::app) fn xtb_size_answer(
        &mut self,
        prompt: crate::app::prompt::Prompt,
        answer: crate::app::prompt::Answer,
    ) -> Message {
        use crate::app::prompt::{Answer, Prompt};
        let Prompt::LargeResults {
            tab,
            epoch,
            lighter,
            ..
        } = &prompt
        else {
            return prompt.answer(answer);
        };
        if (*tab, *epoch) != (self.tab.id, self.tab.file_epoch) {
            return Message::Cancel;
        }
        if answer == Answer::Save && lighter.is_some() {
            self.xtb_make_lighter();
        }
        prompt.answer(answer)
    }

    /// "Make lighter" as one Undo step; the palette and run details follow
    /// the lightened results.
    fn xtb_make_lighter(&mut self) {
        let mut doc = self.tab.doc.clone();
        if science::make_lighter(&mut doc) == 0 {
            return;
        }
        for (old, new) in self
            .tab
            .doc
            .science
            .results
            .iter()
            .zip(&doc.science.results)
        {
            if Arc::ptr_eq(&old.result, &new.result) {
                continue;
            }
            if let Some(info) = self.tab.xtb.info(&old.result).cloned() {
                self.tab.xtb.remember(&new.result, Arc::new(info));
            }
            let shown = self.tab.xtb.palette.result.as_ref();
            if shown.is_some_and(|weak| weak.as_ptr() == Arc::as_ptr(&old.result)) {
                self.tab.xtb.palette.result = Some(Arc::downgrade(&new.result));
            }
        }
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
    }
}

/// The prompt's text for results of `bytes`, and what Make lighter leaves.
pub(in crate::app) fn size_question(
    bytes: usize,
    lighter: Option<(usize, usize)>,
) -> (String, String) {
    let title = format!(
        "Calculation results in this document take {}",
        results::size(bytes)
    );
    let text = match lighter {
        Some((count, after)) => format!(
            "Make lighter drops the full data you chose to keep for {count} result{} (optimization steps, gradients), leaving {} of results. Energies and final geometries stay; data imported from other programs stays.",
            if count == 1 { "" } else { "s" },
            results::size(after)
        ),
        None => "The results are already light: data imported from other programs cannot be recomputed, so it stays.".into(),
    };
    (title, text)
}

mod view;

#[cfg(test)]
pub(in crate::app) mod tests;
