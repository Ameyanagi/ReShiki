//! Element, charge, isotope, radical and mark edits to selected atoms.
use super::App;
use crate::canvas::Tool;
use reshiki::chemistry::isotopes::{is_known_nuclide, nuclide_symbol, parse_mass_number};

impl App {
    pub(super) fn rotate_mark(&mut self, id: u64, index: usize) {
        let before = self.tab.doc.clone();
        if let Some(a) = self.tab.doc.atom_mut(id)
            && let Some(m) = a.marks.get_mut(index)
        {
            m.angle = (m.angle + 45.).rem_euclid(360.);
        }
        self.changed(before);
    }
    pub(super) fn remove_mark(&mut self, id: u64, index: usize) {
        let before = self.tab.doc.clone();
        let crystal = self
            .tab
            .doc
            .models
            .iter()
            .any(|m| m.cell.is_some() && m.contains(id));
        if let Some(a) = self.tab.doc.atom_mut(id)
            && index < a.marks.len()
        {
            let mark = a.marks.remove(index);
            if mark.kind.charge() {
                a.charge = 0;
            }
            if mark.kind.radical() {
                a.radical_electrons = 0;
            }
            if mark.kind.charge() || mark.kind.radical() {
                a.explicit_h = 0;
                a.no_implicit = crystal;
                self.tab.doc.invalidate_chemistry(&[id]);
            }
        }
        self.changed(before);
    }
    pub(super) fn set_radical(&mut self, value: u8) {
        let before = self.tab.doc.clone();
        self.tab.doc.invalidate_chemistry(&self.tab.selected);
        for atom in self
            .tab
            .doc
            .atoms
            .iter_mut()
            .filter(|a| self.tab.selected.contains(&a.id))
        {
            atom.radical_electrons = value;
            atom.explicit_h = 0;
            atom.no_implicit = false;
        }
        self.changed(before);
    }
    pub(super) fn choose_element(&mut self, element: String) {
        self.element = element;
        self.tool = Tool::Atom;
    }
    pub(super) fn apply_custom_element(&mut self) {
        let symbol = self.custom_element.trim();
        if reshiki::editing::ELEMENTS.contains(&symbol) {
            self.element = symbol.into();
            self.tool = Tool::Atom;
            self.status = format!("Place {} atoms", self.element);
            self.error = false;
        } else {
            self.status = "Enter an element symbol, for example Si, Fe, Na or H".into();
            self.error = true;
        }
    }
    pub(super) fn change_charge(&mut self, delta: i32) {
        let before = self.tab.doc.clone();
        self.tab.doc.invalidate_chemistry(&self.tab.selected);
        for id in &self.tab.selected {
            let crystal = self
                .tab
                .doc
                .models
                .iter()
                .any(|m| m.cell.is_some() && m.contains(*id));
            if let Some(a) = self.tab.doc.atom_mut(*id) {
                a.charge = a.charge.saturating_add(delta).clamp(-8, 8);
                a.display.hide_charge = false;
                a.explicit_h = 0;
                a.no_implicit = crystal;
            }
        }
        self.changed(before);
    }
    pub(super) fn apply_isotope(&mut self) {
        let mut edits = Vec::new();
        let mut warnings = std::collections::BTreeSet::new();
        for atom in self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id))
        {
            let value = match parse_mass_number(&self.tab.isotope, &atom.element) {
                Ok(value) => value,
                Err(error) => {
                    self.status = error;
                    self.error = true;
                    return;
                }
            };
            if value != 0 && !is_known_nuclide(&atom.element, value) {
                warnings.insert(format!(
                    "{} is not a known nuclide",
                    nuclide_symbol(&atom.element, value)
                ));
            }
            edits.push((atom.id, value));
        }
        let before = self.tab.doc.clone();
        for (id, value) in edits {
            if let Some(atom) = self.tab.doc.atom_mut(id) {
                atom.isotope = value;
            }
        }
        self.error = false;
        self.status = "Mass number applied".into();
        self.changed(before);
        if !self.error && !warnings.is_empty() {
            self.status = format!(
                "Mass number applied; {}",
                warnings.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
    }
}

#[cfg(test)]
mod tests;
