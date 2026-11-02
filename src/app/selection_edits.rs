//! Selection, grouping, framing and arrangement commands.
use super::{App, Message};
use crate::canvas::Tool;
use reshiki::{
    document::Point,
    editing,
    graphics::{BracketSides, Graphic, GraphicStyle},
};

impl App {
    /// Replaces the selection and refreshes its cached inspector controls.
    pub(super) fn set_selection(&mut self, ids: Vec<u64>) {
        self.tab.selected = ids;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
    }

    pub(super) fn add_frame(&mut self, kind: reshiki::graphics::GraphicKind) {
        let ids = self.tab.doc.complete_selection(&self.tab.selected);
        self.frame(kind, ids, None);
    }

    pub(super) fn bracket_ion(&mut self) {
        match reshiki::ionic::bracket_charge(&self.tab.doc, &self.tab.selected) {
            Ok((atoms, charge)) => self.frame(
                reshiki::graphics::GraphicKind::Brackets,
                atoms,
                Some(charge),
            ),
            Err(reason) => {
                self.status = reason;
                self.error = true;
            }
        }
    }

    /// Frames `fit` and groups the frame with its complete selection. An ion
    /// charge hides the local charge labels of the fitted atoms.
    fn frame(
        &mut self,
        kind: reshiki::graphics::GraphicKind,
        fit: Vec<u64>,
        charge: Option<String>,
    ) {
        if let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.tab.doc, &fit) {
            let before = self.tab.doc.clone();
            let id = self.tab.doc.next_id();
            let padding = reshiki::style::DEFAULT.world(6.0);
            let mut frame = Graphic::dragged(
                id,
                kind,
                lo.offset(-padding, -padding),
                hi.offset(padding, padding),
                GraphicStyle {
                    width_pt: self.tab.doc.drawing_style.line_width_pt,
                    ..Default::default()
                },
                BracketSides::Both,
                false,
            );
            let ion = charge.is_some();
            if ion {
                for atom in &mut self.tab.doc.atoms {
                    if fit.contains(&atom.id) {
                        atom.display.hide_charge = true;
                    }
                }
            }
            frame.superscript = charge;
            self.tab.doc.graphics.push(frame);
            let mut ids = self.tab.doc.complete_selection(&fit);
            ids.push(id);
            if let Ok(ids) = self.tab.doc.group_selection(&ids) {
                self.set_selection(ids);
            }
            self.changed(before);
            self.tool = Tool::Select;
            self.status = if ion {
                "Ion bracket added and grouped with the component"
            } else {
                "Frame added and grouped with the selection"
            }
            .into();
        }
    }

    pub(super) fn normalize_depiction(&mut self) {
        let ids = if self.tab.selected.is_empty() {
            self.tab.doc.atoms.iter().map(|atom| atom.id).collect()
        } else {
            self.tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected)
        };
        self.draw_charge_separated(&ids);
    }

    pub(super) fn draw_charge_separated(&mut self, ids: &[u64]) {
        let before = self.tab.doc.clone();
        match reshiki::depiction::draw_as_charge_separated(&mut self.tab.doc, ids) {
            Ok(count) if count > 0 => {
                self.changed(before);
                self.status = format!(
                    "Drew {count} nitrogen center{} as charge-separated",
                    if count == 1 { "" } else { "s" }
                );
            }
            Ok(_) => {
                self.status = "No qualifying nitrogen centers to draw as charge-separated".into();
            }
            Err(reason) => {
                self.status = reason;
                self.error = true;
            }
        }
    }
    pub(super) fn group_selected(&mut self) {
        let before = self.tab.doc.clone();
        match self.tab.doc.group_selection(&self.tab.selected) {
            Ok(ids) => {
                self.tab.selected = ids;
                self.changed(before);
                self.tool = Tool::Select;
                self.status = format!(
                    "Grouped · {}-click selects a member · {} ungroups",
                    super::shortcuts::keys(iced::keyboard::Modifiers::ALT, ""),
                    super::shortcuts::label(&Message::Ungroup).unwrap_or_default()
                );
            }
            Err(e) => {
                self.status = e;
                self.error = true;
            }
        }
    }
    pub(super) fn ungroup_selected(&mut self) {
        let before = self.tab.doc.clone();
        if self.tab.doc.ungroup_selection(&self.tab.selected) {
            self.changed(before);
            self.status = "Ungrouped one level".into();
        }
    }
    pub(super) fn set_integral_groups(&mut self, integral: bool) {
        let before = self.tab.doc.clone();
        let ids = self.tab.doc.outer_selected_groups(&self.tab.selected);
        for g in &mut self.tab.doc.groups {
            if ids.contains(&g.id) {
                g.integral = integral;
            }
        }
        self.changed(before);
    }
    pub(super) fn invert_selection(&mut self) {
        let selected = self.tab.doc.expand_groups(&self.tab.selected);
        self.tab.selected = self
            .tab
            .doc
            .all_ids()
            .into_iter()
            .filter(|id| !selected.contains(id))
            .collect();
        self.tool = Tool::Select;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
    }
    pub(super) fn duplicate_selection(&mut self) {
        let part = editing::selection(&self.tab.doc, &self.tab.selected);
        let before = self.tab.doc.clone();
        self.tab.selected = editing::append(&mut self.tab.doc, &part, Point::new(28.0, 28.0));
        self.changed(before);
        self.tool = Tool::Select;
    }
    pub(super) fn transform_selection(&mut self, transform: reshiki::editing::Transform) {
        // A molecule shown only in 3D keeps its hidden drawing: Rotate rolls
        // its model and tilt turns it, while a flip has no 3D equivalent (a
        // mirrored model would be the other enantiomer), so it skips them.
        let selected = &self.tab.selected;
        let ids = match transform {
            editing::Transform::TiltX(_) | editing::Transform::TiltY(_) => selected.clone(),
            editing::Transform::Rotate(_) => {
                reshiki::models3d::without_hidden(&self.tab.doc, selected)
            }
            editing::Transform::FlipHorizontal | editing::Transform::FlipVertical => {
                if let Some(reason) = reshiki::models3d::refusal(&self.tab.doc, selected, "flip") {
                    self.status = reason;
                    self.error = true;
                    return;
                }
                reshiki::models3d::without_hidden(&self.tab.doc, selected)
            }
        };
        let before = self.tab.doc.clone();
        if let editing::Transform::Rotate(degrees) = transform {
            reshiki::models3d::roll(&mut self.tab.doc, &self.tab.selected, degrees);
        }
        editing::transform(&mut self.tab.doc, &ids, transform);
        let revision = self.tab.revision;
        self.changed(before);
        if self.tab.revision != revision
            && let editing::Transform::Rotate(degrees) = transform
        {
            self.status = format!(
                "Rotated {}° {}",
                degrees.abs(),
                if degrees > 0. {
                    "clockwise"
                } else {
                    "counterclockwise"
                }
            );
        }
    }
    pub(super) fn arrange_selection(&mut self, arrange: reshiki::editing::Arrange) {
        let before = self.tab.doc.clone();
        editing::arrange(&mut self.tab.doc, &self.tab.selected, arrange);
        self.changed(before);
    }
    pub(super) fn delete_selection(&mut self) {
        let before = self.tab.doc.clone();
        let ids = reshiki::arrow_labels::with_labels(&self.tab.doc, &self.tab.selected);
        self.tab.doc.delete(&ids);
        self.changed(before);
    }
    pub(super) fn select_all_objects(&mut self) {
        let closed_inspector = (!self.inspector_open).then_some(self.inspector_tab);
        self.tool = Tool::Select;
        self.set_selection(self.tab.doc.all_ids());
        if let Some(tab) = closed_inspector {
            self.inspector_open = false;
            self.inspector_tab = tab;
        }
    }
}
