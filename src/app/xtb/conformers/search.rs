//! A conformer search as the app runs it: the card's fields, the start
//! from the molecule's 3D model, progress, Cancel (keeping what is
//! re-ranked), the stored result and the tab's overlay and export.
use super::{Action, Format, Table};
#[cfg(not(target_arch = "wasm32"))]
use crate::app::xtb::{Action as XtbAction, Pending, results::RunInfo, state};
use crate::app::{App, Message};
use iced::Task;
use reshiki::science;
#[cfg(not(target_arch = "wasm32"))]
use reshiki::{document::Document, qcdata, science::Launch};
use std::sync::Arc;

#[derive(Default)]
pub(in crate::app::xtb) struct CardStereo {
    revision: (u64, u64),
    target: Vec<u64>,
    has_model: bool,
    labels: Option<(String, [String; 2])>,
}

impl App {
    pub(in crate::app::xtb) fn xtb_conformers(&mut self, action: Action) -> Task<Message> {
        let settings = &mut self.xtb.conformers;
        match action {
            Action::Starts(text) => settings.starts = text,
            Action::Minimizer(minimizer) => settings.minimizer = minimizer,
            Action::Diversity(diversity) => settings.diversity = diversity,
            Action::Window(text) => settings.window = text,
            Action::Representatives(text) => settings.representatives = text,
            Action::Three(three) => settings.three = three,
            Action::InvertUndefinedStereo(invert) => {
                self.xtb.conformer_inversion_target = invert
                    .then(|| self.xtb_target())
                    .flatten()
                    .map(|atoms| (self.tab.id.0, self.tab.file_epoch, atoms));
                self.xtb.conformers.invert_undefined_stereo =
                    self.xtb.conformer_inversion_target.is_some();
            }
            Action::Overlay(on) => {
                self.tab.xtb.palette.ensemble.overlay = on;
                // Off only takes the ghosts away from a preview.
                if on || self.xtb_step().is_some() {
                    self.xtb_reshow_conformer();
                }
            }
            Action::Export(format) => return self.xtb_export_conformers(format),
        }
        Task::none()
    }

    /// Show the previewed conformer again (with or without the overlay), or
    /// the one the model shows when none is previewed.
    fn xtb_reshow_conformer(&mut self) {
        let Some(shown) = self.xtb_shown() else {
            return;
        };
        if !super::is_search(&shown.stored.result) || shown.stale {
            return;
        }
        let index = self
            .xtb_step()
            .or_else(|| self.xtb_model_step(&shown))
            .unwrap_or(0);
        self.xtb_preview_step(index);
    }

    /// The Conformers tab's rows for the shown result, measured once per
    /// result and document state.
    pub(in crate::app::xtb) fn xtb_conformer_table(
        &self,
        shown: &crate::app::xtb::palette::Shown<'_>,
    ) -> std::cell::Ref<'_, Table> {
        let revision = (self.tab.file_epoch, self.tab.revision);
        let cache = &self.tab.xtb.palette.ensemble.table;
        let fresh = {
            let table = cache.borrow();
            table.revision == revision && crate::app::xtb::is(&table.result, &shown.stored.result)
        };
        if !fresh {
            let rows = super::rows(
                &self.tab.doc,
                shown.atoms,
                &shown.stored.result,
                shown.stale,
            );
            let stereo_note = self.xtb_conformer_stereo_note(shown, &rows);
            *cache.borrow_mut() = Table {
                result: Arc::downgrade(&shown.stored.result),
                revision,
                rows,
                stereo_note,
            };
        }
        cache.borrow()
    }

    /// Assess each drawing and target once, including targets with no undefined centres.
    pub(super) fn xtb_conformer_stereo(
        &self,
        ids: &[u64],
        has_model: bool,
    ) -> Option<(String, [String; 2])> {
        let revision = (self.tab.file_epoch, self.tab.revision);
        let mut cache = self.tab.xtb.conformer_stereo.borrow_mut();
        if cache.revision != revision || cache.target != ids || cache.has_model != has_model {
            let first = *ids.first()?;
            let doc = &self.tab.doc;
            let launch = if has_model {
                let (charge, multiplicity) = science::drawn_state(doc, first);
                science::launch(doc, first, charge, multiplicity).ok()
            } else {
                None
            };
            let model = launch
                .as_ref()
                .map(|l| (l.atoms.as_slice(), l.frame.as_ref()));
            let labels =
                science::tetrahedral_stereo(doc, ids, model)
                    .ok()
                    .and_then(|mut stereo| {
                        let drawing_order: Vec<_> = ids.iter().copied().map(Some).collect();
                        let atoms = model.map_or(drawing_order.as_slice(), |(atoms, _)| atoms);
                        stereo.undefined.sort_by_key(|centre| {
                            atoms.iter().position(|&id| id == Some(centre.atom))
                        });
                        super::view::card_stereo(&stereo, launch.is_some(), |atom| {
                            self.xtb_atom_name(atoms, atom).0
                        })
                    });
            *cache = CardStereo {
                revision,
                target: ids.to_vec(),
                has_model,
                labels,
            };
        }
        cache.labels.clone()
    }

    fn xtb_export_conformers(&mut self, format: Format) -> Task<Message> {
        let Some(shown) = self.xtb_shown() else {
            return Task::none();
        };
        if format == Format::Sdf && shown.stale {
            // Bonds and charges come from the drawing, which no longer
            // describes these geometries.
            self.status = "The molecule changed: export XYZ, or recalculate".into();
            self.error = true;
            return Task::none();
        }
        let export = match self.xtb_prepare_conformer_export(&shown, format) {
            Ok(export) => export,
            Err(error) => {
                self.status = error;
                self.error = true;
                return Task::none();
            }
        };
        self.export_ready(export, cfg!(target_arch = "wasm32"))
    }

    /// Cancel while re-ranking: stop the workers and keep what finished.
    /// `false` when Cancel should end the run as usual.
    pub(in crate::app::xtb) fn xtb_search_stop(&mut self) -> bool {
        let Some(run) = self.tab.xtb.run.as_mut() else {
            return false;
        };
        let Some(live) = run.search.as_mut() else {
            return false;
        };
        // Making the 3D model first: Cancel ends the run as usual. Once the
        // search runs, its thread knows best what finished, so the run
        // waits for its last word: the kept conformers, or nothing.
        if run.launch.is_none() {
            return false;
        }
        if live.stopping {
            return true;
        }
        live.stopping = true;
        #[cfg(not(target_arch = "wasm32"))]
        run.cancel.cancel();
        self.status = run.status();
        self.error = false;
        true
    }

    /// Start the search for the run in flight from its molecule's model.
    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::app::xtb) fn xtb_search_start(
        &mut self,
        launch: reshiki::science::Launch,
    ) -> Task<Message> {
        let count = launch.frame.numbers.len();
        let Some(run) = self.tab.xtb.run.as_ref() else {
            return Task::none();
        };
        let Some(live) = run.search.as_ref() else {
            return Task::none();
        };
        if let Err(failure) = live.config.check_atoms(count) {
            self.xtb_fail(failure);
            return Task::none();
        }
        let mut search = match search_for(
            &self.tab.doc,
            &run.atoms,
            &launch,
            &live.config,
            &run.config,
        ) {
            Ok(search) => search,
            Err(failure) => {
                self.xtb_fail(failure);
                return Task::none();
            }
        };
        search.describe_unpinned_stereo(|ids| {
            let names: Vec<_> = ids
                .iter()
                .map(|&id| self.xtb_atom_name(&launch.atoms, id).0)
                .collect();
            super::view::unpinned_stereo_note(&names)
        });
        let (epoch, revision) = (self.tab.file_epoch, self.tab.revision);
        let Some(run) = self.tab.xtb.run.as_mut() else {
            return Task::none();
        };
        run.label = format!(
            "xTB · conformers · {}",
            state::formula(&launch.frame.numbers)
        );
        run.launch = Some(launch);
        run.revision = (epoch, revision);
        run.started = iced::time::Instant::now();
        let serial = run.serial;
        let cancel = run.cancel.clone();
        let events = iced::futures::StreamExt::flatten(iced::futures::stream::once(async move {
            super::pipeline::start(search, cancel)
        }));
        let (task, handle) = Task::run(events, move |event| {
            Message::Xtb(XtbAction::Search(serial, event))
        })
        .abortable();
        run.handle = Some(handle.abort_on_drop());
        self.status = run.status();
        self.error = false;
        task
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::app::xtb) fn xtb_search_event(&mut self, serial: u64, event: super::Event) {
        let Some(run) = self.tab.xtb.run.as_mut().filter(|run| run.serial == serial) else {
            return;
        };
        match event {
            super::Event::Phase(phase) => {
                if let Some(live) = run.search.as_mut() {
                    live.phase = phase;
                }
                self.status = run.status();
                self.error = false;
            }
            super::Event::Tick => {
                self.status = run.status();
                self.error = false;
            }
            super::Event::Done(Ok(found)) => match pending(run, &found) {
                Ok(pending) => {
                    self.tab.xtb.run = None;
                    self.xtb_store(pending, false);
                }
                Err(failure) => self.xtb_fail(failure),
            },
            // Cancelled before any conformer was kept.
            super::Event::Done(Err(state::Failure::Other(message)))
                if message == "Calculation canceled"
                    && run.search.as_ref().is_some_and(|live| live.stopping) =>
            {
                self.tab.xtb.run = None;
                self.status = "Calculation canceled".into();
                self.error = false;
            }
            super::Event::Done(Err(failure)) => self.xtb_fail(failure),
        }
    }
}

/// Prepare the pipeline input from the drawing and the launched model frame.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn search_for(
    doc: &Document,
    run_atoms: &[u64],
    launch: &Launch,
    search: &super::Config,
    calc: &state::Config,
) -> Result<super::Search, state::Failure> {
    super::Search::from_launch(
        doc,
        run_atoms,
        launch,
        search.clone(),
        crate::app::xtb::job(calc, &launch.frame, vec![]),
        reshiki::geometry::Client::default(),
        None,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn pending(run: &crate::app::xtb::Run, found: &super::Found) -> Result<Pending, state::Failure> {
    let launch = run
        .launch
        .as_ref()
        .ok_or_else(|| state::Failure::Other("the search finished before it started".into()))?;
    let live = run
        .search
        .as_ref()
        .ok_or_else(|| state::Failure::Other("not a conformer search".into()))?;
    pending_for(launch, &run.config, &live.config, run.keep_full, found)
}

/// The result to store from a finished search: the lowest conformer is its
/// frame, every kept conformer its trajectory.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn pending_for(
    launch: &Launch,
    calc: &state::Config,
    search: &super::Config,
    keep_full: bool,
    found: &super::Found,
) -> Result<Pending, state::Failure> {
    let failed = |error: &dyn std::fmt::Display| state::Failure::Other(error.to_string());
    let best = found
        .ranked
        .first()
        .ok_or_else(|| state::Failure::Other("No conformer was found".into()))?;
    let frame = qcdata::Frame {
        positions_angstrom: best.positions.clone(),
        ..(*launch.frame).clone()
    };
    let method = calc.method;
    let gfnff_reference = (method == qcdata::XtbMethod::GfnFf).then_some(launch.frame.hash());
    let computed = &found.best.computed;
    let mut result = computed
        .to_calc_result(
            &frame,
            qcdata::Task::Conformers,
            &found.engine,
            gfnff_reference,
        )
        .map_err(|e| failed(&e))?;
    let trajectory = found.trajectory(&frame)?;
    let kept = trajectory.steps();
    let effective = super::Config {
        minimizer: found.minimizer,
        ..search.clone()
    };
    result.provenance.method = Some(crate::app::xtb::run::optimizer_provenance(
        &effective.describe(method),
        &found.optimizers,
    ));
    result.input_frame = Some(launch.frame.hash());
    result.status = if found.stopped {
        qcdata::Status::NotConverged {
            reason: format!(
                "Canceled after {} of {} representatives",
                found.reranked, found.planned
            ),
        }
    } else {
        qcdata::Status::Converged
    };
    result.trajectory = Some(trajectory);
    result.keep_full = keep_full;
    // The run's settings and summary, as an optimization records them: the
    // whole search's time, and the SCF work of the lowest conformer's run.
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.threads = Some(calc.threads);
        settings.convergence = Some(if calc.tight {
            qcdata::Convergence::Tight
        } else {
            qcdata::Convergence::Normal
        });
    }
    result.run = Some(qcdata::RunSummary {
        seconds: found.seconds,
        scf_iterations: found
            .best
            .evaluations
            .iter()
            .map(|evaluation| evaluation.scf_iterations as u32)
            .collect(),
        finished: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        search: Some(qcdata::SearchSummary {
            kept,
            ..found.search.clone()
        }),
    });
    let mut notes = found.notes.clone();
    if found.stopped {
        notes.push(format!(
            "Canceled. {kept} re-ranked conformers are kept and usable."
        ));
    }
    if computed.near_degenerate {
        notes.push(state::NEAR_DEGENERATE.into());
    }
    let line = format!(
        "{kept} conformer{} kept · {} starts · {} cluster{} · lowest {}",
        if kept == 1 { "" } else { "s" },
        found.starts,
        found.clusters,
        if found.clusters == 1 { "" } else { "s" },
        state::summary(found.best.energy, None, None, None),
    );
    let info = RunInfo {
        gap_ev: computed.homo_lumo_gap_ev,
        near_degenerate: computed.near_degenerate,
        evaluations: found.best.evaluations.clone(),
        numerics: computed.numerics.clone(),
    };
    Ok(Pending {
        pushed: false,
        accepted: None,
        launch: launch.clone(),
        frames: vec![frame],
        result,
        line,
        notes,
        info,
    })
}
