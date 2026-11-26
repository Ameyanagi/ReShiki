//! Name the calculation or attached geometry restored by drawing history.
use crate::app::App;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Settings, Task};
use reshiki::science::Stored;
use std::sync::Arc;

impl App {
    pub(in crate::app) fn xtb_history_status(
        &self,
        mut before: Document,
        redo: bool,
    ) -> Option<String> {
        let after = &self.tab.doc;
        let previous = &before.science.results;
        let restored = &after.science.results;
        if previous.is_empty() && restored.is_empty()
            || !drawing_unchanged(&before, after)
            || before.models.len() != after.models.len()
        {
            return None;
        }
        let mut moved = None;
        for (old, now) in before.models.iter_mut().zip(&after.models) {
            if old == now {
                continue;
            }
            for (old, now) in old.atoms.iter_mut().zip(&now.atoms) {
                old.xyz = now.xyz;
            }
            for (old, now) in old.hydrogens.iter_mut().zip(&now.hydrogens) {
                old.xyz = now.xyz;
            }
            if old != now || moved.replace(now).is_some() {
                return None;
            }
        }
        let action = if redo { "Redid" } else { "Undid" };
        if previous.len() == restored.len()
            && previous
                .iter()
                .zip(restored)
                .all(|(a, b)| Arc::ptr_eq(&a.result, &b.result))
        {
            let model = moved?;
            let has_result = restored.iter().any(|stored| {
                what(&stored.result).is_some()
                    && after
                        .science
                        .attachment(stored.attachment)
                        .is_some_and(|link| {
                            link.atoms.iter().flatten().any(|&id| model.contains(id))
                        })
            });
            return has_result.then(|| {
                let source = self.xtb_model_source(model).short;
                format!("{action} the 3D model change · now the {source}")
            });
        }
        let (doc, without, with) = if redo {
            (after, previous, restored)
        } else {
            (&before, restored, previous)
        };
        let contains = |results: &[Stored], stored: &Stored| {
            results
                .iter()
                .any(|other| Arc::ptr_eq(&stored.result, &other.result))
        };
        if without.iter().any(|stored| !contains(with, stored)) {
            return None;
        }
        let stored = with
            .iter()
            .rev()
            .find(|stored| !contains(without, stored))?;
        let what = what(&stored.result)?;
        let link = doc.science.attachment(stored.attachment)?;
        let mut status = format!("{action} xTB {what}");
        if let Some(model) = after
            .models
            .iter()
            .find(|model| link.atoms.iter().flatten().any(|&id| model.contains(id)))
        {
            let source = self.xtb_model_source(model).short;
            let state = if redo { "is" } else { "back to" };
            status.push_str(&format!(" · 3D model {state} the {source}"));
        }
        Some(status)
    }
}

fn what(result: &CalcResult) -> Option<&'static str> {
    if !matches!(result.settings, Settings::Xtb(_)) {
        return None;
    }
    Some(match result.task {
        Task::Optimize if result.vibrations.is_some() => "optimization + frequencies",
        Task::Optimize => "geometry optimization",
        Task::Energy => "energy",
        Task::Hessian => "frequencies",
        Task::Conformers => "conformer search",
        _ => return None,
    })
}

fn drawing_unchanged(before: &Document, after: &Document) -> bool {
    before.atoms == after.atoms
        && before.bonds == after.bonds
        && before.arrows == after.arrows
        && before.graphics == after.graphics
        && before.annotations == after.annotations
        && before.generic == after.generic
        && before.canvas_theme == after.canvas_theme
        && before.color_theme == after.color_theme
        && before.custom_theme == after.custom_theme
        && before.drawing_style == after.drawing_style
        && before.page_layout == after.page_layout
        && before.abbreviations == after.abbreviations
        && before.atom_labels == after.atom_labels
        && before.ring_fills == after.ring_fills
        && before.highlight_style == after.highlight_style
        && before.depth_appearance == after.depth_appearance
        && before.molecule_names == after.molecule_names
        && before.property_captions == after.property_captions
        && before.compound_numbers == after.compound_numbers
        && before.arrow_labels == after.arrow_labels
        && before.groups == after.groups
        && before.reactions == after.reactions
        && before.recent_colors == after.recent_colors
        && before.piece_definitions == after.piece_definitions
        && before.seats == after.seats
}
