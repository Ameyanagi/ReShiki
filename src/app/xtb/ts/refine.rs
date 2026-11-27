//! Resolve a saved saddle and its endpoint frames on the originating reaction.
use super::{Retry, saved_search, view};
use crate::app::{App, Message};
use iced::Task;
use reshiki::{
    qcdata::{CalcResult, TransitionStateId, XtbMethod},
    reactions::ts_plan,
    science::Stored,
};
use std::sync::Arc;

impl App {
    /// Resolve both profiles through the saved source calculation on this arrow.
    pub(in crate::app::xtb) fn xtb_ts_linked_pair(
        &self,
        result: &Arc<CalcResult>,
    ) -> Option<[&Arc<CalcResult>; 2]> {
        let search = saved_search(result)?;
        let arrow = search.arrow?;
        let runs = &self.tab.doc.science.results;
        let shown = &runs.iter().find(|s| Arc::ptr_eq(&s.result, result))?.result;
        if let Some(id) = search.refined_from {
            return Some([&self.xtb_ts_source(arrow, id)?.result, shown]);
        }
        let id = result.transition_state_id()?;
        let child = runs.iter().rfind(|s| {
            saved_search(&s.result)
                .is_some_and(|run| run.arrow == Some(arrow) && run.refined_from == Some(id))
        })?;
        Some([shown, &child.result])
    }

    pub(super) fn xtb_ts_source(&self, arrow: u64, id: TransitionStateId) -> Option<&Stored> {
        self.tab.doc.science.results.iter().rev().find(|stored| {
            saved_search(&stored.result)
                .is_some_and(|s| s.arrow == Some(arrow) && s.method == XtbMethod::Gfn2Xtb)
                && stored.result.transition_state_id() == Some(id)
        })
    }

    fn xtb_gxtb_source(&self, arrow: u64, id: TransitionStateId) -> Option<&Stored> {
        let stored = self.xtb_ts_source(arrow, id)?;
        let result = &stored.result;
        let session = self.tab.xtb.ts_sessions.get(result).map(AsRef::as_ref);
        (view::presentation(result, session).reliable() && !self.xtb_ts_result_stale(result))
            .then_some(stored)
    }

    pub(super) fn xtb_refine_gxtb(
        &mut self,
        arrow: u64,
        source: &Arc<CalcResult>,
    ) -> Task<Message> {
        if !super::super::available() {
            return Task::none();
        }
        let Some(parent) = source.transition_state_id() else {
            return Task::none();
        };
        let Some(stored) = self
            .xtb_gxtb_source(arrow, parent)
            .filter(|stored| Arc::ptr_eq(source, &stored.result))
        else {
            return Task::none();
        };
        let Some(frame) = self.tab.doc.science.frame(&stored.result.frame) else {
            return Task::none();
        };
        let Ok(plan) = ts_plan::plan_for_review(&self.tab.doc, arrow) else {
            return Task::none();
        };
        self.xtb_ts_begin(
            arrow,
            plan,
            reshiki_optimize::ts::TsOptions::default().images,
            (frame.charge, frame.multiplicity - 1),
            self.tab.xtb.ts_card.free_energy,
            Some(Retry::Gxtb(parent)),
        )
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    pub(in crate::app::xtb) fn xtb_gxtb_launch(
        &self,
        arrow: u64,
        parent: TransitionStateId,
    ) -> Option<reshiki::science::Launch> {
        let source = self.xtb_gxtb_source(arrow, parent)?;
        let science = &self.tab.doc.science;
        let link = science.attachment(source.attachment)?;
        Some(reshiki::science::Launch {
            frame: science.frame(&link.frame)?.clone(),
            atoms: link.atoms.clone(),
            baseline: link.baseline.clone(),
        })
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    pub(super) fn xtb_gxtb_path(
        &self,
        arrow: u64,
        parent: TransitionStateId,
    ) -> Option<super::job::SearchPath> {
        let source = self.xtb_gxtb_source(arrow, parent)?;
        let endpoints = source.result.endpoints?;
        let positions = |hash| {
            self.tab
                .doc
                .science
                .frame(&hash)
                .map(|frame| super::job::bohr(&frame.positions_angstrom))
        };
        Some(super::job::SearchPath::Refine {
            parent,
            reactant: positions(endpoints.reactant)?,
            product: positions(endpoints.product)?,
            saddle: positions(source.result.frame)?,
        })
    }
}
