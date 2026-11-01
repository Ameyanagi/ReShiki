//! Advisory drawing checks and explicit, undoable stereobond fixes.
use super::{App, Tool};
use reshiki::chemistry::stereo_lint::{self, Kind, Note};

#[derive(Debug, Clone)]
pub enum Action {
    Select(usize),
    Reverse,
    Redraw,
    IupacStyleNotes(bool),
}

fn units(notes: &[Note]) -> Vec<u64> {
    let mut atoms: Vec<_> = notes
        .iter()
        .flat_map(|note| {
            note.atoms
                .iter()
                .copied()
                .chain(note.bonds.iter().flat_map(|&(a, b)| [a, b]))
        })
        .collect();
    atoms.sort_unstable();
    atoms.dedup();
    atoms
}

fn summaries(notes: &[Note]) -> Vec<String> {
    let mut grouped: Vec<(Kind, usize)> = vec![];
    for note in notes {
        if let Some((_, count)) = grouped.iter_mut().find(|(kind, _)| *kind == note.kind) {
            *count += 1;
        } else {
            grouped.push((note.kind, 1));
        }
    }
    grouped
        .into_iter()
        .map(|(kind, count)| {
            let plural = if count == 1 { "" } else { "s" };
            match kind {
                Kind::Reversed => format!("{count} stereo bond{plural} probably reversed"),
                Kind::NoEffect => format!("{count} stereo bond{plural} with no effect"),
                Kind::Ambiguous => format!("Ambiguous wedge geometry at {count} centre{plural}"),
                Kind::Conflicting => format!("Conflicting wedges at {count} centre{plural}"),
                Kind::Undefined => notes
                    .iter()
                    .filter(|note| note.kind == kind)
                    .map(|note| note.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" · "),
                Kind::BetweenCentres => format!("{count} stereobond{plural} between stereocentres"),
                Kind::MixedHashes => "Mixed tapered and parallel hashes".into(),
                Kind::NonWedgeOnly => {
                    format!("{count} centre{plural} defined only by nonstandard stereobonds")
                }
                Kind::WavyAlkeneCentre => {
                    format!("{count} wavy bond{plural} between double bonds and stereocentres")
                }
                Kind::ReEntrant => format!("{count} stereobond{plural} at re-entrant ring atoms"),
                Kind::TShape => format!("T-shaped wedge geometry at {count} centre{plural}"),
            }
        })
        .collect()
}

impl App {
    pub(super) fn checked_stereo(&mut self, mut warnings: Vec<String>) {
        self.tab.stereo_notes =
            stereo_lint::check(&self.tab.doc, self.appearance.iupac_style_notes);
        if !self.tab.stereo_notes.is_empty() {
            self.tab.selected = units(&self.tab.stereo_notes);
            self.tool = Tool::Select;
            self.sync_bonds();
        }
        warnings.extend(summaries(&self.tab.stereo_notes));
        self.status = if warnings.is_empty() {
            "No chemistry errors found".into()
        } else {
            format!("Structure checked · {}", warnings.join(" · "))
        };
    }

    pub(super) fn stereo_action(&mut self, action: Action) {
        match action {
            Action::Select(index) => {
                let Some(note) = self.tab.stereo_notes.get(index) else {
                    return;
                };
                self.tab.selected = units(std::slice::from_ref(note));
                self.tool = Tool::Select;
                self.sync_bonds();
            }
            Action::Reverse => self.reverse_stereo_notes(),
            Action::Redraw => self.redraw_stereo_bonds(),
            Action::IupacStyleNotes(enabled) => {
                self.appearance.iupac_style_notes = enabled;
                self.tab.stereo_notes.clear();
                self.status = format!(
                    "IUPAC style notes {} · Check drawing to update notices",
                    if enabled { "on" } else { "off" }
                );
                self.error = false;
                if let Err(error) = self.appearance.save() {
                    self.status = format!("Could not save IUPAC style notes preference: {error}");
                    self.error = true;
                }
            }
        }
    }

    fn reverse_stereo_notes(&mut self) {
        let notes: Vec<_> = self
            .tab
            .stereo_notes
            .iter()
            .filter(|note| note.kind == Kind::Reversed)
            .cloned()
            .collect();
        if notes.is_empty() {
            return;
        }
        // A note identifies exact bonds. Its selected endpoints may also be
        // joined by other bonds, which must retain their direction.
        let affected: Vec<_> = self
            .tab
            .doc
            .bonds
            .iter()
            .filter(|bond| {
                notes
                    .iter()
                    .any(|note| note.bonds.iter().any(|&(a, b)| bond.connects(a, b)))
            })
            .filter(|bond| !bond.is_projected_dative())
            .map(|bond| (bond.a, bond.b))
            .collect();
        if affected.is_empty() {
            return;
        }
        let before = self.tab.doc.clone();
        let atoms: Vec<_> = affected.iter().flat_map(|&(a, b)| [a, b]).collect();
        self.tab.doc.invalidate_chemistry(&atoms);
        for bond in &mut self.tab.doc.bonds {
            if affected.iter().any(|&(a, b)| bond.connects(a, b)) {
                bond.reverse_drawn();
            }
        }
        self.tab.selected = units(&notes);
        self.changed(before);
        self.status = format!(
            "Reversed {} stereo bond{} · Check drawing to update notices",
            affected.len(),
            if affected.len() == 1 { "" } else { "s" }
        );
        self.error = false;
    }

    fn redraw_stereo_bonds(&mut self) {
        match stereo_lint::redraw(&self.tab.doc, &self.tab.selected) {
            Ok((document, count)) => {
                let before = std::mem::replace(&mut self.tab.doc, document);
                self.changed(before);
                self.tab.stereo_notes.clear();
                self.status = if count == 0 {
                    "No stereocentres to redraw".into()
                } else {
                    format!(
                        "Redrew stereo bonds at {count} stereocentre{}",
                        if count == 1 { "" } else { "s" }
                    )
                };
                self.error = false;
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
            }
        }
    }
}

#[cfg(test)]
mod tests;
