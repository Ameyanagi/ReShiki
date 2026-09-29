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
        }
    }

    fn selected(&mut self, doc: &Document, ids: &[u64]) {
        self.document(doc);
        if self.ids != ids {
            self.ids = ids.to_vec();
            self.selection = None;
            self.markers = None;
            let selected: HashSet<_> = ids.iter().copied().collect();
            let all = doc.all_ids();
            self.whole = !all.is_empty() && all.iter().all(|id| selected.contains(id));
        }
    }

    pub fn whole_document(&mut self, doc: &Document, ids: &[u64]) -> bool {
        self.selected(doc, ids);
        self.whole
    }

    pub fn primitives(&mut self, doc: &Document) -> Rc<[Primitive]> {
        self.document(doc);
        self.scene
            .get_or_insert_with(|| scene::primitives(doc).into())
            .clone()
    }

    pub fn markers(&mut self, doc: &Document, ids: &[u64]) -> Rc<super::markers::Markers> {
        self.selected(doc, ids);
        self.markers
            .get_or_insert_with(|| Rc::new(super::markers::Markers::new(doc, ids)))
            .clone()
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
        assert_eq!(
            format!("{:?}", cache.primitives(doc)),
            format!("{:?}", scene::primitives(doc)),
        );
        assert_eq!(
            *cache.markers(doc, ids),
            super::super::markers::Markers::new(doc, ids)
        );
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
        let first = cache.primitives(&doc);
        let markers = cache.markers(&doc, &ids);
        assert!(Rc::ptr_eq(&markers, &cache.markers(&doc, &ids)));
        assert!(Rc::ptr_eq(&first, &cache.primitives(&doc)));
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
        assert!(!Rc::ptr_eq(&first, &cache.primitives(&doc)));
        assert!(!Rc::ptr_eq(&markers, &cache.markers(&doc, &ids)));
        assert!(history.undo(&mut doc));
        check(&mut cache, &doc, &ids);
        assert!(history.redo(&mut doc));
        check(&mut cache, &doc, &ids);
        // Load another drawing with reused IDs, then load an empty drawing.
        doc = drawing();
        doc.atoms[0].element = "Cl".into();
        check(&mut cache, &doc, &ids);
        check(&mut cache, &Document::default(), &[]);
        assert!(cache.primitives(&Document::default()).is_empty());
    }

    #[test]
    fn selection_changes_do_not_invalidate_scene_or_claim_partial_moves_are_rigid() {
        let doc = drawing();
        let ids = doc.all_ids();
        let mut cache = SceneCache::default();
        let scene = cache.primitives(&doc);
        for selection in [&ids[..], &ids[..1], &ids[1..], &[][..], &ids[..]] {
            check(&mut cache, &doc, selection);
            assert_eq!(cache.whole_document(&doc, selection), selection == ids);
            assert!(Rc::ptr_eq(&scene, &cache.primitives(&doc)));
        }
        assert!(!cache.whole_document(&doc, &[ids[0], ids[0], 999]));
        assert!(!cache.whole_document(&Document::default(), &[]));
    }
}
