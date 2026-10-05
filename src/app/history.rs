//! Undo and redo of drawing history.
use super::{App, chemistry_changed, pages};

impl App {
    pub(super) fn step_history(&mut self, redo: bool) {
        self.tab.erase_stroke = false;
        self.tab.cleanup = None;
        let before = self.tab.doc.clone();
        let selected_group = !before.outer_selected_groups(&self.tab.selected).is_empty();
        let changed = if !redo {
            self.tab.history.undo(&mut self.tab.doc)
        } else {
            self.tab.history.redo(&mut self.tab.doc)
        };
        if changed {
            self.tab
                .keyboard_drawing
                .restore(redo, &self.tab.doc, self.tab.file_epoch);
            self.tab.recent_molecules.restore(redo, self.tab.file_epoch);
            self.tab.revision = self.tab.revision.wrapping_add(1);
            if chemistry_changed(&before, &self.tab.doc) {
                self.tab.analysis = None;
                reshiki::atom_labels::clear_computed(&mut self.tab.doc);
                self.tab.labels_dirty = true;
            }
            let ids = self.tab.doc.all_ids();
            self.tab.selected.retain(|id| ids.contains(id));
            let previous_ids = before.all_ids();
            let restored_group = self.tab.doc.groups.iter().any(|group| {
                group
                    .members
                    .iter()
                    .any(|id| self.tab.selected.contains(id))
                    && group
                        .members
                        .iter()
                        .all(|id| self.tab.selected.contains(id) || !previous_ids.contains(id))
            });
            if selected_group || restored_group {
                self.tab.selected = self.tab.doc.expand_groups(&self.tab.selected);
            }
            if self.tab.selected.is_empty()
                && let Some(id) = self.tab.caption_target.filter(|id| ids.contains(id))
            {
                self.tab.selected.push(id);
            }
            self.sync_typography();
            self.sync_graphics();
            self.sync_arrows();
            self.sync_bonds();
            if before.drawing_style != self.tab.doc.drawing_style {
                self.sync_drawing_defaults();
                self.tab.styles.editor = None;
            }
            if before.page_layout != self.tab.doc.page_layout {
                self.tab.pages.editor = self
                    .tab
                    .pages
                    .editor
                    .as_ref()
                    .map(|_| pages::Editor::new(&self.tab.doc, self.tab.file_epoch));
                if let Some(layout) = &self.tab.doc.page_layout {
                    self.tab.pages.active =
                        self.tab.pages.active.min(layout.count().saturating_sub(1));
                    self.fit_pages(Some(self.tab.pages.active));
                } else {
                    self.fit();
                }
            }
            self.status = "History restored".into();
            self.error = false;
        }
    }
}
