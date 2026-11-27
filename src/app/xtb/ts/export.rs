//! Feed the existing input-export dialog from the saved TS, without depending
//! on the current drawing or the geometry temporarily previewed on the canvas.
use crate::app::qc_export::Dialog;
use crate::app::xtb::EnergyUnit;
use crate::app::{App, Message};
use iced::Task;
use reshiki::qc_input::{self, Geometry, Job, Output, Settings};
use reshiki::qcdata::{CalcResult, Frame};
use std::sync::Arc;

/// Immutable geometry source for an open export dialog. Arc references keep a
/// result and its endpoint frames together even if the document changes later.
#[derive(Clone)]
pub(in crate::app) struct Source {
    pub(in crate::app) name: String,
    pub(in crate::app) frame: Arc<Frame>,
    stem: String,
    result: Arc<CalcResult>,
    reference: Option<&'static str>,
    reactant: Option<Arc<Frame>>,
    product: Option<Arc<Frame>>,
}

impl Source {
    fn from_result(
        result: &Arc<CalcResult>,
        science: &reshiki::science::Science,
        name: String,
        first_reactant: &str,
        formula: Option<&str>,
        reference: Option<&'static str>,
    ) -> Option<Self> {
        let endpoints = result.endpoints;
        let frame = science.frame(&result.frame)?.clone();
        let reactant = endpoints
            .and_then(|endpoints| science.frame(&endpoints.reactant))
            .cloned();
        let fallback =
            || crate::app::xtb::state::formula(&reactant.as_ref().unwrap_or(&frame).numbers);
        let stem = match formula {
            Some(formula) => reaction_stem(first_reactant, formula),
            None => reaction_stem(first_reactant, &fallback()),
        };
        Some(Self {
            name,
            frame,
            stem,
            result: result.clone(),
            reference,
            reactant,
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
        let method = super::view::method_name(&self.result);
        let hessian = if self.result.hessian_hartree_per_bohr2.is_empty() {
            String::new()
        } else {
            format!(" · {method} Hessian available")
        };
        let structure = if super::view::presentation(&self.result, None).reliable() {
            "Saved transition state"
        } else {
            "Saved transition-state candidate"
        };
        format!("{structure} · {method}{hessian}")
    }

    pub(in crate::app) fn output(
        &self,
        settings: &Settings,
        stem: &str,
        generator: &str,
        unit: EnergyUnit,
    ) -> Result<Output, String> {
        // GAMESS also writes the title as an 80-column comment prefixed with "! ".
        const GAMESS_TITLE_BYTES: usize = 78;
        let mut settings = settings.clone();
        let gamess = settings.program == qc_input::Program::Gamess;
        let mut energies = vec![super::view::method_name(&self.result).to_owned()];
        let barrier = super::view::presentation(&self.result, None).barrier_line(unit);
        let include_reaction = !gamess || barrier.is_none();
        if let Some(barrier) = barrier {
            energies.push(barrier);
        }
        if include_reaction && let Some(delta) = super::view::reaction_energy_kcal(&self.result) {
            energies.push(format!("ΔE {}", unit.format(delta)));
        }
        if let Some(delta) = super::view::barrier_gibbs_kcal(&self.result) {
            energies.push(format!("ΔG‡ {}", unit.format(delta)));
        }
        if !gamess && let Some(delta) = super::view::reaction_gibbs_kcal(&self.result) {
            energies.push(format!("ΔG {}", unit.format(delta)));
        }
        if !energies.is_empty() {
            if let Some(reference) = self.reference {
                energies.push(reference.into());
            }
            let separator = " · ";
            let annotation = energies.join(separator);
            if gamess {
                let budget = GAMESS_TITLE_BYTES.saturating_sub(annotation.len() + separator.len());
                settings
                    .title
                    .truncate(settings.title.floor_char_boundary(budget));
            }
            let title = settings.title.trim_end();
            settings.title = if title.is_empty() {
                annotation
            } else {
                format!("{title}{separator}{annotation}")
            };
        }
        if gamess {
            settings
                .title
                .truncate(settings.title.floor_char_boundary(GAMESS_TITLE_BYTES));
        }
        qc_input::write(
            &settings,
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

/// Keep names short and safe in every program's TS, QST, NEB and IRC files.
fn reaction_stem(name: &str, formula: &str) -> String {
    let normalize = |name: &str| {
        let mut stem = String::new();
        for c in name.chars().flat_map(char::to_lowercase) {
            if c.is_alphanumeric() {
                stem.push(c);
            } else if !stem.is_empty() && !stem.ends_with('-') {
                stem.push('-');
            }
        }
        stem.truncate(stem.trim_end_matches('-').len());
        stem
    };
    let stem = normalize(name);
    let stem = if stem.is_empty() || stem.len() > 64 {
        normalize(formula)
    } else {
        stem
    };
    if stem.is_empty() {
        "reaction".into()
    } else {
        stem
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
        let Some(source) = self.xtb_ts_export_source(&shown.stored.result, shown.arrow) else {
            return Task::none();
        };
        self.open_qc_export_source(source)
    }

    /// Export this card's result without changing which result the palette shows.
    pub(in crate::app::xtb) fn xtb_ts_export_arrow(&mut self, arrow: u64) -> Task<Message> {
        let super::card::ArrowResult::Found(result) = self.xtb_ts_card_display_result(arrow) else {
            return Task::none();
        };
        let Some(source) = self.xtb_ts_export_source(result, Some(arrow)) else {
            return Task::none();
        };
        self.open_qc_export_source(source)
    }

    pub(super) fn xtb_ts_export_source(
        &self,
        result: &Arc<CalcResult>,
        arrow: Option<u64>,
    ) -> Option<Source> {
        let doc = &self.tab.doc;
        let reference = self.xtb_ts_reference(result, arrow);
        if let Some(context) =
            super::saved_search(result).and_then(|search| search.context.as_ref())
        {
            let label = arrow
                .filter(|_| !self.xtb_ts_search_stale(result))
                .map(|arrow| self.xtb_reaction_label(arrow));
            return Source::from_result(
                result,
                &doc.science,
                context.title.clone(),
                label
                    .as_ref()
                    .and_then(|label| label.first_reactant())
                    .unwrap_or(&context.reactant),
                label
                    .as_ref()
                    .and_then(|label| label.first_reactant_formula()),
                reference,
            );
        }
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
        if let Some(arrow) = arrow {
            let label = self.xtb_reaction_label(arrow);
            let reactant = label.first_reactant().map_or_else(fallback, str::to_owned);
            Source::from_result(
                result,
                &doc.science,
                label.title(),
                &reactant,
                label.first_reactant_formula(),
                reference,
            )
        } else {
            let name = fallback();
            Source::from_result(result, &doc.science, name.clone(), &name, None, reference)
        }
    }
}

#[cfg(test)]
mod tests;
