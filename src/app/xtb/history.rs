//! Name the calculation or attached geometry restored by drawing history.
use crate::app::App;
use reshiki::document::Document;
use reshiki::models3d::Model3d;
use reshiki::qcdata::{CalcResult, Settings, Task};
use reshiki::science::Stored;
use std::sync::Arc;

impl App {
    pub(in crate::app) fn xtb_history_status(
        &self,
        before: &Document,
        redo: bool,
    ) -> Option<String> {
        let after = &self.tab.doc;
        let previous = &before.science.results;
        let restored = &after.science.results;
        let action = if redo { "Redid" } else { "Undid" };
        let source = |atoms: &[u64]| {
            self.xtb_geometry_sources(atoms)
                .into_iter()
                .next()
                .map(|source| source.short)
                .filter(|short| !short.is_empty())
        };
        if previous.len() == restored.len()
            && previous
                .iter()
                .zip(restored)
                .all(|(a, b)| Arc::ptr_eq(&a.result, &b.result))
        {
            return restored.iter().rev().find_map(|stored| {
                what(&stored.result)?;
                let link = after.science.attachment(stored.attachment)?;
                let atoms: Vec<_> = link.atoms.iter().flatten().copied().collect();
                let old = model(before, &atoms)?;
                let now = model(after, &atoms)?;
                if coordinates(old).eq(coordinates(now)) {
                    return None;
                }
                source(&atoms)
                    .map(|source| format!("{action} the 3D model change · now the {source}"))
            });
        }
        let (doc, without, with) = if redo {
            (after, previous, restored)
        } else {
            (before, restored, previous)
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
        let atoms: Vec<_> = link.atoms.iter().flatten().copied().collect();
        let mut status = format!("{action} xTB {what}");
        if let Some(source) = source(&atoms) {
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

fn model<'a>(doc: &'a Document, atoms: &[u64]) -> Option<&'a Model3d> {
    doc.models
        .iter()
        .find(|model| atoms.iter().any(|id| model.contains(*id)))
}

fn coordinates(model: &Model3d) -> impl Iterator<Item = [f32; 3]> + '_ {
    model
        .atoms
        .iter()
        .map(|atom| atom.xyz)
        .chain(model.hydrogens.iter().map(|atom| atom.xyz))
}
