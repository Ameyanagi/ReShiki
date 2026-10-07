//! Canvas-local derived data, invalidated by document contents rather than edit hooks.
//! Keeping one snapshot covers imports, undo/redo, styles and in-place edits alike.
use super::{Camera, Document, Rectangle, SelectionBox};
use reshiki::scene::{self, Primitive};
use std::{collections::HashSet, rc::Rc};

#[derive(Default)]
pub(super) struct SceneCache {
    document: Option<Document>,
    scene: Option<Rc<[Primitive]>>,
    ids: Vec<u64>,
    selection: Option<Option<SelectionBox>>,
    markers: Option<Rc<super::markers::Markers>>,
    whole: bool,
    copy: Option<Rc<Document>>,
    /// Keyed by the dragged IDs alone: hovering reads the selection box of
    /// another selection during a drag.
    guides: Option<(Vec<u64>, Option<Rc<super::smart_guides::Layout>>)>,
}

impl SceneCache {
    fn document(&mut self, doc: &Document) {
        if self.document.as_ref() != Some(doc) {
            self.document = Some(doc.clone());
            self.scene = None;
            self.ids.clear();
            self.selection = None;
            self.markers = None;
            self.whole = false;
            self.copy = None;
            self.guides = None;
        }
    }

    fn selected(&mut self, doc: &Document, ids: &[u64]) {
        self.document(doc);
        if self.ids != ids {
            self.ids = ids.to_vec();
            self.selection = None;
            self.markers = None;
            self.copy = None;
            let selected: HashSet<_> = ids.iter().copied().collect();
            let all = doc.all_ids();
            self.whole = !all.is_empty() && all.iter().all(|id| selected.contains(id));
        }
    }

    pub fn whole_document(&mut self, doc: &Document, ids: &[u64]) -> bool {
        self.selected(doc, ids);
        self.whole
    }

    /// The selection extracted for a Ctrl/Cmd drag copy.
    pub fn copy(&mut self, doc: &Document, ids: &[u64]) -> Rc<Document> {
        self.selected(doc, ids);
        self.copy
            .get_or_insert_with(|| Rc::new(reshiki::editing::selection(doc, ids)))
            .clone()
    }

    /// The objects a drag of `ids` can snap to, measured once per drag.
    pub fn guides(
        &mut self,
        doc: &Document,
        ids: &[u64],
    ) -> Option<Rc<super::smart_guides::Layout>> {
        self.document(doc);
        if !matches!(&self.guides, Some((cached, _)) if cached == ids) {
            let layout = super::smart_guides::Layout::new(doc, ids).map(Rc::new);
            self.guides = Some((ids.to_vec(), layout));
        }
        self.guides.as_ref().and_then(|(_, layout)| layout.clone())
    }

    /// A cancelled drag leaves the document and selection unchanged, so its
    /// copy would otherwise stay until one of them changes.
    pub fn release_copy(&mut self) {
        self.copy = None;
    }

    pub fn render(
        &mut self,
        doc: &Document,
        ids: &[u64],
    ) -> (Rc<super::markers::Markers>, Rc<[Primitive]>) {
        self.selected(doc, ids);
        let markers = self
            .markers
            .get_or_insert_with(|| Rc::new(super::markers::Markers::new(doc, ids)))
            .clone();
        let scene = self
            .scene
            .get_or_insert_with(|| scene::primitives(doc).into())
            .clone();
        (markers, scene)
    }

    pub fn selection(
        &mut self,
        doc: &Document,
        ids: &[u64],
        camera: Camera,
        bounds: Rectangle,
    ) -> Option<SelectionBox> {
        self.selected(doc, ids);
        self.selection
            .get_or_insert_with(|| SelectionBox::new(doc, ids, camera, bounds))
            .map(|selection| selection.with_view(camera, bounds))
    }
}

#[cfg(test)]
mod tests;
