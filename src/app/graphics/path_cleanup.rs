//! Selection-wide path commands share one document transaction with curved arrows.
use super::*;

impl App {
    pub(in crate::app) fn has_cleanup_selection(&self) -> bool {
        self.tab
            .doc
            .graphics
            .iter()
            .any(|g| self.tab.selected.contains(&g.id) && g.path_summary().is_some())
            || self
                .tab
                .doc
                .arrows
                .iter()
                .any(|a| self.tab.selected.contains(&a.id) && a.bezier_controls().is_some())
    }

    pub(super) fn cleanup_paths(&mut self, command: Cleanup) {
        if !self.has_cleanup_selection() {
            self.status = "Select a pen path, Bézier curve or curved arrow".into();
            self.error = true;
            return;
        }
        let before = self.tab.doc.clone();
        let mut changed = 0;
        let mut refused = vec![];
        let mut record = |result| match result {
            Ok(true) => changed += 1,
            Ok(false) => {}
            Err(reason) => {
                if !refused.contains(&reason) {
                    refused.push(reason);
                }
            }
        };
        for graphic in &mut self.tab.doc.graphics {
            if self.tab.selected.contains(&graphic.id) && graphic.path_summary().is_some() {
                record(match command {
                    Cleanup::Smooth => graphic.smooth_path(),
                    Cleanup::Symmetrise { upright } => {
                        graphic.symmetrise_path(upright || self.shift_held)
                    }
                });
            }
        }
        for arrow in &mut self.tab.doc.arrows {
            if self.tab.selected.contains(&arrow.id) && arrow.bezier_controls().is_some() {
                record(match command {
                    Cleanup::Smooth => arrow.smooth_bow(),
                    Cleanup::Symmetrise { .. } => arrow.symmetrise(),
                });
            }
        }
        if changed > 0 {
            self.tab.path_point = None;
            self.changed(before);
            self.sync_graphics();
            self.sync_arrows();
        }
        let verb = match command {
            Cleanup::Smooth => "Smoothed",
            Cleanup::Symmetrise { .. } => "Symmetrised",
        };
        let status = if changed > 0 {
            format!(
                "{verb} {changed} object{}",
                if changed == 1 { "" } else { "s" }
            )
        } else if refused.is_empty() {
            match command {
                Cleanup::Smooth => "No smoothing needed".into(),
                Cleanup::Symmetrise { .. } => "Already symmetric".into(),
            }
        } else {
            String::new()
        };
        self.error = !refused.is_empty();
        self.status = std::iter::once(status)
            .chain(refused)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
    }
}
