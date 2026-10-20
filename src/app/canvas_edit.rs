//! Canvas edits: gesture dispatch, direct manipulation, placement and clicks.
use crate::app::{App, context_menu};
use crate::canvas::{Edit, Tool};
use reshiki::document::{Arrow, Point};
use reshiki::editing;
mod click;
mod placement;

const BOND_TEXT_HINT: &str = "Drag to bond; this text becomes an atom label";

impl App {
    pub(super) fn edit(&mut self, edit: Edit) {
        match edit {
            Edit::ArrowTarget(p, bypass) => {
                self.arrow_target_click(p, bypass);
                return;
            }
            Edit::CancelArrowSource => {
                self.error = false;
                self.tab.arrows.cancel_source();
                self.status = "Arrow source canceled".into();
                return;
            }
            Edit::ContextMenu {
                position,
                selected,
                hit,
            } => {
                self.tab.arrows.cancel_source();
                if self.tab.cleanup.is_none() {
                    self.tab.selected = selected;
                    self.tool = Tool::Select;
                    self.sync_typography();
                    let mut menu = context_menu::State::new(position, Default::default());
                    menu.hit = Some(hit);
                    self.context_menu = Some(menu);
                }
                return;
            }
            _ => {}
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
            self.hover_bond_text(point);
            return;
        }
        if matches!(edit, Edit::Pan(..) | Edit::Zoom(..)) {
            self.tab.hover = None;
            self.hover_bond_text(None);
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
        if matches!(edit, Edit::Bond(..) | Edit::TextBond { .. }) {
            self.tab.arrows.cancel_source();
        }
        match edit {
            // These edits are handled by the leading match, before preview gates.
            Edit::ContextMenu { .. } | Edit::ArrowTarget(..) | Edit::CancelArrowSource => return,
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
                source_text,
                target,
            } => return self.place_chain_from_text(&points, source, source_text, target, before),
            edit @ Edit::TextBond { .. } => return self.place_text_bond(edit, before),
            Edit::TextBondRefused(error) | Edit::Refused(error) => {
                self.status = error;
                self.error = true;
                return;
            }
            Edit::Graphic(start, end, constrain) => {
                return self.place_graphic(start, end, constrain, self.tab.snap_orbitals, before);
            }
            Edit::Orbital(start, end, constrain, snap) => {
                return self.place_graphic(start, end, constrain, snap, before);
            }
            Edit::PenSegment(stroke) => {
                return self.place_pen_segment(stroke, before);
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
                reshiki::arrow_anchors::edit_handle(&mut self.tab.doc, id, index, p);
            }
            Edit::GraphicPoint(id, index, p) => {
                self.tab.path_point = Some((id, index));
                if let Some(g) = self.tab.doc.graphics.iter_mut().find(|g| g.id == id)
                    && let Err(error) = g.edit_point(index, p)
                {
                    self.status = error;
                    self.error = true;
                    return;
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
                self.set_selection(ids);
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
            Edit::StretchBond {
                fixed,
                moving,
                length,
            } => {
                if !self.reference_stretch_active(fixed, moving) {
                    self.status = "Choose a bond in this drawing before stretching".into();
                    self.error = true;
                    return;
                }
                match editing::reference::Stretch::commit(&self.tab.doc, fixed, moving, length) {
                    Ok(candidate) => self.tab.doc = candidate,
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return;
                    }
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
    pub(super) fn place_arrow(&mut self, start: Point, end: Point) {
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

impl App {
    /// The caption hint belongs to the pointer over this tab with this tool:
    /// put the status it replaced back before either changes.
    pub(super) fn clear_bond_text_hint(&mut self) {
        self.hover_bond_text(None);
    }

    fn hover_bond_text(&mut self, point: Option<Point>) {
        let hint = point
            .filter(|_| self.tool.bonds_from_text())
            .and_then(|p| {
                crate::canvas::bond_text_at(
                    &self.tab.doc,
                    p,
                    self.tab
                        .camera
                        .world_len(crate::canvas::pick::PICK_RADIUS_PX),
                )
            })
            .map(|id| {
                reshiki::atom_text::text_atom_refusal(&self.tab.doc, id)
                    .unwrap_or_else(|| BOND_TEXT_HINT.into())
            });
        let previous = self
            .bond_text_hint
            .take()
            .and_then(|(status, error, shown)| {
                (self.status == shown && !self.error).then_some((status, error))
            });
        if let Some(hint) = hint {
            let (status, error) = previous.unwrap_or_else(|| (self.status.clone(), self.error));
            self.status = hint.clone();
            self.error = false;
            self.bond_text_hint = Some((status, error, hint));
        } else if let Some((status, error)) = previous {
            self.status = status;
            self.error = error;
        }
    }

    #[cfg(test)]
    pub(crate) fn bond_text_evidence_after(
        doc: reshiki::document::Document,
        edit: Edit,
    ) -> reshiki::document::Document {
        let (mut app, _) = Self::new();
        app.tab.busy = false;
        app.tab.doc = doc;
        app.tool = Tool::Bond(1);
        let _ = app.update(crate::app::Message::Canvas(edit));
        app.tab.doc
    }
}

#[cfg(test)]
mod tests;
