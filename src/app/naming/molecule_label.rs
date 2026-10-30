//! Direct molecule-context naming, using the same bounded local worker and
//! native identity gate as the Chemical names inspector.
use super::*;

pub(super) fn target(doc: &Document, hit: &[u64], selected: &[u64]) -> Option<Vec<u64>> {
    let ids = if hit.is_empty() { selected } else { hit };
    let atoms: Vec<_> = ids
        .iter()
        .copied()
        .filter(|id| doc.atom(*id).is_some())
        .collect();
    let component = reshiki::molecule_names::component(doc, *atoms.first()?);
    atoms
        .iter()
        .all(|id| component.contains(id))
        .then_some(component)
}

impl App {
    pub(super) fn show_molecule_name(&mut self, atoms: Vec<u64>) -> Task<Message> {
        if !self.naming_can_show_molecule() {
            return Task::none();
        }
        if target(&self.tab.doc, &atoms, &[]).as_ref() != Some(&atoms) {
            self.status = "Select one complete molecule to show its chemical name".into();
            self.error = true;
            return Task::none();
        }
        let (ticket, cancel) = self.begin_naming(Pending::Caption);
        self.tab.naming.label_target = Some(atoms.clone());
        let document = self.tab.doc.clone();
        self.status = "Generating chemical name locally…".into();
        self.error = false;
        Task::perform(generate_for_atoms(document, atoms, cancel), move |result| {
            Message::Naming(Action::Finished(ticket, Box::new(result)))
        })
    }

    pub(super) fn place_molecule_name(&mut self, atoms: &[u64], record: Record) {
        let Some(text) = record.systematic_name else {
            self.status = "The local generator returned no supported systematic name".into();
            self.error = true;
            return;
        };
        self.place_molecule_caption(atoms, text, record.canonical_smiles);
    }

    pub(super) fn place_molecule_caption(&mut self, atoms: &[u64], text: String, identity: String) {
        let before = self.tab.doc.clone();
        let result = reshiki::molecule_names::show(
            &mut self.tab.doc,
            atoms,
            text.clone(),
            identity,
            self.tab.caption_format.clone(),
        );
        if let Err(error) = result {
            self.tab.doc = before;
            self.tab.naming.notice = Some(
                Notice::new(NoticeKind::Failed, error.clone()).with_source(NoticeSource::Generate),
            );
            self.status = error;
            self.error = true;
            return;
        }
        self.changed(before);
        if !self.error {
            self.status = format!("Chemical name shown: {text} · Undo removes it");
        }
    }

    pub(super) fn hide_molecule_name(&mut self, atoms: &[u64]) {
        if !self.naming_can_show_molecule() {
            return;
        }
        let before = self.tab.doc.clone();
        if reshiki::molecule_names::hide(&mut self.tab.doc, atoms) {
            self.changed(before);
            if !self.error {
                self.status = "Chemical name hidden · Undo restores it".into();
            }
        }
    }
}

#[cfg(test)]
mod tests;
