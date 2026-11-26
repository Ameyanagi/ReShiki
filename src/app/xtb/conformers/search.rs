//! A conformer search as the app runs it: the card's fields, the start
//! from the molecule's 3D model, progress, Cancel (keeping what is
//! re-ranked), the stored result and the tab's overlay and export.
use super::{Action, Format, Row, Table};
#[cfg(not(target_arch = "wasm32"))]
use crate::app::xtb::{Action as XtbAction, Pending, results::RunInfo, state};
use crate::app::{App, Message};
use iced::Task;
#[cfg(not(target_arch = "wasm32"))]
use reshiki::qcdata;
use std::sync::Arc;

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
    pub(in crate::app::xtb) fn xtb_conformer_rows(
        &self,
        shown: &crate::app::xtb::palette::Shown<'_>,
    ) -> Vec<Row> {
        let revision = (self.tab.file_epoch, self.tab.revision);
        let mut cache = self.tab.xtb.palette.ensemble.table.borrow_mut();
        let fresh = cache.as_ref().is_some_and(|table| {
            table.revision == revision && crate::app::xtb::is(&table.result, &shown.stored.result)
        });
        if !fresh {
            *cache = Some(Table {
                result: Arc::downgrade(&shown.stored.result),
                revision,
                rows: super::rows(
                    &self.tab.doc,
                    shown.atoms,
                    &shown.stored.result,
                    shown.stale,
                ),
            });
        }
        cache
            .as_ref()
            .map_or_else(Vec::new, |table| table.rows.clone())
    }

    fn xtb_export_conformers(&mut self, format: Format) -> Task<Message> {
        let Some(shown) = self.xtb_shown() else {
            return Task::none();
        };
        let stem = std::path::Path::new(&self.document_name())
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Molecule".into());
        let result = &shown.stored.result;
        if format == Format::Sdf && shown.stale {
            // Bonds and charges come from the drawing, which no longer
            // describes these geometries.
            self.status = "The molecule changed: export XYZ, or recalculate".into();
            self.error = true;
            return Task::none();
        }
        let text = match format {
            Format::Xyz => super::export::xyz(result, &stem),
            Format::Sdf => super::export::sdf(&self.tab.doc, shown.atoms, result, &stem),
        };
        let text = match text {
            Ok(text) => text,
            Err(error) => {
                self.status = error;
                self.error = true;
                return Task::none();
            }
        };
        let extension = match format {
            Format::Xyz => "xyz",
            Format::Sdf => "sdf",
        };
        self.export_ready(
            crate::app::figure_export::Prepared::new(
                text.into_bytes(),
                extension,
                Some(format!("{stem} conformers")),
                vec![],
                crate::app::figure_export::Kind::Xtb,
            ),
            cfg!(target_arch = "wasm32"),
        )
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
        if count > super::MAX_ATOMS {
            self.xtb_fail(state::Failure::TooMany {
                count,
                limit: super::MAX_ATOMS,
            });
            return Task::none();
        }
        if live.config.minimizer == super::Minimizer::Gfn2 && count > super::MAX_GFN2_START_ATOMS {
            self.xtb_fail(state::Failure::Other(format!(
                "GFN2-xTB minimizes every start up to {} atoms; choose GFN-FF or MMFF94s",
                super::MAX_GFN2_START_ATOMS
            )));
            return Task::none();
        }
        let prepared = match reshiki::geometry::Prepared::new(&self.tab.doc, &run.atoms) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.xtb_fail(state::Failure::Other(error.to_string()));
                return Task::none();
            }
        };
        let Some(parents) = hydrogen_parents(&self.tab.doc, &launch) else {
            self.xtb_fail(state::Failure::Other(
                "The 3D model's hydrogens cannot be matched; update the 3D model".into(),
            ));
            return Task::none();
        };
        let bonds = super::export::frame_bonds(
            &self.tab.doc,
            &launch.atoms,
            &launch.frame.positions_angstrom,
        )
        .into_iter()
        .map(|(a, b, _)| [a, b])
        .collect();
        let search = super::Search {
            prepared,
            numbers: launch.frame.numbers.clone(),
            atoms: launch.atoms.clone(),
            parents,
            bonds,
            config: live.config.clone(),
            job: crate::app::xtb::run::Job {
                // One GFN-FF topology, from the 3D model, for every start: the
                // result records that frame as its topology reference.
                topology: (run.config.method == qcdata::XtbMethod::GfnFf)
                    .then(|| launch.frame.positions_angstrom.clone()),
                ..crate::app::xtb::job(&run.config, &launch)
            },
        };
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

/// For each frame atom the 3D model added (a hydrogen the drawing does not
/// show), the drawing atom it is bonded to.
#[cfg(not(target_arch = "wasm32"))]
fn hydrogen_parents(
    doc: &reshiki::document::Document,
    launch: &reshiki::science::Launch,
) -> Option<Vec<Option<u64>>> {
    let first = launch.atoms.iter().flatten().next()?;
    let model = doc.models.iter().find(|m| m.linked_atoms.contains(first))?;
    let sources = model.geometry_sources();
    let heavy = model.atoms.len();
    let mut parents = vec![None; heavy];
    for hydrogen in &model.hydrogens {
        // Generated models index their parents; imported ones use drawing IDs.
        let parent = if model.generated.is_some() {
            *sources.get(usize::try_from(hydrogen.parent).ok()?)?
        } else {
            Some(hydrogen.parent)
        };
        parents.push(Some(parent?));
    }
    (parents.len() == launch.atoms.len()).then_some(parents)
}

/// The result to store from a finished search: the lowest conformer is its
/// frame, every kept conformer its trajectory.
#[cfg(not(target_arch = "wasm32"))]
fn pending(run: &crate::app::xtb::Run, found: &super::Found) -> Result<Pending, state::Failure> {
    let failed = |error: &dyn std::fmt::Display| state::Failure::Other(error.to_string());
    let launch = run
        .launch
        .clone()
        .ok_or_else(|| state::Failure::Other("the search finished before it started".into()))?;
    let live = run
        .search
        .as_ref()
        .ok_or_else(|| state::Failure::Other("not a conformer search".into()))?;
    let best = found
        .ranked
        .first()
        .ok_or_else(|| state::Failure::Other("No conformer was found".into()))?;
    let frame = qcdata::Frame {
        positions_angstrom: best.positions.clone(),
        ..(*launch.frame).clone()
    };
    let method = run.config.method;
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
    let mut trajectory = reshiki_optimize::conformers::trajectory(
        &found.ranked,
        &frame.numbers,
        frame.charge,
        frame.multiplicity,
    )
    .map_err(|e| failed(&e))?;
    trajectory.isotopes = frame.isotopes.clone();
    trajectory.masses_amu = frame.masses_amu.clone();
    let kept = trajectory.steps();
    result.provenance.method = Some(live.config.describe(method));
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
    result.keep_full = run.keep_full;
    // The run's settings and summary, as an optimization records them: the
    // whole search's time, and the SCF work of the lowest conformer's run.
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.threads = Some(run.config.threads);
        settings.convergence = Some(if run.config.tight {
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
    });
    let mut notes = found.notes.clone();
    if found.stopped {
        notes.push(format!(
            "Canceled. {} re-ranked conformers are kept and usable.",
            found.reranked
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
        state::summary(found.best.energy, computed.homo_lumo_gap_ev, None),
    );
    let info = RunInfo {
        gap_ev: computed.homo_lumo_gap_ev,
        near_degenerate: computed.near_degenerate,
        evaluations: found.best.evaluations.clone(),
        numerics: computed.numerics.clone(),
    };
    Ok(Pending {
        launch,
        frames: vec![frame],
        result,
        line,
        notes,
        info,
    })
}
