//! Property captions shown from the Properties panel pins and the right-click
//! "Show properties" menu. The model keeps them in step with every edit.
use super::App;
use reshiki::linked_properties::{self, Property};

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
}

#[cfg(test)]
mod tests;
