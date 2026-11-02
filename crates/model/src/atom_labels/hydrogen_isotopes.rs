use crate::{
    document::{Atom, Document},
    style::HydrogenIsotopes,
};
use std::cell::OnceCell;
use std::collections::HashSet;

/// Display-only D/T substitutions, shared by scene layout and editable export.
/// Auto checks each connected fragment once, including hidden group members.
pub struct HydrogenIsotopeLabels<'d> {
    doc: &'d Document,
    numeric: OnceCell<HashSet<u64>>,
}

impl<'d> HydrogenIsotopeLabels<'d> {
    pub fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            numeric: OnceCell::new(),
        }
    }

    /// A replacement element symbol, or `None` to retain the mass number.
    pub fn symbol(&self, atom: &Atom) -> Option<&'static str> {
        let symbol = match (atom.element.as_str(), atom.isotope) {
            ("H", 2) => "D",
            ("H", 3) => "T",
            _ => return None,
        };
        match self.doc.drawing_style.hydrogen_isotopes {
            HydrogenIsotopes::Letters => return Some(symbol),
            HydrogenIsotopes::MassNumbers => return None,
            HydrogenIsotopes::Auto => {}
        }
        let numeric = self.numeric.get_or_init(|| {
            let labelled: Vec<_> = self
                .doc
                .atoms
                .iter()
                .filter(|a| a.isotope != 0 && !(a.element == "H" && matches!(a.isotope, 2 | 3)))
                .map(|a| a.id)
                .collect();
            crate::reactions::molecules(self.doc, &labelled)
                .into_iter()
                .flatten()
                .collect()
        });
        (!numeric.contains(&atom.id)).then_some(symbol)
    }
}
