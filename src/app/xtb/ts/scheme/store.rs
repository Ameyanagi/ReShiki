//! Attach finished calculations to their drawn molecules. A balancing species
//! without drawing atoms belongs only to the scheme report and placed figure.
use crate::app::App;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Frame};
use reshiki::science::{self, Freshness, Launch};
use std::sync::Arc;

fn same_atoms(order: &[Option<u64>], molecule: &[u64]) -> bool {
    order.iter().flatten().count() == molecule.len()
        && molecule.iter().all(|id| order.contains(&Some(*id)))
}

fn save(
    doc: &mut Document,
    atoms: &[u64],
    launch: Option<&Launch>,
    result: &Arc<CalcResult>,
    frames: &[Arc<Frame>],
) -> Result<(), String> {
    if atoms.is_empty() {
        return Ok(());
    }
    // A cached single-card minimum may have acquired frequencies in this run.
    // In that case the original input frame and drawing order are already stored.
    let stored_launch;
    let launch = match launch {
        Some(launch) if same_atoms(&launch.atoms, atoms) => launch,
        Some(_) => return Ok(()),
        None => {
            let hash = result.input_frame.unwrap_or(result.frame);
            let existing = doc.science.attachments.iter().find(|link| {
                link.frame == hash
                    && same_atoms(&link.atoms, atoms)
                    && link.freshness(doc) == Freshness::Current
            });
            let Some(link) = existing else {
                return Ok(());
            };
            let Some(frame) = doc.science.frame(&hash) else {
                return Ok(());
            };
            stored_launch = Launch {
                frame: Arc::clone(frame),
                atoms: link.atoms.clone(),
                baseline: link.baseline.clone(),
            };
            &stored_launch
        }
    };
    let ids: Vec<_> = launch.atoms.iter().flatten().copied().collect();
    if science::baseline(doc, &ids) != launch.baseline {
        return Ok(());
    }
    if doc.science.results.iter().any(|stored| {
        doc.science
            .attachment(stored.attachment)
            .is_some_and(|link| link.atoms == launch.atoms)
            && (Arc::ptr_eq(&stored.result, result) || stored.result == *result)
    }) {
        return Ok(());
    }
    science::attach(
        doc,
        launch,
        frames.iter().map(|f| f.as_ref().clone()).collect(),
        result.as_ref().clone(),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

impl App {
    pub(in crate::app) fn xtb_finish_pending_scheme(&mut self) {
        if !self.tab.drawing_edit_active()
            && !self.tab.busy
            && let Some(refresh) = self
                .tab
                .xtb
                .scheme
                .as_mut()
                .and_then(|s| s.pending_save.take())
            && self.xtb_scheme_current()
        {
            self.xtb_store_scheme(refresh);
        }
    }

    pub(super) fn xtb_store_scheme(&mut self, refresh: bool) {
        let editing = self.tab.drawing_edit_active() || self.tab.busy;
        let Some(state) = &mut self.tab.xtb.scheme else {
            return;
        };
        if editing {
            state.pending_save = Some(refresh);
            return;
        }
        let before = self.tab.doc.clone();
        let doc = &mut self.tab.doc;
        let mut errors = Vec::new();
        for value in &state.work.report.species {
            let Some(species) = state.plan.species.get(value.species) else {
                continue;
            };
            // Reused results keep the atom correspondence of their original
            // drawing. Never attach a duplicate's frame to the representative.
            for atoms in &species.instances {
                if let Err(reason) = save(
                    doc,
                    atoms,
                    value.launch.as_ref(),
                    &value.result,
                    std::slice::from_ref(&value.frame),
                ) {
                    errors.push(format!("{} · {reason}", species.label));
                }
            }
        }
        for (index, step) in state.work.report.steps.iter().enumerate() {
            let Some(step) = step else {
                continue;
            };
            let (Some(ts), Some(launch)) = (&step.ts, &step.launch) else {
                continue;
            };
            let Ok(reaction) = reshiki::reactions::preview(doc, step.arrow) else {
                continue;
            };
            let drawn = reaction
                .reactants
                .iter()
                .chain(&reaction.products)
                .flat_map(|participant| reshiki::reactions::molecules(doc, &participant.atoms))
                .find(|atoms| same_atoms(&launch.atoms, atoms));
            if let Some(atoms) = drawn
                && let Err(reason) = save(doc, &atoms, Some(launch), ts, &step.frames)
            {
                errors.push(format!("Step {} · {reason}", index + 1));
            }
        }
        let save_error = (!errors.is_empty()).then(|| {
            format!(
                "Could not save every result with the drawing. {}",
                errors.join(" · ")
            )
        });
        if let Some(reason) = &save_error {
            state.work.error = Some(reason.clone());
        }
        if refresh {
            self.xtb_refresh_scheme_profile(before);
        } else if self.tab.doc != before {
            let status = self.status.clone();
            self.changed(before);
            if !self.error {
                self.status = status;
            }
        }
        if let Some(reason) = save_error {
            self.status = reason;
            self.error = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::xtb::{self, run};
    use reshiki::{
        document::{Arrow, Point},
        qcdata,
        reactions::{Participant, Reaction},
    };
    use std::cell::Cell;

    fn water_result() -> (Arc<Frame>, Arc<CalcResult>) {
        let frame = Arc::new(Frame {
            numbers: vec![8, 1, 1],
            positions_angstrom: xtb::tests::START.to_vec(),
            charge: 0,
            multiplicity: 1,
            isotopes: vec![],
            masses_amu: vec![],
            drawing_atoms: vec![],
        });
        let mut result = run::example_computed(-1., 1.)
            .to_calc_result(&frame, qcdata::Task::Optimize, &run::example_engine(), None)
            .unwrap();
        if let qcdata::Settings::Xtb(settings) = &mut result.settings {
            settings.convergence = Some(qcdata::Convergence::Tight);
        }
        result.status = qcdata::Status::Converged;
        (frame, Arc::new(result))
    }

    #[test]
    fn finished_drawn_species_is_reusable_after_saving_and_added_water_stays_detached() {
        let mut doc = Document::default();
        let oxygen = doc.add_atom("O", Point::default());
        let (frame, result) = water_result();
        let launch = Launch {
            atoms: vec![Some(oxygen), None, None],
            baseline: science::baseline(&doc, &[oxygen]),
            frame: frame.clone(),
        };
        let frames = std::slice::from_ref(&frame);
        save(&mut doc, &[], Some(&launch), &result, frames).unwrap();
        assert!(doc.science.results.is_empty());
        for _ in 0..2 {
            save(&mut doc, &[oxygen], Some(&launch), &result, frames).unwrap();
        }
        assert_eq!(doc.science.results.len(), 1);
        let reopened = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert!(
            xtb::ts::energy::scheme_reusable(
                &reopened,
                &[oxygen],
                0,
                1,
                qcdata::XtbMethod::Gfn2Xtb,
                false
            )
            .is_some()
        );
    }

    #[test]
    fn a_duplicate_owned_result_is_saved_with_its_own_drawing_atoms() {
        let mut doc = Document::default();
        let atoms: Vec<_> = [0., 240., 480.]
            .into_iter()
            .map(|x| doc.add_atom("O", Point::new(x, 0.)))
            .collect();
        let mut arrows = Vec::new();
        for index in 0..2 {
            let arrow = doc.next_id();
            doc.arrows.push(Arrow::new(
                arrow,
                Point::new(80. + index as f32 * 240., 0.),
                Point::new(160. + index as f32 * 240., 0.),
                Default::default(),
                Default::default(),
            ));
            let mut reaction = Reaction::new(arrow);
            reaction.reactants = vec![Participant {
                atoms: vec![atoms[index]],
                coefficient: 1,
            }];
            reaction.products = vec![Participant {
                atoms: vec![atoms[index + 1]],
                coefficient: 1,
            }];
            doc.reactions.push(reaction);
            arrows.push(arrow);
        }
        let plan = super::super::plan::Plan::read(&doc, &arrows).unwrap();
        assert_eq!(plan.species.len(), 1);
        let second = plan.species[0].instances[1][0];
        let (frame, result) = water_result();
        let launch = Launch {
            atoms: vec![Some(second), None, None],
            baseline: science::baseline(&doc, &[second]),
            frame: frame.clone(),
        };
        let mut work = super::super::run::State::default();
        work.report.species.push(super::super::run::SpeciesValue {
            species: 0,
            method: qcdata::XtbMethod::Gfn2Xtb,
            frame,
            result,
            launch: Some(launch),
        });
        let (mut app, _) = App::new();
        app.tab.doc = doc;
        app.tab.busy = false;
        app.tab.xtb.scheme = Some(super::super::State {
            options: vec![super::super::run::StepOptions::default(); 2],
            plan,
            method: qcdata::XtbMethod::Gfn2Xtb,
            sheet: false,
            original: None,
            reviews: None,
            work,
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
            checked: Cell::new(None),
            pending_save: None,
        });
        app.xtb_store_scheme(false);
        assert_eq!(app.tab.doc.science.results.len(), 1);
        let link = &app.tab.doc.science.attachments[0];
        assert!(same_atoms(&link.atoms, &[second]));
        assert!(!link.atoms.contains(&Some(atoms[0])));
    }
}
