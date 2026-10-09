//! Canvas edits: gesture dispatch, direct manipulation, placement and clicks.
use crate::app::{App, context_menu};
use crate::canvas::{Edit, Tool};
use reshiki::document::{Arrow, Point};
use reshiki::editing;
mod click;
mod placement;

impl App {
    pub(super) fn edit(&mut self, edit: Edit) {
        if let Edit::ContextMenu { position, selected } = edit {
            if self.tab.cleanup.is_none() {
                self.tab.selected = selected;
                self.tool = Tool::Select;
                self.sync_typography();
                self.context_menu = Some(context_menu::State::new(position, Default::default()));
            }
            return;
        }
        match edit {
            Edit::EraseStart(p) => {
                self.tab.erase_stroke = self.tool == Tool::Erase && self.tab.cleanup.is_none();
                self.tab.erase_committed = false;
                if self.tab.erase_stroke {
                    self.erase_segment(p, p);
                }
                return;
            }
            Edit::EraseTo(from, to) => {
                if self.tab.erase_stroke && self.tool == Tool::Erase {
                    self.erase_segment(from, to);
                }
                return;
            }
            Edit::EraseEnd => {
                self.tab.erase_stroke = false;
                self.tab.hover = None;
                return;
            }
            Edit::Hover(_) | Edit::ContextMenu { .. } => {}
            _ => self.tab.erase_stroke = false,
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.tab.pages.fit = None;
        }
        if let Edit::Hover(point) = edit {
            self.tab.hover = point.map(|p| (p, self.tab.file_epoch));
            self.keyboard_pointer_hover(point);
            return;
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.tab.hover = None;
        }
        if self.tab.cleanup.is_some() {
            match edit {
                Edit::Pan(dx, dy) => {
                    self.tab.camera.center = self
                        .tab
                        .camera
                        .center
                        .offset(-dx / self.tab.camera.zoom, -dy / self.tab.camera.zoom);
                    self.tab.fit_to_view = false;
                }
                Edit::Zoom(factor, _) => {
                    self.tab.camera.zoom = (self.tab.camera.zoom * factor).clamp(0.005, 5.);
                    self.tab.fit_to_view = false;
                }
                _ => {}
            }
            return;
        }
        let before = self.tab.doc.clone();
        match edit {
            Edit::ContextMenu { .. } => return,
            Edit::Hover(_)
            | Edit::RelaxDragStart { .. }
            | Edit::RelaxDragTarget { .. }
            | Edit::RelaxDragEnd { .. }
            | Edit::RelaxDragCancel { .. }
            | Edit::RelaxRotate { .. }
            | Edit::BeginText(_)
            | Edit::BeginTransform(_)
            | Edit::EraseStart(_)
            | Edit::EraseTo(..)
            | Edit::EraseEnd => return,
            Edit::ArrowClick(id) => self.apply_arrow_tool(id),
            Edit::Chain {
                points,
                source,
                target,
            } => return self.place_chain(&points, source, target, before),
            Edit::Graphic(start, end, constrain) => {
                return self.place_graphic(start, end, constrain, self.tab.snap_orbitals, before);
            }
            Edit::Orbital(start, end, constrain, snap) => {
                return self.place_graphic(start, end, constrain, snap, before);
            }
            Edit::AtomIndicator(owner, p) => {
                if let Some(anchor) = owner.anchor(&self.tab.doc) {
                    owner.set_offset(
                        &mut self.tab.doc,
                        Some(Point::new(p.x - anchor.x, p.y - anchor.y)),
                    );
                }
            }
            Edit::AtomMark(id, index, p) => {
                if let Some(a) = self.tab.doc.atom_mut(id)
                    && let Some(mark) = a.marks.get_mut(index)
                {
                    mark.offset = Point::new(p.x - a.position.x, p.y - a.position.y);
                }
            }
            Edit::ArrowHandle(id, index, p) => {
                if let Some(a) = self.tab.doc.arrows.iter_mut().find(|a| a.id == id) {
                    a.edit_handle(index, p);
                }
            }
            Edit::GraphicPoint(id, index, p) => {
                if let Some(g) = self.tab.doc.graphics.iter_mut().find(|g| g.id == id) {
                    g.edit_point(index, p);
                }
                self.sync_arc();
            }
            Edit::Template(anchor, direction) => {
                return self.place_template(anchor, direction, before);
            }
            Edit::Transform {
                ids,
                pivot,
                scale,
                rotation,
            } => {
                editing::transform_about(&mut self.tab.doc, &ids, pivot, scale, rotation);
                self.tab.selected = ids;
            }
            Edit::Tilt { ids, x, y } => {
                crate::canvas::tilt::apply(&mut self.tab.doc, &ids, x, y);
                self.tab.selected = ids;
            }
            Edit::ScaleAxes { ids, pivot, x, y } => {
                editing::scale_axes_about(&mut self.tab.doc, &ids, pivot, x, y);
                self.tab.selected = ids;
            }
            Edit::RingPreset(preset, anchor, direction, connect, alternate) => {
                return self
                    .place_ring_preset(preset, anchor, direction, connect, alternate, before);
            }
            Edit::Ring(anchor, direction) => {
                return self.place_ring(
                    anchor,
                    direction,
                    self.ring_size,
                    self.aromatic_ring,
                    before,
                );
            }
            Edit::DelocalizedRing(anchor, direction, size) => {
                return self.place_ring(anchor, direction, size, true, before);
            }
            Edit::Select(ids) | Edit::SelectAt(ids, _) => {
                let inspector_width = self.inspector_width();
                self.tab.selected = ids;
                self.sync_typography();
                self.sync_graphics();
                self.sync_arrows();
                self.sync_bonds();
                // Auto-revealing object properties must not move the clicked
                // target. The inspector occupies the right edge, so compensate
                // for the centered camera's horizontal shift. Record the new
                // size now so its sensor event does not also trigger Fit.
                let width_change = inspector_width - self.inspector_width();
                self.tab.camera.center.x += width_change / (2. * self.tab.camera.zoom);
                self.viewport.width += width_change;
            }
            Edit::Move(ids, dx, dy) => {
                if let Some(snapped) = editing::snap_ring(
                    &mut self.tab.doc,
                    &ids,
                    Point::new(dx, dy),
                    14.0 / self.tab.camera.zoom,
                ) {
                    self.tab.selected = snapped;
                } else {
                    self.tab.doc.translate(&ids, dx, dy);
                    self.tab.selected = ids;
                }
            }
            Edit::Duplicate(ids, dx, dy) => {
                let part = editing::selection(&self.tab.doc, &ids);
                let copy = editing::append(&mut self.tab.doc, &part, Point::new(dx, dy));
                if !copy.is_empty() {
                    self.tab.selected = copy;
                }
            }
            Edit::Pan(dx, dy) => {
                self.tab.fit_to_view = false;
                self.tab.camera.center = self.tab.camera.center.offset(-dx, -dy);
            }
            Edit::Zoom(f, at) => {
                self.tab.fit_to_view = false;
                let old = self.tab.camera.zoom;
                self.tab.camera.zoom = (old * f).clamp(0.005, 5.0);
                let ratio = old / self.tab.camera.zoom;
                self.tab.camera.center = Point::new(
                    at.x + (self.tab.camera.center.x - at.x) * ratio,
                    at.y + (self.tab.camera.center.y - at.y) * ratio,
                );
            }
            Edit::PlaneBond(start, end) => return self.place_plane_bond(start, end, before),
            Edit::Bond(start, end, a, b) => return self.place_bond(start, end, a, b, before),
            Edit::Click(p) => return self.canvas_click(p, before),
        }
        self.changed(before);
    }
    fn place_arrow(&mut self, start: Point, end: Point) {
        let id = self.tab.doc.next_id();
        self.tab.doc.arrows.push(Arrow::new(
            id,
            start,
            end,
            self.tab.arrow_style,
            self.tab.arrows.style.clone(),
        ));
        self.tab.selected = vec![id];
    }
    fn erase_segment(&mut self, from: Point, to: Point) {
        let before = self.tab.doc.clone();
        reshiki::erasing::stroke(&mut self.tab.doc, from, to, 7. / self.tab.camera.zoom);
        if self.tab.doc != before {
            self.tab.selected.clear();
            let revision = self.tab.revision;
            self.changed_continuing(before, self.tab.erase_committed);
            self.tab.erase_committed |= self.tab.revision != revision;
        }
    }
}
