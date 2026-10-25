//! Canvas-local derived data, invalidated by document contents rather than edit hooks.
//! Keeping one snapshot covers imports, undo/redo, styles and in-place edits alike.
use super::{Camera, Document, Rectangle, SelectionBox, nmr};
use reshiki::rear_opacity::Paint;
use reshiki::scene::{self, Primitive};
use std::{collections::HashSet, rc::Rc};

#[derive(Default)]
pub(super) struct SceneCache {
    document: Option<Document>,
    scene: Option<Rc<[Primitive]>>,
    stretch: Option<Rc<reshiki::editing::reference::Stretch>>,
    stretch_key: Option<(u64, u64)>,
    opacity: Option<Rc<Paint>>,
    ids: Vec<u64>,
    selection: Option<Option<SelectionBox>>,
    markers: Option<Rc<super::markers::Markers>>,
    whole: bool,
    copy: Option<Rc<Document>>,
    /// Keyed by the dragged IDs alone: hovering reads the selection box of
    /// another selection during a drag.
    guides: Option<(Vec<u64>, Option<Rc<super::smart_guides::Layout>>)>,
    nmr: NmrCache,
    foreground_orbital: Option<bool>,
    ring_interiors: Option<Rc<reshiki::editing::RingInteriors>>,
}

#[derive(Default)]
struct NmrCache {
    entry: Option<(NmrKey, Rc<[nmr::Assignment]>)>,
    widths: nmr::WidthMemo,
}

struct NmrKey {
    rows: Vec<(u64, Option<u64>)>,
    mode: nmr::LabelMode,
    camera: [u32; 3],
    bounds: [u32; 4],
}

impl NmrKey {
    fn camera(camera: Camera) -> [u32; 3] {
        [
            camera.center.x.to_bits(),
            camera.center.y.to_bits(),
            camera.zoom.to_bits(),
        ]
    }

    fn bounds(bounds: Rectangle) -> [u32; 4] {
        [
            bounds.x.to_bits(),
            bounds.y.to_bits(),
            bounds.width.to_bits(),
            bounds.height.to_bits(),
        ]
    }

    fn rows<'a>(context: nmr::Context<'a>) -> impl Iterator<Item = (u64, Option<u64>)> + 'a {
        context
            .report
            .rows
            .iter()
            .map(|row| (row.atom_id, row.shift().map(f64::to_bits)))
    }

    fn matches(&self, context: nmr::Context<'_>, camera: Camera, bounds: Rectangle) -> bool {
        self.mode == context.mode
            && self.camera == Self::camera(camera)
            && self.bounds == Self::bounds(bounds)
            && self.rows.iter().copied().eq(Self::rows(context))
    }
}

impl SceneCache {
    fn document(&mut self, doc: &Document) {
        if self.document.as_ref() != Some(doc) {
            self.document = Some(doc.clone());
            self.scene = None;
            self.stretch = None;
            self.stretch_key = None;
            self.opacity = None;
            self.ids.clear();
            self.selection = None;
            self.markers = None;
            self.whole = false;
            self.copy = None;
            self.guides = None;
            self.nmr.entry = None;
            self.foreground_orbital = None;
            self.ring_interiors = None;
        }
    }

    pub fn ring_at(&mut self, doc: &Document, point: reshiki::document::Point) -> Option<Vec<u64>> {
        self.document(doc);
        self.ring_interiors
            .get_or_insert_with(|| Rc::new(reshiki::editing::RingInteriors::new(doc)))
            .at(point)
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

    pub fn stretch(
        &mut self,
        doc: &Document,
        fixed: u64,
        moving: u64,
    ) -> Option<Rc<reshiki::editing::reference::Stretch>> {
        self.document(doc);
        let key = (fixed, moving);
        if self.stretch_key != Some(key) {
            self.stretch = reshiki::editing::reference::Stretch::new(doc, fixed, moving)
                .ok()
                .map(Rc::new);
            self.stretch_key = Some(key);
        }
        self.stretch.clone()
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

    /// Rear-opacity paint shared by rendering and picking for the same document,
    /// without the atoms shown as 3D models.
    pub fn opacity(&mut self, doc: &Document) -> Rc<Paint> {
        self.document(doc);
        self.opacity
            .get_or_insert_with(|| Rc::new(Paint::new(&reshiki::models3d::drawn(doc))))
            .clone()
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
        let opacity = &mut self.opacity;
        let scene = self
            .scene
            .get_or_insert_with(|| {
                let flat = reshiki::models3d::drawn(doc);
                let paint = opacity.get_or_insert_with(|| Rc::new(Paint::new(&flat)));
                scene::primitives_with_opacity(doc, &flat, paint).into()
            })
            .clone();
        (markers, scene)
    }

    pub fn nmr_assignments(
        &mut self,
        doc: &Document,
        context: nmr::Context<'_>,
        camera: Camera,
        bounds: Rectangle,
    ) -> Rc<[nmr::Assignment]> {
        self.document(doc);
        if let Some((key, assignments)) = &self.nmr.entry
            && key.matches(context, camera, bounds)
        {
            return assignments.clone();
        }
        let key = NmrKey {
            rows: NmrKey::rows(context).collect(),
            mode: context.mode,
            camera: NmrKey::camera(camera),
            bounds: NmrKey::bounds(bounds),
        };
        let assignments: Rc<[nmr::Assignment]> =
            nmr::memoized_placements(doc, context, camera, bounds, &mut self.nmr.widths).into();
        self.nmr.entry = Some((key, assignments.clone()));
        assignments
    }

    pub fn foreground_orbital(&mut self, doc: &Document) -> bool {
        self.document(doc);
        *self
            .foreground_orbital
            .get_or_insert_with(|| super::hit::foreground_orbital(doc))
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
