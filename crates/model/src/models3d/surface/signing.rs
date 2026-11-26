use super::{Snapshot, draw::view_signature};
use crate::document::Document;
use crate::models3d::panel_model;

#[derive(Clone, Copy)]
enum SnapshotSlot {
    Surface(usize),
    Panel(usize, usize),
    MoPicture(usize, usize),
}

impl SnapshotSlot {
    fn snapshot_and_view(self, doc: &Document) -> Option<(&Snapshot, String)> {
        match self {
            Self::Surface(index) => {
                let model = doc.models.get(index)?;
                let snapshot = model.surface.as_ref()?.snapshot.as_ref()?;
                Some((snapshot, view_signature(doc, model)))
            }
            Self::Panel(index, panel) => {
                let model = doc.models.get(index)?;
                let figure = &model.panels.get(panel)?.figure;
                let snapshot = figure.snapshot.as_ref()?;
                Some((snapshot, view_signature(doc, &panel_model(model, figure))))
            }
            Self::MoPicture(index, picture) => {
                let mo = doc.graphics.get(index)?.mo.as_deref()?;
                let snapshot = mo.pictures.get(picture)?.snapshot.as_ref()?;
                Some((
                    snapshot,
                    view_signature(doc, &mo.picture_model(doc, picture)?),
                ))
            }
        }
    }

    fn snapshot_mut(self, doc: &mut Document) -> Option<&mut Snapshot> {
        let figure = match self {
            Self::Surface(index) => doc.models.get_mut(index)?.surface.as_mut()?,
            Self::Panel(index, panel) => {
                &mut doc.models.get_mut(index)?.panels.get_mut(panel)?.figure
            }
            Self::MoPicture(index, picture) => doc
                .graphics
                .get_mut(index)?
                .mo
                .as_deref_mut()?
                .pictures
                .get_mut(picture)?,
        };
        figure.snapshot.as_mut()
    }
}

/// Keep current saved orbital pictures signed when their IDs change.
/// Renumbering preserves the positions of surviving slots.
pub(crate) fn keep_signed<R>(doc: &mut Document, renumber: impl FnOnce(&mut Document) -> R) -> R {
    let signed: Vec<_> = doc
        .models
        .iter()
        .enumerate()
        .flat_map(|(index, model)| {
            std::iter::once(SnapshotSlot::Surface(index))
                .chain((0..model.panels.len()).map(move |panel| SnapshotSlot::Panel(index, panel)))
        })
        .chain(
            doc.graphics
                .iter()
                .enumerate()
                .flat_map(|(index, graphic)| {
                    let count = graphic.mo.as_deref().map_or(0, |mo| mo.pictures.len());
                    (0..count).map(move |picture| SnapshotSlot::MoPicture(index, picture))
                }),
        )
        .filter(|slot| {
            slot.snapshot_and_view(doc)
                .is_some_and(|(snapshot, view)| snapshot.view == view)
        })
        .collect();
    let result = renumber(doc);
    for slot in signed {
        if let Some((_, view)) = slot.snapshot_and_view(doc)
            && let Some(snapshot) = slot.snapshot_mut(doc)
        {
            snapshot.view = view;
        }
    }
    result
}
