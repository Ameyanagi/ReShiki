//! Two target clicks are one document edit; the source is tab-local UI state.
use super::*;
use crate::canvas::pick::{ARROW_TARGET_PICK_PX, PICK_RADIUS_PX};
use reshiki::arrow_anchors;

// The status bar shows the first line; its tooltip shows the whole text.
const NO_TARGET: &str = concat!(
    "No target here · Click an atom, bond or lone pair\n",
    "Choose an atom, a visible bond or a positioned lone pair. Option/Alt-click ",
    "or turn off Attach ends to atoms and bonds for a free arrow."
);
const SOURCE_SET: &str = concat!(
    "Arrow start set · Click the destination\n",
    "Click a destination atom, bond or lone pair · Escape cancels"
);
const ATTACHED: &str = concat!(
    "Arrow attached · Drag a square to aim an end\n",
    "Drag the squares to adjust each end direction · Drag an endpoint to detach it"
);

impl App {
    pub(in crate::app) fn arrow_target_click(&mut self, p: reshiki::document::Point, bypass: bool) {
        if bypass || !self.arrow_attach_mode() {
            self.tab.arrows.cancel_source();
            let before = self.tab.doc.clone();
            let hit = crate::canvas::hit_object(
                &self.tab.doc,
                p,
                self.tab.camera.world_len(PICK_RADIUS_PX),
            );
            self.place_or_restyle_arrow(p, hit);
            self.changed(before);
            return;
        }
        let Some(target) = arrow_anchors::pick(
            &self.tab.doc,
            p,
            self.tab.camera.world_len(ARROW_TARGET_PICK_PX),
        ) else {
            self.error = true;
            self.status = NO_TARGET.into();
            return;
        };
        let Some(source) = self.tab.arrows.source().cloned() else {
            self.tab.arrows.begin_source(target);
            self.tab.selected.clear();
            self.error = false;
            self.status = SOURCE_SET.into();
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
                self.tab.arrows.cancel_source();
                self.tab.selected = vec![id];
                self.changed(before);
                self.error = false;
                self.status = ATTACHED.into();
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
