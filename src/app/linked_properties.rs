//! Property captions shown from the Properties panel pins and the right-click
//! "Show properties" menu. The model keeps them in step with every edit; the
//! automatic name line is generated here, outside the edit's transaction.
use super::naming::{Action, Outcome, generate_for_atoms};
use super::{App, Message};
use iced::Task;
use reshiki::linked_properties::{self, Property};
use reshiki::naming;
use reshiki::{document::Document, typography::TextFormat};

impl App {
    /// The one whole molecule the selection touches, or the drawing's only
    /// molecule when nothing is selected. A salt's ions are separate molecules.
    pub(super) fn property_target(&self, hit: &[u64]) -> Option<Vec<u64>> {
        let doc = &self.tab.doc;
        let ids = if hit.is_empty() {
            &self.tab.selected
        } else {
            hit
        };
        let atoms: Vec<_> = if ids.is_empty() {
            doc.atoms.iter().map(|a| a.id).collect()
        } else {
            ids.iter()
                .copied()
                .filter(|id| doc.atom(*id).is_some())
                .collect()
        };
        let molecule = reshiki::molecule_names::component(doc, *atoms.first()?);
        atoms
            .iter()
            .all(|id| molecule.contains(id))
            .then_some(molecule)
    }

    pub(super) fn toggle_property(&mut self, atoms: &[u64], property: Property) {
        // Showing a name again retries structures that could not be named.
        if property == Property::Name {
            self.tab.naming.caption_failed.clear();
        }
        let shown = linked_properties::shown(&self.tab.doc, atoms, property);
        if self.edit_captions(|doc, format, two| {
            linked_properties::toggle(doc, atoms, property, format, two)
        }) {
            self.status = format!(
                "{} {} · Undo {} it",
                property.name(),
                if shown { "hidden" } else { "shown" },
                if shown { "restores" } else { "removes" }
            );
        }
    }

    /// Formula, Mol. Wt. and Exact Mass under the molecule, in one Undo step.
    pub(super) fn show_masses(&mut self, atoms: &[u64]) {
        if self.edit_captions(|doc, format, two| {
            linked_properties::show(doc, atoms, &Property::MASSES, format, two)
        }) {
            self.status = "Formula and masses shown".into();
        }
    }

    /// The decimal places of a mass line under the molecule.
    pub(super) fn set_property_places(&mut self, atoms: &[u64], property: Property, places: u8) {
        if self.edit_captions(|doc, _, two| {
            linked_properties::set_places(doc, atoms, property, places, two)
        }) {
            self.status = format!("{} decimals: {places}", property.name());
        }
    }

    /// Run a caption edit as one Undo step. Returns whether it applied.
    pub(super) fn edit_captions(
        &mut self,
        edit: impl FnOnce(&mut Document, TextFormat, bool) -> Result<(), String>,
    ) -> bool {
        let before = self.tab.doc.clone();
        if let Err(error) = edit(
            &mut self.tab.doc,
            self.tab.caption_format.clone(),
            self.appearance.two_decimal_weight,
        ) {
            self.tab.doc = before;
            self.status = error;
            self.error = true;
            return false;
        }
        self.changed(before);
        !self.error
    }

    /// The Name field and the name line's double-click both end here. An
    /// empty name returns to the automatic name.
    pub(super) fn rename_molecule(&mut self, atoms: &[u64], name: &str) {
        self.tab.naming.name_draft = None;
        if self.edit_captions(|doc, format, two| {
            linked_properties::rename(doc, atoms, name, format, two)
        }) {
            self.status = if name.trim().is_empty() {
                "Automatic name shown".into()
            } else {
                "Custom name set".into()
            };
        }
    }

    /// Generate the automatic name of the first caption waiting for one. One
    /// request runs at a time; a structure the generator refused is not asked
    /// again, and one it named before (Undo, Redo) is named at once.
    pub(super) fn start_caption_names(&mut self) -> Task<Message> {
        if !self.naming_can_edit() {
            return Task::none();
        }
        let state = &self.tab.naming;
        let known: Vec<_> = self
            .tab
            .doc
            .property_captions
            .iter()
            .filter(|link| link.needs_name())
            .filter_map(|link| {
                Some((
                    link.smiles.clone(),
                    state.caption_names.get(&link.smiles)?.clone(),
                ))
            })
            .collect();
        for (smiles, name) in known {
            self.write_caption_name(&smiles, &name);
        }
        let state = &self.tab.naming;
        if state.caption_pending.is_some() {
            return Task::none();
        }
        let Some(link) = self
            .tab
            .doc
            .property_captions
            .iter()
            .find(|link| link.needs_name() && !state.caption_failed.contains_key(&link.smiles))
        else {
            return Task::none();
        };
        let smiles = link.smiles.clone();
        let atoms = link.atoms.clone();
        self.tab.naming.caption_pending = Some(smiles.clone());
        Task::perform(
            generate_for_atoms(
                self.tab.doc.clone(),
                atoms,
                naming::Cancel::default(),
                self.appearance.write_unknown_stereo,
            ),
            move |result| Message::Naming(Action::CaptionNamed(smiles.clone(), Box::new(result))),
        )
    }

    /// Write the generated name into the drawing as it stands, with no Undo
    /// step of its own: Undo returns to the drawing before the edit that
    /// needed the name. A caption whose structure changed meanwhile names
    /// a different identity and keeps waiting.
    pub(super) fn caption_named(&mut self, smiles: &str, result: Result<Outcome, naming::Error>) {
        if self.tab.naming.caption_pending.as_deref() != Some(smiles) {
            return;
        }
        self.tab.naming.caption_pending = None;
        let name = match result {
            Ok(Outcome::Structure(record)) => record
                .systematic_name
                .ok_or_else(|| "The local generator returned no supported systematic name".into()),
            Ok(_) => return,
            Err(naming::Error::Cancelled(_)) => return,
            Err(error) => Err(error.to_string()),
        };
        match name {
            Ok(name) => {
                self.tab
                    .naming
                    .caption_names
                    .insert(smiles.into(), name.clone());
                // A text edit or a modal tool owns the drawing; the next
                // update writes the name after it.
                if self.naming_can_edit() {
                    self.write_caption_name(smiles, &name);
                }
            }
            Err(error) => {
                let waiting = self
                    .tab
                    .doc
                    .property_captions
                    .iter()
                    .any(|link| link.needs_name() && link.smiles == smiles);
                if waiting {
                    self.status = format!("No automatic name: {error}");
                    self.error = true;
                    self.tab.naming.caption_failed.insert(smiles.into(), error);
                }
            }
        }
    }

    fn write_caption_name(&mut self, smiles: &str, name: &str) {
        // Like background labels: no new revision, so a Cut or Paste waiting
        // for the clipboard still applies.
        let numbered = (!self.tab.doc.compound_numbers.is_empty()).then(|| self.tab.doc.clone());
        if linked_properties::named(&mut self.tab.doc, smiles, name) {
            if let Some(before) = numbered {
                reshiki::compound_numbers::reconcile(&mut self.tab.doc, &before);
            }
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
            // A selected caption's text box shows the new name.
            let inspector = (self.inspector_open, self.inspector_tab);
            self.sync_typography();
            (self.inspector_open, self.inspector_tab) = inspector;
        }
    }
}

#[cfg(test)]
mod tests;
