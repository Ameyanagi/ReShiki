//! Guidance when the drawing does not identify one molecule to calculate.
use super::{App, available};
use reshiki::molecule_names::component;
use std::collections::HashSet;

impl App {
    /// The card subtitle and what to click when there is no target molecule.
    pub(super) fn xtb_selection_hint(&self) -> (String, String) {
        let doc = &self.tab.doc;
        let selected = &self.tab.selected;
        if selected.is_empty() {
            return if doc.atoms.is_empty() {
                (
                    "No molecule".into(),
                    "Draw or add a molecule to calculate it".into(),
                )
            } else {
                (
                    "No molecule selected".into(),
                    "No molecule selected · click one to calculate it".into(),
                )
            };
        }
        let mut seen = HashSet::new();
        let mut count = 0;
        for id in doc.expand_abbreviation_selection(selected) {
            if !seen.contains(&id) && doc.atom(id).is_some() {
                seen.extend(component(doc, id));
                count += 1;
            }
        }
        if count >= 2 {
            let subtitle = format!("{count} molecules selected");
            let message = format!("{subtitle} · click one to calculate it");
            return (subtitle, message);
        }
        (
            "No molecule selected".into(),
            "Select a molecule to calculate it".into(),
        )
    }

    pub(in crate::app) fn xtb_show_selection_hint(&mut self) {
        if !available() {
            self.status = "Select a molecule to calculate".into();
            self.error = true;
            return;
        }
        let (_, hint) = self.xtb_selection_hint();
        self.tab.xtb.selection_hint = Some(hint.clone());
        self.status = hint;
        self.error = false;
    }

    /// Retire the hint once the drawing identifies a molecule.
    pub(in crate::app) fn xtb_clear_selection_hint(&mut self) {
        let Some(hint) = &self.tab.xtb.selection_hint else {
            return;
        };
        if self.status != *hint {
            self.tab.xtb.selection_hint = None;
        } else if self.xtb_target().is_some() {
            self.tab.xtb.selection_hint = None;
            self.status.clear();
            self.error = false;
        }
    }
}

#[cfg(test)]
mod tests;
