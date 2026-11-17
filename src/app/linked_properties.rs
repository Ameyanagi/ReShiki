//! Property captions shown from the Properties panel pins and the right-click
//! "Show properties" menu. The model keeps them in step with every edit; the
//! automatic name line is generated here, outside the edit's transaction.
use super::naming::{Action, Outcome, generate_for_atoms};
use super::{App, Message};
use iced::Task;
use reshiki::linked_properties::{self, Property};
use reshiki::naming;

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
        let before = self.tab.doc.clone();
        let shown = linked_properties::shown(&before, atoms, property);
        if let Err(error) = linked_properties::toggle(
            &mut self.tab.doc,
            atoms,
            property,
            self.tab.caption_format.clone(),
            self.appearance.two_decimal_weight,
        ) {
            self.tab.doc = before;
            self.status = error;
            self.error = true;
            return;
        }
        self.changed(before);
        if !self.error {
            self.status = format!(
                "{} {} · Undo {} it",
                property.name(),
                if shown { "hidden" } else { "shown" },
                if shown { "restores" } else { "removes" }
            );
        }
    }

    /// The Name field and the name line's double-click both end here. An
    /// empty name returns to the automatic name.
    pub(super) fn rename_molecule(&mut self, atoms: &[u64], name: &str) {
        self.tab.naming.name_draft = None;
        let before = self.tab.doc.clone();
        if let Err(error) = linked_properties::rename(
            &mut self.tab.doc,
            atoms,
            name,
            self.tab.caption_format.clone(),
            self.appearance.two_decimal_weight,
        ) {
            self.tab.doc = before;
            self.status = error;
            self.error = true;
            return;
        }
        self.changed(before);
        if !self.error {
            self.status = if name.trim().is_empty() {
                "Automatic name shown".into()
            } else {
                "Custom name set".into()
            };
        }
    }

    /// Generate the automatic name of the first caption waiting for one. One
    /// request runs at a time; a structure the generator refused is not asked
    /// again.
    pub(super) fn start_caption_names(&mut self) -> Task<Message> {
        if self.tab.naming.caption_pending.is_some() || !self.naming_can_edit() {
            return Task::none();
        }
        let failed = &self.tab.naming.caption_failed;
        let Some(link) = self
            .tab
            .doc
            .property_captions
            .iter()
            .find(|link| link.needs_name() && !failed.contains_key(&link.smiles))
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
        // A text edit or a modal tool owns the drawing; ask again after it.
        if !self.naming_can_edit() {
            return;
        }
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
                // Like background labels: no new revision, so a Cut or Paste
                // waiting for the clipboard still applies.
                if linked_properties::named(&mut self.tab.doc, smiles, &name) {
                    self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
                }
            }
            Err(error) => {
                self.status = format!("No automatic name: {error}");
                self.error = true;
                self.tab.naming.caption_failed.insert(smiles.into(), error);
            }
        }
    }
}

#[cfg(test)]
mod tests;
