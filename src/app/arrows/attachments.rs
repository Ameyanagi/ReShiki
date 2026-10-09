//! Two target clicks are one document edit; the source is tab-local UI state.
use super::*;
use reshiki::arrow_anchors;

impl App {
    pub(in crate::app) fn arrow_target_click(&mut self, p: reshiki::document::Point, bypass: bool) {
        if bypass
            || !self.tab.arrows.attach_targets
            || !matches!(self.tab.arrow_style, Preset::Curved | Preset::Fishhook)
        {
            self.tab.arrow_source = None;
            let before = self.tab.doc.clone();
            let hit = crate::canvas::hit_object(&self.tab.doc, p, 10. / self.tab.camera.zoom);
            if let Some(id) = hit.filter(|id| self.tab.doc.arrows.iter().any(|a| a.id == *id)) {
                self.apply_arrow_tool(id);
            } else {
                let length = self.tab.doc.drawing_style.bond_length_world * 2.;
                self.place_arrow(p, p.offset(length, 0.));
            }
            self.changed(before);
            return;
        }
        let Some(target) = arrow_anchors::pick(&self.tab.doc, p, 8. / self.tab.camera.zoom) else {
            self.error = true;
            self.status = "Choose an atom, a visible bond, or a positioned lone pair; Alt-click or turn off Attach targets for free placement".into();
            return;
        };
        let Some(source) = self.tab.arrow_source.clone() else {
            self.tab.arrow_source = Some(target);
            self.tab.selected.clear();
            self.error = false;
            self.status = "Arrow source selected · Click a destination atom, bond, or lone pair · Escape cancels".into();
            return;
        };
        let before = self.tab.doc.clone();
        match arrow_anchors::create(
            &mut self.tab.doc,
            &source,
            &target,
            self.tab.arrow_style,
            self.tab.arrows.style.clone(),
        ) {
            Ok(id) => {
                self.tab.arrow_source = None;
                self.tab.selected = vec![id];
                self.changed(before);
                self.error = false;
                self.status = "Attached arrow created · Drag the squares to adjust each end direction · Drag an endpoint to detach it".into();
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
}

#[cfg(test)]
mod tests;
