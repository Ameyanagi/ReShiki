use crate::app::App;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum StatusKind {
    Palette,
    Mode,
    /// Completion feedback belongs to this molecule's drawn atom.
    Molecule(u64),
}

impl App {
    pub(super) fn xtb_palette_exported(
        &mut self,
        result: Result<Option<PathBuf>, String>,
        details: &[String],
        ir: bool,
    ) {
        if !self.tab.xtb.palette.open || self.tabs.temporary_depth != 0 {
            return;
        }
        match result {
            Ok(Some(path)) => {
                let Some(name) = path.file_name() else {
                    return;
                };
                // The file name leads; the IR figure's folder is in the details.
                let mut text = format!("Exported {}", name.to_string_lossy());
                for detail in details {
                    text.push_str(&format!(" · {detail}"));
                }
                if ir {
                    text = format!("{text}\n{}", path.display());
                }
                self.xtb_set_palette_status(StatusKind::Palette, text, false);
            }
            Ok(None) => {}
            Err(error) => {
                let text = if ir {
                    format!("Could not export the IR spectrum\n{error}")
                } else {
                    error
                };
                self.xtb_set_palette_status(StatusKind::Palette, text, true);
            }
        }
    }

    pub(in crate::app::xtb) fn xtb_set_palette_status(
        &mut self,
        kind: StatusKind,
        text: impl Into<String>,
        error: bool,
    ) {
        self.status = text.into();
        self.error = error;
        self.tab.xtb.palette.status = Some((kind, self.status.clone()));
    }

    /// Steps and conformers share geometry controls. Conformer feedback has
    /// its own lifetime.
    pub(super) fn xtb_set_result_status(&mut self, text: impl Into<String>, error: bool) {
        if self
            .xtb_shown()
            .is_some_and(|shown| super::super::conformers::is_search(&shown.stored.result))
        {
            self.status = text.into();
            self.error = error;
            self.tab.xtb.palette.status = None;
        } else {
            self.xtb_set_palette_status(StatusKind::Palette, text, error);
        }
    }

    pub(in crate::app::xtb) fn xtb_set_molecule_status(
        &mut self,
        text: impl Into<String>,
        atoms: &[Option<u64>],
    ) {
        if let Some(&atom) = atoms.iter().flatten().next()
            && self
                .xtb_target()
                .is_some_and(|target| target.contains(&atom))
        {
            let mut text = text.into();
            if let Some((limit, _)) = text.char_indices().nth(60) {
                let end = text.get(..limit).and_then(|prefix| prefix.rfind(" · "));
                text.truncate(end.unwrap_or(limit));
            }
            self.xtb_set_palette_status(StatusKind::Molecule(atom), text, false);
        }
    }

    /// Retire owned feedback unless the effective target is still its molecule.
    pub(in crate::app::xtb) fn xtb_clear_other_molecule_status(&mut self) {
        let Some((StatusKind::Molecule(atom), text)) = &self.tab.xtb.palette.status else {
            return;
        };
        if self.status != *text {
            self.tab.xtb.palette.status = None;
        } else if !self.xtb_target().is_some_and(|atoms| atoms.contains(atom)) {
            self.xtb_clear_palette_status(None);
        }
    }

    pub(in crate::app::xtb) fn xtb_mode_status_is_current(&self) -> bool {
        self.tab
            .xtb
            .palette
            .status
            .as_ref()
            .is_some_and(|(kind, text)| *kind == StatusKind::Mode && *text == self.status)
    }

    pub(in crate::app) fn xtb_clear_palette_status(&mut self, only: Option<StatusKind>) {
        if let Some((_, text)) = self
            .tab
            .xtb
            .palette
            .status
            .take_if(|(kind, _)| only.is_none_or(|only| *kind == only))
            && self.status == text
        {
            self.status.clear();
            self.error = false;
        }
    }
}
