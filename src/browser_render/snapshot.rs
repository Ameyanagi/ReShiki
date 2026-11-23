//! File JSON omits transient display state. Worker snapshots retain it without
//! changing the saved-file format or trying to recompute the visible drawing.
use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot<'a> {
    #[serde(flatten)]
    document: Cow<'a, Document>,
    #[serde(
        default,
        rename = "_render_visible",
        skip_serializing_if = "Visible::is_empty"
    )]
    visible: Visible,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Visible {
    atoms: Vec<(u64, bool, bool)>,
    bonds: Vec<(u64, u64)>,
    on_other_canvas: BTreeSet<u64>,
    models: Vec<(usize, Option<f32>, bool)>,
}
impl Visible {
    fn capture(document: &Document) -> Self {
        Self {
            atoms: document
                .atoms
                .iter()
                .filter(|a| a.undefined_stereo || a.undefined_pseudoasymmetric)
                .map(|a| (a.id, a.undefined_stereo, a.undefined_pseudoasymmetric))
                .collect(),
            bonds: document
                .bonds
                .iter()
                .filter(|b| b.undefined_stereo)
                .map(|b| (b.a.min(b.b), b.a.max(b.b)))
                .collect(),
            on_other_canvas: document.on_other_canvas.clone(),
            models: document
                .models
                .iter()
                .enumerate()
                .filter(|(_, model)| model.morph.is_some() || model.turning)
                .map(|(index, model)| (index, model.morph, model.turning))
                .collect(),
        }
    }
    fn is_empty(&self) -> bool {
        self.atoms.is_empty()
            && self.bonds.is_empty()
            && self.on_other_canvas.is_empty()
            && self.models.is_empty()
    }
    fn restore(self, document: &mut Document) {
        let atoms: BTreeMap<_, _> = self
            .atoms
            .into_iter()
            .map(|(id, stereo, pseudo)| (id, (stereo, pseudo)))
            .collect();
        for atom in &mut document.atoms {
            let (stereo, pseudo) = atoms.get(&atom.id).copied().unwrap_or_default();
            atom.undefined_stereo = stereo;
            atom.undefined_pseudoasymmetric = pseudo;
        }
        let bonds: BTreeSet<_> = self.bonds.into_iter().collect();
        for bond in &mut document.bonds {
            bond.undefined_stereo = bonds.contains(&(bond.a.min(bond.b), bond.a.max(bond.b)));
        }
        document.on_other_canvas = self.on_other_canvas;
        for (index, morph, turning) in self.models {
            if let Some(model) = document.models.get_mut(index) {
                model.morph = morph;
                model.turning = turning;
            }
        }
    }
}
impl<'a> Snapshot<'a> {
    pub fn borrowed(document: &'a Document) -> Self {
        Self {
            visible: Visible::capture(document),
            document: Cow::Borrowed(document),
        }
    }
    pub fn owned(document: Document) -> Self {
        Self {
            visible: Visible::capture(&document),
            document: Cow::Owned(document),
        }
    }
    pub fn into_document(self) -> Document {
        let mut document = self.document.into_owned();
        self.visible.restore(&mut document);
        document
    }
}
