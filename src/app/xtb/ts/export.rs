//! Feed the existing input-export dialog from the saved TS, without depending
//! on the current drawing or the geometry temporarily previewed on the canvas.
use crate::app::qc_export::Dialog;
use crate::app::xtb::palette::Shown;
use crate::app::{App, Message};
use iced::Task;
use reshiki::qc_input::{self, Geometry, Job, Output, Settings};
use reshiki::qcdata::{CalcResult, Frame};
use std::sync::Arc;

/// Immutable geometry source for an open export dialog. Arc references keep a
/// result and its endpoint frames together even if the document changes later.
pub(in crate::app) struct Source {
    pub(in crate::app) name: String,
    pub(in crate::app) frame: Arc<Frame>,
    result: Arc<CalcResult>,
    reactant: Option<Arc<Frame>>,
    product: Option<Arc<Frame>>,
}

impl Source {
    fn from_shown(shown: &Shown<'_>, science: &reshiki::science::Science, name: String) -> Self {
        let endpoints = shown.stored.result.endpoints;
        Self {
            name,
            frame: shown.frame.clone(),
            result: shown.stored.result.clone(),
            reactant: endpoints
                .and_then(|endpoints| science.frame(&endpoints.reactant))
                .cloned(),
            product: endpoints
                .and_then(|endpoints| science.frame(&endpoints.product))
                .cloned(),
        }
    }

    pub(in crate::app) fn has_endpoints(&self) -> bool {
        self.reactant.is_some() && self.product.is_some()
    }

    pub(in crate::app) fn label(&self) -> String {
        let hessian = if self.result.hessian_hartree_per_bohr2.is_empty() {
            ""
        } else {
            " · xTB Hessian available"
        };
        format!(
            "Saved transition state · {} atoms{hessian}",
            self.frame.numbers.len()
        )
    }

    pub(in crate::app) fn output(
        &self,
        settings: &Settings,
        stem: &str,
        generator: &str,
    ) -> Result<Output, String> {
        qc_input::write(
            settings,
            &Geometry {
                frame: &self.frame,
                reactant: self.reactant.as_deref(),
                product: self.product.as_deref(),
                result: Some(&self.result),
            },
            stem,
            generator,
        )
        .map_err(|error| error.to_string())
    }
}

impl App {
    pub(in crate::app) fn open_qc_export_source(&mut self, source: Source) -> Task<Message> {
        let atoms = source
            .frame
            .drawing_atoms
            .iter()
            .flatten()
            .copied()
            .collect();
        let dialog = Dialog::new(
            atoms,
            source.name.clone(),
            source.frame.charge,
            source.frame.multiplicity,
        )
        .with_source(source, Job::TransitionState);
        self.show_qc_export(dialog)
    }

    pub(in crate::app::xtb) fn xtb_ts_export(&mut self) -> Task<Message> {
        let Some(shown) = self
            .xtb_shown()
            .filter(|shown| shown.stored.result.task == reshiki::qcdata::Task::TransitionState)
        else {
            return Task::none();
        };
        let atoms: Vec<_> = shown.atoms.iter().flatten().copied().collect();
        let name = reshiki::molecule_names::label(&self.tab.doc, &atoms)
            .map(|name| format!("{} TS", name.text))
            .or_else(|| self.export_stem().map(|stem| format!("{stem} TS")))
            .unwrap_or_else(|| "transition-state".into());
        let source = Source::from_shown(&shown, &self.tab.doc.science, name);
        self.open_qc_export_source(source)
    }
}

#[cfg(test)]
mod tests;
