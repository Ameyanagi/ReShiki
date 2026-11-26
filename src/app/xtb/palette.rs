//! The xTB results palette: docked under the status bar like NMR (or in the
//! inspector), with result tabs, the stale banner, energy labels,
//! "Saved in file" and the large-results question on Save.
use super::results::{self, RunInfo, View};
use super::{Request, is, state, ts};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Frame, Spin};
use reshiki::science::{self, Freshness, Stored};
use std::{
    cell::RefCell,
    collections::HashMap,
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
    IrExported(Result<Option<std::path::PathBuf>, String>),
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
    /// The result shown, and the lighter copies Make lighter made of it:
    /// identities that Undo, Redo and other results' edits keep, unlike an
    /// index or an attachment ID.
    result: Vec<Weak<CalcResult>>,
    pub(super) open: bool,
    pub(super) view: View,
    pub(super) labels: bool,
    step: Option<Step>,
    ts_views: Vec<TsView>,
    /// Why the previewed step was not used: the stereo check's findings.
    pub(super) confirm: Vec<String>,
    /// The Conformers tab's overlay choice and its table.
    pub(super) ensemble: super::conformers::Ensemble,
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

struct TsView {
    result: Vec<Weak<CalcResult>>,
    view: View,
    step: Option<(usize, (u64, u64))>,
}

struct Sizes {
    /// The results measured, by identity and "keep full" choice, without
    /// holding them, and the attachments (imported results share a size
    /// budget per molecule).
    results: Vec<(Weak<CalcResult>, bool)>,
    attachments: Vec<science::Attachment>,
    pins: HashMap<String, Vec<(Spin, u32)>>,
    index: usize,
    /// Saved bytes per result, in `Science::results` order.
    saved: Vec<usize>,
    /// Extra bytes of the shown result's full data.
    full: usize,
}

impl Palette {
    /// The shown result's identity, to notice another result shown.
    pub(super) fn shown_id(&self) -> usize {
        self.result.first().map_or(0, |weak| weak.as_ptr() as usize)
    }

    fn clear_result(&mut self) {
        self.ts_views
            .retain(|view| view.result.iter().any(|weak| weak.strong_count() > 0));
        if self.is_showing_ts() {
            self.ts_views.push(TsView {
                result: std::mem::take(&mut self.result),
                view: self.view,
                step: self.step.as_ref().map(|step| (step.index, step.revision)),
            });
        }
        self.result.clear();
        self.view = View::Summary;
        self.step = None;
        self.confirm.clear();
    }

    fn is_showing_ts(&self) -> bool {
        self.result
            .iter()
            .filter_map(Weak::upgrade)
            .any(|result| ts::is_ts(&result))
    }

    fn follows(&self, result: &Arc<CalcResult>) -> bool {
        self.result.iter().any(|weak| is(weak, result))
    }

    /// Open the result, restoring a reaction's last tab and path selection.
    pub(super) fn show(&mut self, result: &Arc<CalcResult>) -> bool {
        self.open = true;
        let same = self.follows(result);
        if same && ts::is_ts(result) {
            return false;
        }
        if !same {
            self.clear_result();
            if let Some(index) = self
                .ts_views
                .iter()
                .position(|view| view.result.iter().any(|weak| is(weak, result)))
            {
                let saved = self.ts_views.swap_remove(index);
                self.result = saved.result;
                self.view = saved.view;
                self.step = saved.step.map(|(index, revision)| Step {
                    index,
                    revision,
                    preview: None,
                });
                return self.step.is_some();
            }
            self.result = vec![Arc::downgrade(result)];
        }
        self.view = if ts::is_ts(result) {
            View::Path
        } else if super::conformers::is_search(result) {
            View::Conformers
        } else {
            View::Summary
        };
        self.step = None;
        self.confirm.clear();
        false
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
    pub arrow: Option<u64>,
    pub info: Option<&'a RunInfo>,
    pub ts: Option<&'a ts::Session>,
}

impl Shown<'_> {
    pub(super) fn method(&self) -> String {
        results::method_line(&self.stored.result, self.frame)
    }

    pub(super) fn rows(&self) -> Vec<(String, String)> {
        results::summary_rows(&self.stored.result, self.input, self.frame, self.info)
    }

    pub(super) fn step_positions(&self, step: usize) -> Option<Vec<[f64; 3]>> {
        if ts::is_ts(&self.stored.result) {
            return self.ts?.image(step);
        }
        results::step_positions(&self.stored.result, step, self.input, self.frame)
    }
}

impl App {
    /// The result the palette follows, while the document holds it.
    fn xtb_palette_result(&self) -> Option<(usize, &Stored)> {
        self.tab
            .doc
            .science
            .results
            .iter()
            .enumerate()
            .find(|(_, s)| {
                let shown = &self.tab.xtb.palette.result;
                shown.iter().any(|weak| is(weak, &s.result))
            })
    }

    pub(super) fn xtb_shown(&self) -> Option<Shown<'_>> {
        let doc = &self.tab.doc;
        let science = &doc.science;
        let (index, stored) = self.xtb_palette_result()?;
        let link = science.attachment(stored.attachment)?;
        let ts = self
            .tab
            .xtb
            .ts_sessions
            .get(&stored.result)
            .map(|ts| ts.as_ref());
        let is_ts = ts::is_ts(&stored.result);
        let arrow = if is_ts {
            match ts::saved_search(&stored.result) {
                Some(search) => search
                    .arrow
                    .and_then(|arrow| ts::bound_arrow(doc, arrow, &link.atoms)),
                None => match ts {
                    Some(session) => ts::bound_arrow(doc, session.arrow, &link.atoms),
                    None => ts::unique_matching_plan(doc, &link.atoms).map(|(arrow, _)| arrow),
                },
            }
        } else {
            None
        };
        Some(Shown {
            index,
            stored,
            input: science.frame(&link.frame)?,
            frame: science.frame(&stored.result.frame)?,
            atoms: &link.atoms,
            stale: link.freshness(doc) == Freshness::Stale
                || is_ts && (arrow.is_none() || self.xtb_ts_search_stale(&stored.result)),
            arrow,
            info: self.tab.xtb.info(&stored.result),
            ts,
        })
    }

    /// Open the palette on this result, restoring a reaction's path preview.
    pub(in crate::app) fn xtb_show_result(&mut self, result: &Arc<CalcResult>) {
        if self.tab.xtb.palette.show(result)
            && let Some(index) = self.xtb_step()
        {
            let status = std::mem::take(&mut self.status);
            let error = self.error;
            self.xtb_preview_step(index);
            self.status = status;
            self.error = error;
        }
    }

    pub(in crate::app) fn xtb_sync_reaction_palette(&mut self, selection_changed: bool) -> bool {
        let palette = &self.tab.xtb.palette;
        let Some(arrow) = self.xtb_selected_reaction().filter(|_| palette.open) else {
            return false;
        };
        let showing_ts = palette.is_showing_ts();
        let failed = self
            .xtb_palette_failure(Some(arrow))
            .is_some_and(|failed| failed.ts().is_some());
        if !selection_changed && !showing_ts && !failed {
            return false;
        }
        if let ts::card::ArrowResult::Found(result) = self.xtb_ts_card_result(arrow) {
            if palette.follows(result) {
                return false;
            }
            let result = Arc::clone(result);
            self.xtb_show_result(&result);
        } else if showing_ts || failed {
            if palette.result.is_empty() {
                return false;
            }
            self.tab.xtb.palette.clear_result();
        } else {
            return false;
        }
        self.xtb_vibrations_leave();
        true
    }

    fn xtb_palette_failure(&self, arrow: Option<u64>) -> Option<&super::Failed> {
        let Some(super::Outcome::Failed(failed)) = &self.tab.xtb.outcome else {
            return None;
        };
        failed
            .ts()
            .is_none_or(|ts| arrow == Some(ts.arrow))
            .then_some(failed)
    }

    /// Whether Show results would show something new: the card's molecule
    /// has a newer result than the one on display, or the palette is closed
    /// on results.
    pub(super) fn xtb_can_show(&self) -> bool {
        if !self.tab.xtb.palette.open
            && self
                .xtb_palette_failure(self.xtb_selected_reaction())
                .is_some()
        {
            return true;
        }
        let visible = self.xtb_palette_visible();
        match self.xtb_newest() {
            Some(newest) => {
                !visible
                    || !self
                        .xtb_shown()
                        .is_some_and(|s| Arc::ptr_eq(&s.stored.result, &newest))
            }
            None => !visible && self.xtb_shown().is_some(),
        }
    }

    /// The newest result stored for the card's molecule; cheap enough for
    /// every frame (no freshness or frame hashing).
    fn xtb_newest(&self) -> Option<Arc<CalcResult>> {
        if let Some(arrow) = self.xtb_selected_reaction() {
            return match self.xtb_ts_card_result(arrow) {
                ts::card::ArrowResult::Found(result) => Some(Arc::clone(result)),
                _ => None,
            };
        }
        let target: std::collections::HashSet<u64> = self.xtb_target()?.into_iter().collect();
        let science = &self.tab.doc.science;
        science
            .results
            .iter()
            .rev()
            .find(|s| {
                science
                    .attachment(s.attachment)
                    .is_some_and(|a| a.atoms.iter().flatten().any(|id| target.contains(id)))
            })
            .map(|s| s.result.clone())
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
    pub(super) fn xtb_step(&self) -> Option<usize> {
        let step = self.tab.xtb.palette.step.as_ref()?;
        (step.revision == (self.tab.file_epoch, self.tab.revision)).then_some(step.index)
    }

    /// Esc ends a step preview first.
    pub(in crate::app) fn xtb_end_preview(&mut self) -> bool {
        let previewing = self.xtb_step().is_some();
        self.tab.xtb.palette.step = None;
        self.tab.xtb.palette.confirm.clear();
        // The overlay belongs to the preview that drew it.
        self.tab.xtb.palette.ensemble.overlay = false;
        previewing
    }

    /// The step whose geometry the 3D model shows, if any: the last one
    /// unless another step was used.
    pub(super) fn xtb_model_step(&self, shown: &Shown<'_>) -> Option<usize> {
        let result = &shown.stored.result;
        let count = results::energies(result).len();
        let atom = shown.atoms.iter().flatten().next()?;
        let model = self
            .tab
            .doc
            .models
            .iter()
            .find(|m| m.linked_atoms.contains(atom))?;
        let now: Vec<[f32; 3]> = model
            .atoms
            .iter()
            .map(|a| a.xyz)
            .chain(model.hydrogens.iter().map(|h| h.xyz))
            .collect();
        if now.is_empty() {
            return None;
        }
        let at = |xyz: &[f64]| {
            xyz.len() == now.len() * 3
                && xyz
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .zip(&now)
                    .all(|(p, q)| p.map(|x| x as f32) == *q)
        };
        let path = result
            .trajectory
            .as_ref()
            .map_or(&[][..], |path| path.positions_angstrom.as_slice());
        if path.len() == count * now.len() * 3 {
            return path.chunks_exact(now.len() * 3).rposition(at);
        }
        [(count.checked_sub(1)?, shown.frame), (0, shown.input)]
            .into_iter()
            .find(|(_, frame)| at(frame.positions_angstrom.as_flattened()))
            .map(|(step, _)| step)
    }

    /// "Saved in file" for each result, and the shown result's full extra,
    /// measured again when the calculation data, shown result or pins change.
    fn xtb_sizes(&self, shown: &Shown<'_>) -> (usize, usize) {
        let doc = &self.tab.doc;
        let science = &doc.science;
        let pins = doc.orbital_pins();
        let same = |sizes: &Sizes| {
            sizes.index == shown.index
                && sizes.attachments == science.attachments
                && sizes.pins == pins
                && sizes.results.len() == science.results.len()
                && sizes
                    .results
                    .iter()
                    .zip(&science.results)
                    .all(|((weak, keep), s)| is(weak, &s.result) && *keep == s.keep_full)
        };
        let mut cache = self.tab.xtb.palette.sizes.borrow_mut();
        let sizes = match cache.take() {
            Some(sizes) if same(&sizes) => cache.insert(sizes),
            _ => cache.insert(Sizes {
                results: science
                    .results
                    .iter()
                    .map(|s| (Arc::downgrade(&s.result), s.keep_full))
                    .collect(),
                attachments: science.attachments.clone(),
                index: shown.index,
                saved: science.saved_sizes_with_pins(&pins),
                pins,
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
        // Partial charges beside the drawn atoms (implicit hydrogens have none).
        let atom_labels = shown
            .stored
            .result
            .charges
            .first()
            .map(|charges| {
                shown
                    .atoms
                    .iter()
                    .zip(&charges.values)
                    .filter_map(|(atom, q)| Some(((*atom)?, results::signed_charge(*q))))
                    .collect()
            })
            .unwrap_or_default();
        Some(crate::canvas::ResultLabel {
            atoms: shown.atoms.iter().flatten().copied().collect(),
            text: format!("E {} Eh", state::signed(energy, 6)),
            atom_labels,
        })
    }

    pub(super) fn xtb_palette(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show => {
                if self
                    .xtb_palette_failure(self.xtb_selected_reaction())
                    .is_some()
                {
                    self.tab.xtb.palette.open = true;
                    self.xtb.dock.collapsed = false;
                }
                let newest = self.xtb_newest();
                let shown = self
                    .xtb_palette_result()
                    .filter(|_| self.xtb_selected_reaction().is_none())
                    .map(|(_, shown)| shown.result.clone());
                if let Some(result) = newest.or(shown) {
                    self.xtb_show_result(&result);
                }
            }
            Action::Close => {
                self.tab.xtb.palette.open = false;
                self.xtb_end_preview();
                return super::super::focus_drawing();
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
                if !matches!(view, View::Steps | View::Conformers | View::Path) {
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
                    let log = results::log_with_ts(
                        &shown.stored.result,
                        shown.frame,
                        shown.info,
                        &method,
                        shown.ts,
                    );
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
            Action::IrExported(result) => match result {
                Ok(Some(path)) => {
                    self.status = format!("Exported the IR spectrum\n{}", path.display());
                    self.error = false;
                }
                Ok(None) => {}
                Err(error) => {
                    self.status = format!("Could not export the IR spectrum\n{error}");
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

    pub(super) fn xtb_preview_step(&mut self, index: usize) {
        let Some(shown) = self.xtb_shown() else {
            return;
        };
        let result = &shown.stored.result;
        let path = ts::is_ts(result);
        let previewable = !shown.stale && !self.xtb_running();
        if !previewable && !path {
            return;
        }
        let count = if path {
            ts::path_energies(result)
        } else {
            results::energies(result)
        }
        .len();
        if index >= count {
            return;
        }
        let mut failure = None;
        let positions = previewable.then(|| shown.step_positions(index)).flatten();
        let preview = positions.and_then(|positions| {
            let mut doc = self.tab.doc.clone();
            let frame = Frame {
                positions_angstrom: positions,
                ..(**shown.frame).clone()
            };
            let moved = if path {
                ts::preview_image(&mut doc, shown.atoms, shown.input, &frame)
            } else {
                science::move_model(&mut doc, shown.atoms, &frame)
            };
            if let Err(error) = moved {
                failure = Some(error.to_string());
                return None;
            }
            // A model shown as 2D only appears beside the drawing.
            let atom = shown.atoms.iter().flatten().next()?;
            let model = doc.models.iter().position(|m| {
                m.linked_atoms.contains(atom) && m.display == reshiki::models3d::Display::TwoD
            });
            if let Some(model) = model {
                reshiki::models3d::set_display(&mut doc, model, reshiki::models3d::Display::Both);
            }
            if self.tab.xtb.palette.ensemble.overlay {
                super::conformers::overlay(&mut doc, &shown, index);
            }
            Some(doc)
        });
        let name = if path {
            "Path point".into()
        } else {
            super::conformers::step_name(result, index)
        };
        self.status = if let Some(error) = failure {
            format!("{name} cannot be shown: {error}")
        } else if preview.is_some() {
            if path {
                "Previewing the reaction path · Esc returns".into()
            } else {
                super::conformers::previewing(
                    result,
                    index,
                    count,
                    self.tab.xtb.palette.ensemble.overlay,
                )
            }
        } else if path {
            "Selected a point on the reaction path".into()
        } else {
            format!("{name} is not saved in this file")
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
        let Some(shown) = self
            .xtb_shown()
            .filter(|shown| !shown.stale && !ts::is_ts(&shown.stored.result))
        else {
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
                            &format!(
                                "{} changes",
                                super::conformers::step_name(&shown.stored.result, index)
                            ),
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
            self.status = match self.xtb_shown() {
                Some(shown) if super::conformers::is_search(&shown.stored.result) => {
                    format!(
                        "Applied conformer {} to the 3D model · ⌘Z undoes",
                        index + 1
                    )
                }
                _ => format!("Applied xTB geometry of step {index} · ⌘Z undoes"),
            };
        }
        self.tab.xtb.palette.step = None;
        self.tab.xtb.palette.confirm.clear();
        self.tab.xtb.palette.ensemble.overlay = false;
    }

    /// Run the shown result's calculation again on the molecule as it is now.
    /// Builds without the engine show no Recalculate, as they show no Run.
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
        let Some(mut config) = results::config(
            result,
            shown.frame,
            shown.atoms,
            self.xtb.settings.threads(),
        ) else {
            self.status = "The Calculate card cannot repeat this calculation".into();
            self.error = true;
            return Task::none();
        };
        config.fixed.retain(|id| self.tab.doc.atom(*id).is_some());
        // A search runs again with the card's Conformers fields.
        let search = match config.task {
            super::CalcTask::Conformers => {
                match super::conformers::recalculation(&mut config, &self.xtb.conformers) {
                    Ok(search) => Some(search),
                    Err(reason) => {
                        self.status = reason;
                        self.error = true;
                        return Task::none();
                    }
                }
            }
            _ => None,
        };
        let request = Request {
            atoms: reshiki::molecule_names::component(&self.tab.doc, first),
            search,
            config,
            keep_full: shown.stored.keep_full,
            start: None,
            ts: None,
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
        let pins = self.tab.doc.orbital_pins();
        let bytes = self.tab.doc.science.file_bytes_with_pins(&pins);
        if bytes <= science::LIGHTER_BYTES {
            return false;
        }
        self.tab.xtb.palette.size_asked = true;
        let mut lighter = Document {
            science: self.tab.doc.science.clone(),
            models: self.tab.doc.models.clone(),
            ..Document::default()
        };
        let results = science::make_lighter(&mut lighter);
        self.prompt = Some(crate::app::prompt::Prompt::LargeResults {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            bytes,
            lighter: (results > 0).then(|| (results, lighter.science.file_bytes_with_pins(&pins))),
            then: Box::new(then),
        });
        true
    }
}

impl App {
    /// The large-results question's answer, for the document it asked
    /// about: Make lighter changes it first (one Undo step), then the save
    /// goes on. With a different document in front nothing happens.
    pub(in crate::app) fn xtb_size_answer(
        &mut self,
        prompt: crate::app::prompt::Prompt,
        answer: crate::app::prompt::Answer,
    ) -> Option<Message> {
        use crate::app::prompt::{Answer, Prompt};
        let Prompt::LargeResults {
            tab,
            epoch,
            lighter,
            ..
        } = &prompt
        else {
            return Some(prompt.answer(answer));
        };
        // Asked about another document: drop the question, and end only a
        // close that was waiting for that document's save.
        if (*tab, *epoch) != (self.tab.id, self.tab.file_epoch) {
            let ours = matches!(
                self.pending,
                Some(crate::app::Pending::CloseTab(id) | crate::app::Pending::CloseWindow(_, id, _))
                    if id == *tab
            );
            return ours.then_some(Message::Cancel);
        }
        if answer == Answer::Save && lighter.is_some() {
            self.xtb_make_lighter();
        }
        Some(prompt.answer(answer))
    }

    /// "Make lighter" as one Undo step; the palette and run details follow
    /// the lightened results.
    fn xtb_make_lighter(&mut self) {
        let mut doc = self.tab.doc.clone();
        if science::make_lighter(&mut doc) == 0 {
            return;
        }
        self.xtb_follow_results(&doc);
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
    }

    /// Before `doc` replaces the drawing: results it replaced by lighter or
    /// pinned copies keep their run details and the palette's place.
    pub(super) fn xtb_follow_results(&mut self, doc: &Document) {
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
            if let Some(ts) = self.tab.xtb.ts_sessions.get(&old.result).cloned() {
                self.tab.xtb.ts_sessions.remember(&new.result, ts);
            }
            let shown = &mut self.tab.xtb.palette.result;
            if shown.iter().any(|weak| is(weak, &old.result)) {
                shown.push(Arc::downgrade(&new.result));
            }
            for view in &mut self.tab.xtb.palette.ts_views {
                if view.result.iter().any(|weak| is(weak, &old.result)) {
                    view.result.push(Arc::downgrade(&new.result));
                }
            }
        }
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
            "Make lighter drops the full data you chose to keep for {count} result{} (optimization steps, gradients, normal modes), leaving {} of results. Energies and final geometries stay; data imported from other programs stays.",
            if count == 1 { "" } else { "s" },
            results::size(after)
        ),
        None => "The results are already light: data imported from other programs cannot be recomputed, so it stays.".into(),
    };
    (title, text)
}

mod view;
pub(super) use view::alert;

#[cfg(test)]
pub(in crate::app) mod tests;
