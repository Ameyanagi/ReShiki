//! One shared plan for pen previews and completed segments.
use super::{MoleculeCanvas, World};
use crate::canvas::pick::{DRAG_SLOP_PX, HANDLE_PICK_PX};
use reshiki::{
    document::Document,
    graphics::{Graphic, GraphicStyle, PathSummary},
};

#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub start: World,
    pub end: World,
    pub dragged: bool,
    pub close: bool,
}

/// A path handle follows pointer displacement without jumping to the grab point.
#[derive(Debug)]
pub(super) struct PointDrag {
    pub id: u64,
    pub index: usize,
    pub pressed: World,
    pub original: World,
}

impl PointDrag {
    pub(super) fn target(&self, pointer: World) -> World {
        if pointer == self.pressed {
            self.original
        } else {
            self.original
                .offset(pointer.x - self.pressed.x, pointer.y - self.pressed.y)
        }
    }
}

pub(crate) fn active<'a>(
    doc: &'a Document,
    selected: &[u64],
) -> Option<(&'a Graphic, PathSummary)> {
    let [id] = selected else { return None };
    let graphic = doc.graphics.iter().find(|graphic| graphic.id == *id)?;
    let summary = graphic.path_summary()?;
    (!summary.closed).then_some((graphic, summary))
}

pub(crate) fn apply(
    doc: &Document,
    selected: &[u64],
    style: &GraphicStyle,
    stroke: Stroke,
) -> Result<(Document, u64, usize), String> {
    let mut result = doc.clone();
    if let Some((graphic, _)) = active(doc, selected) {
        let id = graphic.id;
        let graphic = result
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or("Missing pen path")?;
        let index = if stroke.close {
            graphic.set_path_closed(true)?;
            0
        } else {
            graphic.append_pen_node(stroke.start, stroke.dragged.then_some(stroke.end))?
        };
        Ok((result, id, index))
    } else if stroke.dragged {
        let id = result.next_id();
        let graphic = Graphic::pen_curve(id, stroke.start, stroke.end, style.clone());
        let index = graphic
            .path_summary()
            .ok_or("Invalid pen path")?
            .nodes()
            .next_back()
            .ok_or("Missing pen node")?
            .index;
        result.graphics.push(graphic);
        Ok((result, id, index))
    } else {
        Err("Drag to draw the first segment".into())
    }
}

impl MoleculeCanvas<'_> {
    pub(super) fn pen_stroke(&self, start: World, end: World) -> Option<Stroke> {
        let dragged = start.distance(end) >= self.camera.world_len(DRAG_SLOP_PX);
        let path = active(self.doc, self.selected);
        if path.is_none() && !dragged {
            return None;
        }
        let close = path.is_some_and(|(_, summary)| {
            summary.can_close()
                && summary.nodes().next().is_some_and(|node| {
                    start.distance(node.point) < self.camera.world_len(HANDLE_PICK_PX)
                })
        });
        Some(Stroke {
            start,
            end,
            dragged,
            close,
        })
    }
}
