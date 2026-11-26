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
                let snapshot = mo.pictures.get(picture)?.figure.snapshot.as_ref()?;
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
            Self::MoPicture(index, picture) => {
                &mut doc
                    .graphics
                    .get_mut(index)?
                    .mo
                    .as_deref_mut()?
                    .pictures
                    .get_mut(picture)?
                    .figure
            }
        };
        figure.snapshot.as_mut()
    }
}

/// Keep current saved orbital pictures signed when their IDs change.
/// Renumbering preserves the positions of surviving slots.
pub(crate) fn keep_signed<R>(doc: &mut Document, renumber: impl FnOnce(&mut Document) -> R) -> R {
    let signed = signed_slots(
        doc,
        doc.models
            .iter()
            .enumerate()
            .flat_map(|(index, model)| {
                std::iter::once(SnapshotSlot::Surface(index)).chain(
                    (0..model.panels.len()).map(move |panel| SnapshotSlot::Panel(index, panel)),
                )
            })
            .chain(mo_slots(doc)),
    );
    let result = renumber(doc);
    restore_signatures(doc, signed);
    result
}

/// Keep a copied diagram's saved picture when pasted onto a different canvas.
pub(crate) fn keep_mo_signed(doc: &mut Document, change: impl FnOnce(&mut Document)) {
    let signed = signed_slots(doc, mo_slots(doc));
    change(doc);
    restore_signatures(doc, signed);
}

fn mo_slots(doc: &Document) -> impl Iterator<Item = SnapshotSlot> + '_ {
    doc.graphics
        .iter()
        .enumerate()
        .flat_map(|(index, graphic)| {
            let count = graphic.mo.as_deref().map_or(0, |mo| mo.pictures.len());
            (0..count).map(move |picture| SnapshotSlot::MoPicture(index, picture))
        })
}

fn signed_slots(doc: &Document, slots: impl Iterator<Item = SnapshotSlot>) -> Vec<SnapshotSlot> {
    slots
        .filter(|slot| {
            slot.snapshot_and_view(doc)
                .is_some_and(|(snapshot, view)| snapshot.view == view)
        })
        .collect()
}

fn restore_signatures(doc: &mut Document, signed: Vec<SnapshotSlot>) {
    for slot in signed {
        if let Some((_, view)) = slot.snapshot_and_view(doc)
            && let Some(snapshot) = slot.snapshot_mut(doc)
        {
            snapshot.view = view;
        }
    }
}
