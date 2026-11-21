//! Compound numbering uses the normal caption transaction and text editor.
use super::App;
use reshiki::compound_numbers;

impl App {
    pub(super) fn number_compound(&mut self, atoms: &[u64]) {
        if self.edit_captions(|doc, format, _| compound_numbers::number(doc, atoms, format)) {
            self.status = "Compound numbered".into();
        }
    }

    pub(super) fn renumber_compounds(&mut self) {
        if self.edit_captions(|doc, _, _| {
            compound_numbers::renumber(doc);
            Ok(())
        }) {
            self.status = "Compounds renumbered · Edited labels kept".into();
        }
    }
}

#[cfg(test)]
mod tests;
