//! Feed the existing input-export dialog from the saved TS, without depending
//! on the current drawing or the geometry temporarily previewed on the canvas.
use crate::app::qc_export::Dialog;
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
    stem: String,
    result: Arc<CalcResult>,
    reactant: Option<Arc<Frame>>,
    product: Option<Arc<Frame>>,
}

impl Source {
    fn from_result(
        result: &Arc<CalcResult>,
        science: &reshiki::science::Science,
        name: String,
        first_reactant: &str,
    ) -> Option<Self> {
        let endpoints = result.endpoints;
        Some(Self {
            name,
            frame: science.frame(&result.frame)?.clone(),
            stem: reaction_stem(first_reactant),
            result: result.clone(),
            reactant: endpoints
                .and_then(|endpoints| science.frame(&endpoints.reactant))
                .cloned(),
            product: endpoints
                .and_then(|endpoints| science.frame(&endpoints.product))
                .cloned(),
        })
    }

    /// Keep the reactant label in the file name when the export job changes.
    pub(in crate::app) fn stem(&self, job: Job) -> String {
        let suffix = match job {
            Job::Qst2 => "qst2",
            Job::Qst3 => "qst3",
            Job::NebTs => "neb",
            Job::Irc => "irc",
            _ => "ts",
        };
        format!("{}-{suffix}", self.stem)
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
        let structure = if super::view::presentation(&self.result, None).reliable() {
            "Saved transition state"
        } else {
            "Saved transition-state candidate"
        };
        format!("{structure} · {} atoms{hessian}", self.frame.numbers.len())
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

/// The name is also a file stem, so punctuation cannot introduce path segments.
fn reaction_stem(name: &str) -> String {
    let stem: String = name
        .split_whitespace()
        .map(|word| {
            word.chars()
                .flat_map(char::to_lowercase)
                .map(|c| {
                    if c.is_alphanumeric() || matches!(c, '-' | '_') {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-");
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "reaction".into()
    } else {
        stem.into()
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
        let arrow = shown
            .ts
            .map(|session| session.arrow)
            .or_else(|| super::saved_search(&shown.stored.result).map(|search| search.arrow))
            .or_else(|| {
                super::unique_matching_plan(&self.tab.doc, shown.atoms).map(|(arrow, _)| arrow)
            });
        let Some(source) = self.xtb_ts_export_source(&shown.stored.result, arrow) else {
            return Task::none();
        };
        self.open_qc_export_source(source)
    }

    /// Export this card's result without changing which result the palette shows.
    pub(in crate::app::xtb) fn xtb_ts_export_arrow(&mut self, arrow: u64) -> Task<Message> {
        let super::card::ArrowResult::Found(result) = self.xtb_ts_card_result(arrow) else {
            return Task::none();
        };
        let Some(source) = self.xtb_ts_export_source(result, Some(arrow)) else {
            return Task::none();
        };
        self.open_qc_export_source(source)
    }

    fn xtb_ts_export_source(&self, result: &Arc<CalcResult>, arrow: Option<u64>) -> Option<Source> {
        let doc = &self.tab.doc;
        let frame = doc.science.frame(&result.frame)?;
        let fallback = || {
            let atoms: Vec<_> = doc
                .science
                .results
                .iter()
                .find(|stored| Arc::ptr_eq(&stored.result, result))
                .and_then(|stored| doc.science.attachment(stored.attachment))
                .into_iter()
                .flat_map(|link| link.atoms.iter().flatten().copied())
                .collect();
            reshiki::molecule_names::label(doc, &atoms)
                .map(|name| name.text.clone())
                .unwrap_or_else(|| crate::app::xtb::state::formula(&frame.numbers))
        };
        let (title, reactant) = match arrow {
            Some(arrow) => {
                let label = self.xtb_reaction_label(arrow);
                (
                    label.title(),
                    label
                        .first_reactant()
                        .map(str::to_owned)
                        .unwrap_or_else(fallback),
                )
            }
            None => {
                let name = fallback();
                (name.clone(), name)
            }
        };
        Source::from_result(result, &doc.science, title, &reactant)
    }
}

#[cfg(test)]
mod tests;
