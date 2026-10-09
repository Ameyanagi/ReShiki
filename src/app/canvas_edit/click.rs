//! Clicks on the canvas, by tool.
use crate::app::App;
use crate::canvas::{self, Edit, Tool};
use reshiki::document::{Annotation, Document, Point};
use reshiki::editing;

impl App {
    pub(super) fn canvas_click(&mut self, p: Point, before: Document) {
        let hit = canvas::hit_object(&self.tab.doc, p, 10.0 / self.tab.camera.zoom);
        match self.tool {
            Tool::Atom => {
                if let Some(id) = self.tab.doc.nearest(p, 10.0 / self.tab.camera.zoom) {
                    self.tab.doc.invalidate_chemistry(&[id]);
                    if let Some(a) = self.tab.doc.atom_mut(id) {
                        a.element = self.element.clone();
                        a.display.variable = None;
                        a.explicit_h = 0;
                        a.no_implicit = false;
                        a.charge = 0;
                        a.isotope = 0;
                    }
                    self.tab.selected = vec![id];
                } else {
                    let id = self.tab.doc.add_atom(&self.element, p);
                    self.tab.selected = vec![id];
                }
            }
            tool if tool.bond_preset() == Some(reshiki::bonds::BondPreset::Dotted) => {
                self.status = "Drag from a bonded explicit H to an existing acceptor".into();
                self.error = true;
                return;
            }
            Tool::Bond(_) | Tool::StyledBond(_) | Tool::Wedge | Tool::Hash | Tool::Wavy => {
                return self.click_bond_tool(p, before);
            }
            Tool::Ring => {
                return self.edit(Edit::Ring(p, None));
            }
            Tool::Text => {
                if let Some(label) =
                    hit.filter(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id))
                {
                    self.tab.selected = vec![label];
                    self.sync_typography();
                    return;
                }
                if !self.tab.caption.trim().is_empty() {
                    let id = self.tab.doc.next_id();
                    self.tab.doc.annotations.push(Annotation {
                        id,
                        position: p,
                        text: self.tab.caption.clone(),
                        format: self.tab.caption_format.clone(),
                    });
                    self.tab.selected = vec![id];
                    self.tab.caption_target = Some(id);
                    self.tool = Tool::Select;
                }
            }
            Tool::Arrow => {
                if let Some(id) = hit.filter(|id| self.tab.doc.arrows.iter().any(|a| a.id == *id)) {
                    self.apply_arrow_tool(id);
                } else {
                    let length = self.tab.doc.drawing_style.bond_length_world * 2.;
                    self.place_arrow(p, p.offset(length, 0.));
                }
            }
            Tool::Erase => {
                reshiki::erasing::stroke(&mut self.tab.doc, p, p, 7. / self.tab.camera.zoom)
            }
            _ => self.tab.selected = hit.into_iter().collect(),
        }
        self.changed(before);
    }
    fn click_bond_tool(&mut self, p: Point, before: Document) {
        let atom = self.tab.doc.nearest(p, 10.0 / self.tab.camera.zoom);
        let bond = self
            .tab
            .doc
            .bonds
            .iter()
            .find(|b| {
                self.tab
                    .doc
                    .atom(b.a)
                    .zip(self.tab.doc.atom(b.b))
                    .is_some_and(|(a, z)| {
                        canvas::distance_to_segment(p, a.position, z.position)
                            < 7.0 / self.tab.camera.zoom
                    })
            })
            .cloned();
        if let Some(b) = bond.filter(|_| atom.is_none()) {
            let shift_double = self.tool.bond_preset().is_some_and(|preset| {
                use reshiki::bonds::BondPreset as P;
                matches!(
                    preset,
                    P::Double | P::BoldDouble | P::DashedDouble | P::DoubleDashed
                ) && P::of(&b) == Some(preset)
            });
            if shift_double {
                let position =
                    reshiki::scene::effective_double_position(&self.tab.doc, &b).cycled();
                if let Some(bond) = self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .find(|bond| bond.a == b.a && bond.b == b.b)
                {
                    bond.double_position = position;
                }
                self.tab.selected = vec![b.a, b.b];
                self.changed(before);
                self.status = format!("Double bond: {position} · Click again to shift its lines");
                return;
            }
            let reverse = self.tool.bond_preset().is_some_and(|p| {
                use reshiki::bonds::BondPreset as P;
                matches!(
                    p,
                    P::Wedge
                        | P::HashedWedge
                        | P::HollowWedge
                        | P::Hashed
                        | P::Bold
                        | P::Dative
                        | P::Dashed
                ) && P::of(&b) == Some(p)
            });
            let (order, display) = if self.tool == Tool::Bond(2) {
                (2, "plain")
            } else if matches!(self.tool, Tool::Bond(_)) {
                (
                    match b.order {
                        1 => 2,
                        2 => 3,
                        _ => 1,
                    },
                    "plain",
                )
            } else {
                self.bond_style()
            };
            if let Some(preset) = self
                .tool
                .bond_preset()
                .filter(|p| p.preserves_chemistry(&b))
            {
                if b.order == 5 {
                    if let Some(bond) = self
                        .tab
                        .doc
                        .bonds
                        .iter_mut()
                        .find(|bond| bond.a == b.a && bond.b == b.b)
                    {
                        preset.apply(bond);
                    }
                } else {
                    preset.place(&mut self.tab.doc, b.a, b.b);
                }
            } else {
                self.tab.doc.add_bond(b.a, b.b, order, display);
                self.apply_current_bond_preset(b.a, b.b);
            }
            if reverse
                && let Some(bond) = self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .find(|bond| bond.a == b.a && bond.b == b.b)
            {
                if bond.order == 5 {
                    bond.reverse_projection();
                } else {
                    bond.reverse();
                }
            }
            self.tab.selected = vec![b.a, b.b];
        } else {
            let (order, display) = self.bond_style();
            let a = atom.unwrap_or_else(|| self.tab.doc.add_atom("C", p));
            let Some(start) = self.tab.doc.atom(a).map(|a| a.position) else {
                self.status = "The bond's starting atom is unavailable".into();
                self.error = true;
                return;
            };
            if let Some(endpoint) = reshiki::projection::growth::Plane::at(&self.tab.doc, a)
                .and_then(|plane| plane.outward(self.tab.bond_drawing.length))
            {
                let preset = self
                    .tool
                    .bond_preset()
                    .unwrap_or(reshiki::bonds::BondPreset::Single);
                match reshiki::projection::growth::place(&self.tab.doc, a, endpoint, "C", preset) {
                    Ok((doc, id)) => {
                        self.tab.doc = doc;
                        self.tab.selected = vec![id];
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return;
                    }
                }
            } else {
                let end = editing::bond_extension(&self.tab.doc, start, Some(a), order);
                let ratio =
                    self.tab.bond_drawing.length / reshiki::style::DEFAULT.bond_length_world;
                let end = start.offset((end.x - start.x) * ratio, (end.y - start.y) * ratio);
                let b = self.tab.doc.add_atom("C", end);
                self.tab.doc.add_bond(a, b, order, display);
                self.apply_current_bond_preset(a, b);
                self.tab.selected = vec![b];
            }
        }
        self.changed(before);
    }
}
