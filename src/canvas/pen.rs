//! One shared plan for pen previews and completed segments.
use super::{MoleculeCanvas, World};
use reshiki::{
    document::Document,
    graphics::{Graphic, GraphicKind, GraphicStyle},
};

#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub start: World,
    pub end: World,
    pub dragged: bool,
    pub close: bool,
}

pub(crate) fn active<'a>(doc: &'a Document, selected: &[u64]) -> Option<&'a Graphic> {
    let [id] = selected else { return None };
    doc.graphics.iter().find(|g| {
        g.id == *id
            && matches!(g.kind, GraphicKind::Path | GraphicKind::Curve)
            && !g.path_closed()
            && g.path_handles().is_some()
    })
}

pub(crate) fn apply(
    doc: &Document,
    selected: &[u64],
    style: &GraphicStyle,
    stroke: Stroke,
) -> Result<(Document, u64, usize), String> {
    let mut result = doc.clone();
    if let Some(graphic) = active(doc, selected) {
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
            .path_handles()
            .ok_or("Invalid pen path")?
            .into_iter()
            .filter(|h| h.node)
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
        let dragged = start.distance(end) >= 3. / self.camera.zoom;
        let path = active(self.doc, self.selected);
        if path.is_none() && !dragged {
            return None;
        }
        let close = path.and_then(Graphic::path_handles).is_some_and(|handles| {
            let nodes: Vec<_> = handles.into_iter().filter(|h| h.node).collect();
            nodes.len() >= 3
                && nodes
                    .first()
                    .is_some_and(|node| start.distance(node.point) < 8. / self.camera.zoom)
        });
        Some(Stroke {
            start,
            end,
            dragged,
            close,
        })
    }
}
