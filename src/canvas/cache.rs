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
mod tests {
    use super::*;
    use reshiki::document::{Annotation, History, Point};

    fn drawing() -> Document {
        let mut doc = Document::default();
        let a = doc.add_atom("N", Point::new(0., 0.));
        let b = doc.add_atom("O", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.annotations.push(Annotation {
            id: 100,
            position: Point::new(0., 60.),
            text: "Caption H2O".into(),
            format: Default::default(),
        });
        doc
    }

    fn check(cache: &mut SceneCache, doc: &Document, ids: &[u64]) {
        let (markers, primitives) = cache.render(doc, ids);
        assert_eq!(
            format!("{primitives:?}"),
            format!("{:?}", scene::primitives(doc)),
        );
        assert_eq!(*markers, super::super::markers::Markers::new(doc, ids));
        for (zoom, x, width) in [(1., 0., 400.), (0.5, 150., 800.), (2., -20., 300.)] {
            let camera = Camera {
                center: Point::new(x, -15.),
                zoom,
            };
            let bounds = Rectangle::with_size(iced::Size::new(width, 300.));
            assert_eq!(
                format!("{:?}", cache.selection(doc, ids, camera, bounds)),
                format!("{:?}", SelectionBox::new(doc, ids, camera, bounds)),
            );
        }
    }

    #[test]
    fn document_edits_loads_and_history_cannot_reuse_stale_geometry() {
        let mut cache = SceneCache::default();
        let mut doc = drawing();
        let ids = doc.all_ids();
        check(&mut cache, &doc, &ids);
        let first = cache.render(&doc, &ids).1;
        let markers = cache.render(&doc, &ids).0;
        assert!(Rc::ptr_eq(&markers, &cache.render(&doc, &ids).0));
        assert!(Rc::ptr_eq(&first, &cache.render(&doc, &ids).1));
        // The UI may mutate the same document allocation without changing IDs.
        let before = doc.clone();
        doc.atoms[0].position.x -= 25.;
        doc.bonds[0].order = 2;
        doc.annotations[0].text = "A longer edited caption".into();
        doc.annotations[0].format.style.bold = true;
        doc.annotations[0].format.style.size_pt = 18.;
        let mut history = History::default();
        assert!(history.commit(before, &doc));
        check(&mut cache, &doc, &ids);
        assert!(!Rc::ptr_eq(&first, &cache.render(&doc, &ids).1));
        assert!(!Rc::ptr_eq(&markers, &cache.render(&doc, &ids).0));
        assert!(history.undo(&mut doc));
        check(&mut cache, &doc, &ids);
        assert!(history.redo(&mut doc));
        check(&mut cache, &doc, &ids);
        // Load another drawing with reused IDs, then load an empty drawing.
        doc = drawing();
        doc.atoms[0].element = "Cl".into();
        check(&mut cache, &doc, &ids);
        check(&mut cache, &Document::default(), &[]);
        assert!(cache.render(&Document::default(), &[]).1.is_empty());
    }

    #[test]
    fn selection_changes_do_not_invalidate_scene_or_claim_partial_moves_are_rigid() {
        let doc = drawing();
        let ids = doc.all_ids();
        let mut cache = SceneCache::default();
        let scene = cache.render(&doc, &ids).1;
        for selection in [&ids[..], &ids[..1], &ids[1..], &[][..], &ids[..]] {
            check(&mut cache, &doc, selection);
            assert_eq!(cache.whole_document(&doc, selection), selection == ids);
            assert!(Rc::ptr_eq(&scene, &cache.render(&doc, selection).1));
        }
        assert!(!cache.whole_document(&doc, &[ids[0], ids[0], 999]));
        assert!(!cache.whole_document(&Document::default(), &[]));
    }

    #[test]
    fn guide_layouts_outlive_other_selection_reads_but_not_edits() {
        let mut doc = drawing();
        let ids = doc.all_ids();
        let mut cache = SceneCache::default();
        let dragged = &ids[..2];
        let first = cache.guides(&doc, dragged).unwrap();
        // The pointer reads the current selection's box on every event of a drag.
        let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
        let _ = cache.selection(&doc, &ids[2..], Camera::default(), bounds);
        assert!(Rc::ptr_eq(&first, &cache.guides(&doc, dragged).unwrap()));
        doc.atoms[0].position.x -= 25.;
        assert!(!Rc::ptr_eq(&first, &cache.guides(&doc, dragged).unwrap()));
    }
}
