//! The xTB results palette: docked under the status bar like NMR (or in the
//! inspector), with result tabs, the stale banner, energy labels,
//! "Saved in file" and the large-results question on Save.
use super::results::{self, RunInfo, View};
use super::{Request, is, state, ts};
use crate::app::{App, Message};
use iced::Task;
use reshiki::atom_labels::{Number, shift_label_style};
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, ChargeScheme, Frame, Spin};
use reshiki::science::{self, Freshness, Stored};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

#[derive(Clone, Debug)]
pub(crate) enum Action {
    /// Open the palette on the newest result of the card's molecule.
    Show,
    Close,
    Collapse,
    Previous,
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
    CopyMethods,
    CopyTable,
    Export,
    Exported(Result<Option<crate::app::figure_export::Saved>, String>),
    IrcExported(Result<Option<crate::app::figure_export::Saved>, String>),
    IrExported(Result<Option<std::path::PathBuf>, String>),
    CopyLog,
    Labels(bool),
    PlaceCharges,
    Recalculate,
    RecalculateMolecule,
    KeepFull(bool),
    /// Save as `name` after the large-results question.
    SaveNamed(String),
}

/// The window's palette placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Dock {
    pub inspector: bool,
}

/// One document's palette.
#[derive(Default)]
pub(crate) struct Palette {
    /// The result shown, and the lighter copies Make lighter made of it:
    /// identities that Undo, Redo and other results' edits keep, unlike an
    /// index or an attachment ID.
    result: Vec<Weak<CalcResult>>,
    pub(super) open: bool,
    pub(in crate::app) collapsed: bool,
    pub(super) previous: bool,
    pub(super) view: View,
    pub(super) labels: bool,
    /// The live search explicitly chosen for the dock, independently of saved results.
    live_ts: Option<u64>,
    step: Option<Step>,
    pub(super) profile: Option<ts::irc::Profile>,
    pub(super) irc_playback: Option<ts::irc::Playback>,
    ts_views: Vec<TsView>,
    /// Why the previewed step was not used: the stereo check's findings.
    pub(super) confirm: Vec<String>,
    /// The Conformers tab's overlay choice and its table.
    pub(super) ensemble: super::conformers::Ensemble,
    status: Option<(StatusKind, String)>,
    /// Save asked about large results once for this document.
    size_asked: bool,
    /// File sizes for one document state; serializing is too slow per frame.
    sizes: RefCell<Option<Sizes>>,
}

struct Step {
    point: PreviewPoint,
    attachment: u64,
    result: Weak<CalcResult>,
    /// The document state the preview belongs to; any edit ends it.
    revision: (u64, u64),
    /// The document with the model at this step, when the step's geometry
    /// is known.
    preview: Option<Box<Document>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewPoint {
    Step(usize),
    ReachedEndpoint,
}

struct TsView {
    result: Vec<Weak<CalcResult>>,
    view: View,
    profile: Option<ts::irc::Profile>,
    step: Option<Step>,
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
    fn clear_result(&mut self) {
        self.ts_views
            .retain(|view| view.result.iter().any(|weak| weak.strong_count() > 0));
        if self.is_showing_ts() {
            self.ts_views.push(TsView {
                result: std::mem::take(&mut self.result),
                view: self.view,
                profile: self.profile,
                step: self.step.take().map(|step| Step {
                    preview: None,
                    ..step
                }),
            });
        }
        self.result.clear();
        self.profile = None;
        self.irc_playback = None;
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
        self.previous = false;
        self.live_ts = None;
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
                self.profile = saved.profile;
                self.step = saved.step;
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
    pub ts_reference: Option<&'static str>,
}

impl Shown<'_> {
    pub(super) fn method(&self) -> String {
        results::method_line(&self.stored.result, self.frame)
    }

    pub(super) fn rows(&self, unit: Option<super::EnergyUnit>) -> Vec<(String, String)> {
        let result = &self.stored.result;
        let mut rows = results::summary_rows(result, self.input, self.frame, self.info);
        if let Some(unit) = unit {
            rows.extend(ts::view::path_rows(
                result,
                self.ts,
                unit,
                self.ts_reference,
            ));
        }
        rows
    }

    pub(super) fn step_positions(&self, step: usize) -> Option<Vec<[f64; 3]>> {
        if ts::is_ts(&self.stored.result) {
            return self.ts?.image(step);
        }
        results::step_positions(&self.stored.result, step, self.input, self.frame)
    }

    fn repeatable(&self) -> bool {
        results::config(&self.stored.result, self.frame, self.atoms, 1).is_some()
    }

    /// Only atoms present in the drawing can carry a preview or placed charge.
    fn charge_labels(&self, doc: &Document, places: usize) -> Vec<(u64, String)> {
        self.stored
            .result
            .charges
            .first()
            .map(|charges| {
                self.atoms
                    .iter()
                    .zip(&charges.values)
                    .filter_map(|(atom, q)| {
                        let id = (*atom)?;
                        doc.atom_visible(id)
                            .then(|| (id, results::signed_charge(*q, places)))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Placed charges are decimals signed with "+" or a real minus sign (U+2212).
/// NMR shift labels are unsigned or use an ASCII hyphen ("-0.25"), and user
/// text is anything else, so those are preserved.
fn is_charge_label(text: &str) -> bool {
    let Some(digits) = text.strip_prefix(['+', '−']) else {
        return false;
    };
    let Some((whole, part)) = digits.split_once('.') else {
        return false;
    };
    [whole, part]
        .iter()
        .all(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

impl App {
    /// A running search occupies the dock only when that reaction chose it.
    pub(super) fn xtb_palette_live_ts(&self) -> Option<&ts::Live> {
        self.xtb_live_ts()
            .filter(|live| self.xtb_palette_follows_live_ts(live.request.arrow))
    }

    pub(super) fn xtb_palette_follows_live_ts(&self, arrow: u64) -> bool {
        self.tab.xtb.palette.open
            && self.tab.xtb.palette.live_ts == Some(arrow)
            && self.xtb_selected_reaction() == Some(arrow)
    }

    pub(super) fn xtb_show_live_path(&mut self, arrow: u64) -> bool {
        if !self
            .xtb_live_ts()
            .is_some_and(|live| live.request.arrow == arrow)
        {
            return false;
        }
        self.xtb_end_preview();
        let palette = &mut self.tab.xtb.palette;
        palette.live_ts = Some(arrow);
        palette.open = true;
        palette.collapsed = false;
        palette.previous = false;
        palette.view = View::Path;
        true
    }

    /// Keep an unrelated open result until the user chooses the live path.
    pub(super) fn xtb_ts_palette_started(&mut self) {
        let Some(arrow) = self.xtb_live_ts().map(|live| live.request.arrow) else {
            return;
        };
        self.tab.xtb.palette.live_ts = None;
        if !self.tab.xtb.palette.open
            || self
                .xtb_shown()
                .is_none_or(|shown| shown.arrow == Some(arrow))
        {
            self.xtb_show_live_path(arrow);
        }
    }

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
        self.xtb_result(self.xtb_shown_entry()?)
    }

    pub(super) fn xtb_shown_entry(&self) -> Option<(usize, &Stored)> {
        if self.xtb_palette_live_ts().is_some() {
            return None;
        }
        let palette = &self.tab.xtb.palette;
        if palette.result.is_empty() {
            return palette.open.then(|| self.xtb_newest()).flatten();
        }
        let Some(entry) = self.xtb_palette_result() else {
            return self.xtb_newest();
        };
        // Reaction selection follows its own result through xtb_sync_reaction_palette.
        if self.xtb_selected_reaction().is_some() {
            return Some(entry);
        }
        let science = &self.tab.doc.science;
        let link = science.attachment(entry.1.attachment)?;
        if link
            .atoms
            .iter()
            .flatten()
            .any(|id| self.tab.selected.contains(id))
        {
            return Some(entry);
        }
        let Some(target) = self.xtb_target() else {
            return Some(entry);
        };
        let target: HashSet<_> = target.into_iter().collect();
        if link.atoms.iter().flatten().any(|id| target.contains(id)) {
            Some(entry)
        } else {
            self.xtb_newest_for(&target)
        }
    }

    fn xtb_result<'a>(&'a self, (index, stored): (usize, &'a Stored)) -> Option<Shown<'a>> {
        let doc = &self.tab.doc;
        let science = &doc.science;
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
            ts_reference: self.xtb_ts_reference(&stored.result, arrow),
        })
    }

    /// Open the palette on this result, restoring a reaction's path preview.
    pub(in crate::app) fn xtb_show_result(&mut self, result: &Arc<CalcResult>) {
        self.xtb_follow_result(result);
        self.xtb_palette_opened();
    }

    fn xtb_follow_result(&mut self, result: &Arc<CalcResult>) {
        self.xtb_clear_palette_status(Some(StatusKind::Mode));
        if self.tab.xtb.palette.show(result)
            && let Some(point) = self.xtb_current_step().map(|step| step.point)
        {
            let status = std::mem::take(&mut self.status);
            let error = self.error;
            let palette_status = self.tab.xtb.palette.status.take();
            self.xtb_preview_point(point);
            self.status = status;
            self.error = error;
            self.tab.xtb.palette.status = palette_status;
        }
    }

    pub(in crate::app) fn xtb_sync_reaction_palette(&mut self, selection_changed: bool) -> bool {
        let palette = &self.tab.xtb.palette;
        let Some(arrow) = self.xtb_selected_reaction().filter(|&arrow| {
            palette.open
                && self
                    .xtb_live_ts()
                    .is_none_or(|live| live.request.arrow != arrow)
        }) else {
            return false;
        };
        let showing_ts = palette.is_showing_ts();
        let failed = self
            .xtb_palette_failure(Some(arrow))
            .is_some_and(|failed| failed.ts().is_some());
        if !selection_changed
            && !failed
            && (!showing_ts
                || self
                    .xtb_palette_result()
                    .and_then(|(_, stored)| ts::saved_search(&stored.result)?.arrow)
                    .is_some_and(|shown| shown != arrow))
        {
            return false;
        }
        if !selection_changed
            && let ts::card::ArrowResult::Found(result) = self.xtb_ts_card_result(arrow)
            && self
                .xtb_ts_linked_pair(result)
                .is_some_and(|pair| pair.into_iter().any(|saved| palette.follows(saved)))
        {
            return false;
        }
        if let ts::card::ArrowResult::Found(result) = self.xtb_ts_card_display_result(arrow) {
            if palette.follows(result) {
                return false;
            }
            let result = Arc::clone(result);
            self.xtb_follow_result(&result);
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
        match failed.ts() {
            Some(ts) => (arrow == Some(ts.arrow)).then_some(failed),
            None => self.xtb_failed(),
        }
    }

    fn xtb_palette_arrow(&self, shown: Option<&Shown<'_>>) -> Option<u64> {
        // Follow the displayed reaction, then the selection, then a failed
        // first search whose reaction has no result on display.
        shown
            .and_then(|shown| shown.arrow)
            .or_else(|| self.xtb_selected_reaction())
            .or_else(|| match (shown, &self.tab.xtb.outcome) {
                (None, Some(super::Outcome::Failed(failed))) => failed.ts().map(|ts| ts.arrow),
                _ => None,
            })
    }

    fn xtb_palette_has_failure(&self) -> bool {
        let shown = self.xtb_shown();
        self.xtb_palette_failure(self.xtb_palette_arrow(shown.as_ref()))
            .is_some()
    }

    /// Whether Show results would show something new: the card's molecule
    /// has a newer result than the one on display, or the palette is closed
    /// on results.
    pub(super) fn xtb_can_show(&self) -> bool {
        let open = self.tab.xtb.palette.open;
        if self
            .xtb_live_ts()
            .is_some_and(|live| self.xtb_selected_reaction() == Some(live.request.arrow))
        {
            return !open || self.xtb_palette_live_ts().is_none();
        }
        let reaction = self.xtb_selected_reaction();
        if self.xtb_palette_failure(reaction).is_some() {
            return !open;
        }
        let shown = self.xtb_shown_entry().map(|(index, _)| index);
        match self.xtb_newest() {
            Some((newest, _)) => !open || shown != Some(newest),
            None => reaction.is_none() && !open && shown.is_some(),
        }
    }

    /// The newest result stored for the card's molecule; cheap enough for
    /// every frame (no freshness or frame hashing).
    fn xtb_newest(&self) -> Option<(usize, &Stored)> {
        if let Some(arrow) = self.xtb_selected_reaction() {
            let ts::card::ArrowResult::Found(result) = self.xtb_ts_card_result(arrow) else {
                return None;
            };
            return self
                .tab
                .doc
                .science
                .results
                .iter()
                .enumerate()
                .rfind(|(_, stored)| Arc::ptr_eq(&stored.result, result));
        }
        self.xtb_newest_for(&self.xtb_target()?.into_iter().collect())
    }

    fn xtb_newest_for(&self, target: &HashSet<u64>) -> Option<(usize, &Stored)> {
        self.tab
            .doc
            .science
            .results
            .iter()
            .enumerate()
            .rev()
            .find(|(_, stored)| self.xtb_result_belongs(stored, target))
    }

    fn xtb_result_belongs(&self, stored: &Stored, target: &HashSet<u64>) -> bool {
        self.tab
            .doc
            .science
            .attachment(stored.attachment)
            .is_some_and(|link| link.atoms.iter().flatten().any(|id| target.contains(id)))
    }

    /// Whether the card can repeat this molecule's newest calculation.
    pub(in crate::app) fn xtb_can_recalculate_molecule(&self) -> bool {
        super::available()
            && self
                .xtb_newest()
                .and_then(|entry| self.xtb_result(entry))
                .is_some_and(|shown| shown.repeatable())
    }

    pub(super) fn xtb_palette_body_visible(&self) -> bool {
        self.tab.xtb.palette.open
            && if self.xtb.dock.inspector {
                self.inspector_open
                    && self.inspector_tab == crate::app::InspectorTab::Predict
                    && self.xtb.page == super::Page::Xtb
            } else {
                !self.tab.xtb.palette.collapsed
            }
    }

    /// Whether the visible palette shows a stale result it can run again.
    pub(in crate::app) fn xtb_palette_can_repeat_stale(&self, atoms: &[u64]) -> bool {
        self.xtb_palette_body_visible()
            && self.tab.xtb.run.is_none()
            && self.xtb_failed().is_none()
            && self.xtb_shown().is_some_and(|shown| {
                shown.stale
                    && shown.atoms.iter().flatten().any(|id| atoms.contains(id))
                    && shown.repeatable()
            })
    }

    /// The palette is showing a result.
    pub(super) fn xtb_palette_visible(&self) -> bool {
        self.tab.xtb.palette.open
            && (self.xtb_palette_live_ts().is_some() || self.xtb_shown_entry().is_some())
    }

    /// The step preview, while the document has not changed since.
    pub(super) fn xtb_step_preview(&self) -> Option<&Document> {
        self.xtb_current_step()?.preview.as_deref()
    }

    /// The selected step, while it is current.
    pub(super) fn xtb_step(&self) -> Option<usize> {
        match self.xtb_current_step()?.point {
            PreviewPoint::Step(index) => Some(index),
            PreviewPoint::ReachedEndpoint => None,
        }
    }

    pub(super) fn xtb_reached_endpoint_selected(&self) -> bool {
        self.xtb_current_step()
            .is_some_and(|step| step.point == PreviewPoint::ReachedEndpoint)
    }

    fn xtb_current_step(&self) -> Option<&Step> {
        let step = self.tab.xtb.palette.step.as_ref()?;
        if step.revision != (self.tab.file_epoch, self.tab.revision) || self.xtb_failed().is_some()
        {
            return None;
        }
        let (_, shown) = self.xtb_shown_entry()?;
        (step.attachment == shown.attachment && is(&step.result, &shown.result)).then_some(step)
    }

    /// Esc ends a step preview first.
    pub(in crate::app) fn xtb_end_preview(&mut self) -> bool {
        let playing = self.tab.xtb.palette.irc_playback.take().is_some();
        let previewing = playing || self.xtb_current_step().is_some();
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

    /// The on-screen charges and energy for the current result.
    pub(in crate::app) fn xtb_label(&self) -> Option<crate::canvas::ResultLabel> {
        if !self.tab.xtb.palette.labels || !self.tab.xtb.palette.open || self.xtb_failed().is_some()
        {
            return None;
        }
        let shown = self.xtb_shown().filter(|shown| !shown.stale)?;
        let text = shown
            .stored
            .result
            .energy_hartree
            .map_or_else(String::new, |energy| {
                format!("E {} Eh", state::signed(energy, 6))
            });
        // Partial charges beside the drawn atoms (implicit hydrogens have none).
        let atom_labels = shown.charge_labels(&self.tab.doc, 3);
        (!text.is_empty() || !atom_labels.is_empty()).then(|| crate::canvas::ResultLabel {
            atoms: shown.atoms.iter().flatten().copied().collect(),
            text,
            atom_labels,
        })
    }

    fn xtb_place_charges(&mut self) {
        let Some(shown) = self.xtb_shown().filter(|shown| !shown.stale) else {
            return;
        };
        let Some(charges) = shown.stored.result.charges.first() else {
            return;
        };
        let scheme = match &charges.scheme {
            ChargeScheme::Mulliken => "Mulliken",
            ChargeScheme::Eeq => "EEQ",
            ChargeScheme::Cm5 => "CM5",
            ChargeScheme::Hirshfeld => "Hirshfeld",
            ChargeScheme::Loewdin => "Löwdin",
            ChargeScheme::Other(name) => name,
        };
        let mut labels = shown.charge_labels(&self.tab.doc, 2);
        let style = shift_label_style(&self.tab.doc);
        let visible = labels.len();
        labels.retain(|(id, _)| {
            self.tab.doc.atom(*id).is_some_and(|atom| {
                atom.display.number.as_ref().is_none_or(|label| {
                    label.style == style && label.offset.is_none() && is_charge_label(&label.text)
                })
            })
        });
        let n = labels.len();
        let kept = visible - n;
        let mut status = format!("Placed {n} {scheme} charges as atom labels · ⌘Z undoes");
        if kept > 0 {
            status.push_str(&if kept == 1 {
                " · 1 atom kept its own label".to_owned()
            } else {
                format!(" · {kept} atoms kept their own labels")
            });
        }
        let before = self.tab.doc.clone();
        for (id, mut text) in labels {
            if let Some(atom) = self.tab.doc.atom_mut(id) {
                // The results table and preview omit a sign on rounded zero.
                // Keep placed zero identifiable by the same signed-decimal
                // rule as every other charge when the user refreshes it.
                if text.starts_with('0') {
                    text.insert(0, '+');
                }
                atom.display.number = Some(Number {
                    text,
                    style: style.clone(),
                    offset: None,
                });
            }
        }
        self.changed(before);
        self.status = status;
        self.error = false;
    }

    pub(super) fn xtb_palette(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show
                if self.xtb_live_ts().is_some_and(|live| {
                    self.xtb_selected_reaction() == Some(live.request.arrow)
                }) =>
            {
                if let Some(arrow) = self.xtb_selected_reaction() {
                    self.xtb_show_live_path(arrow);
                }
            }
            Action::View(_) if self.xtb_palette_live_ts().is_some() => {}
            Action::Show => {
                self.tab.xtb.palette.previous = false;
                let reaction = self.xtb_selected_reaction();
                if self.xtb_palette_failure(reaction).is_some() {
                    self.tab.xtb.palette.open = true;
                    self.tab.xtb.palette.collapsed = false;
                }
                let result = self
                    .xtb_newest()
                    .or_else(|| reaction.is_none().then(|| self.xtb_shown_entry()).flatten())
                    .map(|(_, stored)| Arc::clone(&stored.result));
                if let Some(result) = result {
                    self.xtb_show_result(&result);
                }
            }
            Action::Close => {
                self.tab.xtb.palette.open = false;
                self.xtb_clear_palette_status(None);
                self.xtb_end_preview();
                return super::super::focus_drawing();
            }
            Action::Collapse => {
                self.tab.xtb.palette.collapsed = !self.tab.xtb.palette.collapsed;
                self.tab.xtb.palette.irc_playback = None;
            }
            Action::Previous => self.tab.xtb.palette.previous = !self.tab.xtb.palette.previous,
            Action::Dock => {
                let old_width = self.inspector_width();
                self.xtb.dock.inspector = !self.xtb.dock.inspector;
                if self.xtb.dock.inspector {
                    self.inspector_open = true;
                    self.inspector_tab = crate::app::InspectorTab::Predict;
                    self.xtb.page = super::Page::Xtb;
                } else {
                    self.xtb_palette_opened();
                }
                self.expect_inspector_width_change(old_width);
            }
            Action::View(view) => {
                if let Some((_, shown)) = self.xtb_shown_entry()
                    && !self.tab.xtb.palette.follows(&shown.result)
                {
                    let result = Arc::clone(&shown.result);
                    self.xtb_follow_result(&result);
                }
                if view != View::Vibrations {
                    self.xtb_clear_palette_status(Some(StatusKind::Mode));
                }
                self.tab.xtb.palette.view = view;
                if view != View::Path {
                    self.tab.xtb.palette.irc_playback = None;
                }
                if !matches!(view, View::Steps | View::Conformers | View::Path) {
                    self.xtb_end_preview();
                }
            }
            Action::Step(index) => self.xtb_preview_step(index),
            Action::StepBack => {
                // A Path point on a saved TS shows no geometry to leave.
                let showing_ts = self.tab.xtb.palette.is_showing_ts();
                if self.xtb_end_preview() && !showing_ts {
                    self.status = "Showing the 3D model".into();
                    self.error = false;
                }
            }
            Action::UseStep => self.xtb_use_step(false),
            Action::UseStepAnyway => self.xtb_use_step(true),
            Action::KeepModel => {
                self.xtb_end_preview();
                self.xtb_set_result_status("Step geometry not used", false);
            }
            Action::CopyMethods | Action::CopyTable | Action::Export | Action::CopyLog => {
                if self.xtb_failed().is_some() {
                    return Task::none();
                }
                let Some(shown) = self.xtb_shown() else {
                    return Task::none();
                };
                if matches!(action, Action::CopyMethods) {
                    let text = results::methods_text(&shown.stored.result, shown.frame);
                    self.xtb_set_palette_status(
                        StatusKind::Palette,
                        "Copied the methods text (BibTeX keys from ReShiki's xtb.bib)",
                        false,
                    );
                    return iced::clipboard::write(text);
                }
                let method = shown.method();
                if matches!(action, Action::CopyLog) {
                    let log = results::log_with_ts(
                        &shown.stored.result,
                        shown.frame,
                        shown.info,
                        &method,
                        shown.ts,
                        self.appearance.energy_unit,
                    );
                    self.xtb_set_palette_status(StatusKind::Palette, "Copied the xTB log", false);
                    return iced::clipboard::write(log);
                }
                let unit = ts::is_ts(&shown.stored.result).then_some(self.appearance.energy_unit);
                let rows = shown.rows(unit);
                let table = results::tsv(&method, &rows);
                if matches!(action, Action::CopyTable) {
                    self.xtb_set_palette_status(
                        StatusKind::Palette,
                        format!("Copied {} rows of xTB results", rows.len()),
                        false,
                    );
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
            Action::Exported(result) => {
                let details = match &result {
                    Ok(Some(saved)) => saved.details.clone(),
                    _ => vec![],
                };
                self.xtb_palette_exported(
                    result.map(|saved| saved.map(|s| s.path)),
                    &details,
                    false,
                )
            }
            Action::IrcExported(result) => {
                self.xtb_palette_exported(result.map(|saved| saved.map(|s| s.path)), &[], false);
                return crate::app::focus_drawing();
            }
            Action::IrExported(result) => self.xtb_palette_exported(result, &[], true),
            Action::Labels(on) => self.tab.xtb.palette.labels = on,
            Action::PlaceCharges => {
                if self.xtb_failed().is_none() {
                    self.xtb_place_charges();
                }
            }
            Action::Recalculate => return self.xtb_recalculate(),
            Action::RecalculateMolecule => {
                let result = self
                    .xtb_newest()
                    .map(|(_, stored)| Arc::clone(&stored.result));
                if let Some(result) = result {
                    self.xtb_show_result(&result);
                    self.tab.xtb.palette.collapsed = false;
                    return self.xtb_recalculate();
                }
            }
            Action::KeepFull(keep) => {
                let Some(index) = self.xtb_shown().map(|shown| shown.index) else {
                    return Task::none();
                };
                let mut doc = self.tab.doc.clone();
                if science::set_keep_full(&mut doc, index, keep) {
                    let before = std::mem::replace(&mut self.tab.doc, doc);
                    self.changed(before);
                    let text = if keep {
                        "Full results will be saved in this file"
                    } else {
                        "Only the light results will be saved in this file"
                    };
                    self.xtb_set_palette_status(StatusKind::Palette, text, false);
                }
            }
            Action::SaveNamed(name) => return self.save_drawing_named(name),
        }
        Task::none()
    }

    pub(super) fn xtb_preview_step(&mut self, index: usize) {
        self.xtb_preview_point(PreviewPoint::Step(index));
    }

    pub(super) fn xtb_preview_reached_endpoint(&mut self) {
        self.xtb_preview_point(PreviewPoint::ReachedEndpoint);
    }

    fn xtb_preview_point(&mut self, point: PreviewPoint) {
        self.tab.xtb.palette.irc_playback = None;
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
        let (index, positions) = match point {
            PreviewPoint::Step(index) if index < count => (
                index,
                previewable.then(|| shown.step_positions(index)).flatten(),
            ),
            PreviewPoint::ReachedEndpoint if path && previewable => {
                let Some(search) = ts::saved_search(result)
                    .filter(|search| !search.reached_positions_angstrom.is_empty())
                else {
                    return;
                };
                (0, Some(search.reached_positions_angstrom.clone()))
            }
            _ => return,
        };
        let table = super::conformers::is_search(result).then(|| self.xtb_conformer_table(&shown));
        let (rank, displayed_count) = table
            .as_ref()
            .and_then(|table| Some((table.rank(index)?, table.rows.len())))
            .unwrap_or((index, count));
        let mut failure = None;
        let preview = positions.and_then(|positions| {
            let mut doc = self.tab.doc.clone();
            let frame = Frame {
                positions_angstrom: positions,
                ..(**shown.frame).clone()
            };
            let moved = if path {
                if point == PreviewPoint::ReachedEndpoint {
                    ts::preview_endpoint_image(&mut doc, shown.atoms, shown.input, &frame)
                } else {
                    ts::preview_image(&mut doc, shown.atoms, shown.input, &frame)
                }
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
            if self.tab.xtb.palette.ensemble.overlay
                && let Some(table) = &table
            {
                super::conformers::overlay(&mut doc, &shown, index, &table.rows);
            }
            Some(doc)
        });
        drop(table);
        let reached = point == PreviewPoint::ReachedEndpoint;
        let name = if reached {
            "Reached endpoint".into()
        } else if path {
            "Path point".into()
        } else {
            super::conformers::step_name(result, rank)
        };
        let attachment = shown.stored.attachment;
        let result_id = Arc::downgrade(result);
        let text = if let Some(error) = failure {
            format!("{name} cannot be shown: {error}")
        } else if preview.is_some() {
            if reached {
                "Previewing the reached endpoint · Esc returns".into()
            } else if path {
                "Previewing the reaction path · Esc returns".into()
            } else {
                super::conformers::previewing(
                    result,
                    rank,
                    displayed_count,
                    self.tab.xtb.palette.ensemble.overlay,
                )
            }
        } else if path {
            "Selected a point on the reaction path".into()
        } else {
            format!("{name} is not saved in this file")
        };
        self.xtb_set_result_status(text, false);
        self.tab.xtb.palette.confirm.clear();
        self.tab.xtb.palette.step = Some(Step {
            point,
            attachment,
            result: result_id,
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
            self.xtb_set_result_status("Finish the current operation, then use the step", false);
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
        let rank = if super::conformers::is_search(&shown.stored.result) {
            self.xtb_conformer_table(&shown)
                .rank(index)
                .unwrap_or(index)
        } else {
            index
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
                                super::conformers::step_name(&shown.stored.result, rank)
                            ),
                            1,
                        )
                    })
                    .collect(),
                Err(error) => vec![format!("The stereo check could not run: {error}.")],
            };
            if let Some(first) = reasons.first() {
                self.xtb_set_result_status(
                    format!("{} · nothing applied", first.trim_end_matches('.')),
                    false,
                );
                self.tab.xtb.palette.confirm = reasons;
                return;
            }
        }
        let mut doc = self.tab.doc.clone();
        if let Err(error) = science::move_model(&mut doc, &atoms, &frame) {
            self.xtb_set_result_status(format!("{error}; nothing applied"), true);
            return;
        }
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
        if !self.error {
            let text = match self.xtb_shown() {
                Some(shown) if super::conformers::is_search(&shown.stored.result) => {
                    format!("Applied conformer {} to the 3D model · ⌘Z undoes", rank + 1)
                }
                _ => format!("Applied xTB geometry of step {index} · ⌘Z undoes"),
            };
            self.xtb_set_result_status(text, false);
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
            self.xtb_set_result_status("The molecule was deleted", true);
            return Task::none();
        };
        let Some(mut config) = results::config(
            result,
            shown.frame,
            shown.atoms,
            self.xtb.settings.threads(),
        ) else {
            self.xtb_set_result_status("The Calculate card cannot repeat this calculation", true);
            return Task::none();
        };
        config.fixed.retain(|id| self.tab.doc.atom(*id).is_some());
        let atoms = reshiki::molecule_names::component(&self.tab.doc, first);
        // A search runs again with the card's Conformers fields.
        let search = match config.task {
            super::CalcTask::Conformers => {
                match super::conformers::recalculation(&mut config, &self.xtb.conformers) {
                    Ok(mut search) => {
                        search.invert_undefined_stereo &= self.xtb_inverts_for(&atoms);
                        Some(search)
                    }
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
            atoms,
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
            self.tab.xtb.energy_state.follow_result(old, new);
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

mod sizing;
pub(super) use sizing::dock_height;
mod status;
mod view;

pub(super) use status::StatusKind;
pub(super) use view::alert;

#[cfg(test)]
pub(in crate::app) mod tests;
