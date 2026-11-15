use crate::{
    document::{Atom, Document},
    scene::graph::Graph,
    style::HydrogenIsotopes,
};
use std::cell::OnceCell;
use std::collections::HashSet;

/// Display-only D/T substitutions, shared by scene layout and editable export.
/// Auto checks each connected fragment once, including hidden group members.
/// A pass that lays out every label also lends its atom and bond index, so
/// one label's layout does not scan the whole drawing.
pub struct HydrogenIsotopeLabels<'d> {
    doc: &'d Document,
    numeric: OnceCell<HashSet<u64>>,
    graph: Option<&'d Graph<'d>>,
}

impl<'d> HydrogenIsotopeLabels<'d> {
    pub fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            numeric: OnceCell::new(),
            graph: None,
        }
    }

    /// For laying out many labels of `doc`; `graph` must index that document.
    pub(crate) fn indexed(doc: &'d Document, graph: &'d Graph<'d>) -> Self {
        Self {
            graph: Some(graph),
            ..Self::new(doc)
        }
    }

    pub(crate) fn graph(&self) -> Option<&'d Graph<'d>> {
        self.graph
    }

    /// [`Document::atom_visible`].
    pub(crate) fn atom_visible(&self, id: u64) -> bool {
        match self.graph {
            Some(graph) => graph.atom_visible(id),
            None => self.doc.atom_visible(id),
        }
    }

    /// [`super::visible`].
    pub(crate) fn shown(&self, atom: &Atom) -> bool {
        match self.graph {
            Some(graph) => graph.visible(atom),
            None => super::visible(atom, self.doc),
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
