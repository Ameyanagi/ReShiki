use super::*;
use moruno::atom_labels::{self as labels, Carbons, HydrogenPosition, Number};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Drawing,
    Selection,
}
impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Drawing => "Whole drawing",
            Self::Selection => "Selected atoms",
        })
    }
}
pub struct State {
    pub scope: Scope,
    pub seed: String,
    pub number: String,
    pub size: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            scope: Scope::Drawing,
            seed: "1".into(),
            number: String::new(),
            size: "7.5".into(),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Scope(Scope),
    Carbons(Carbons),
    Hydrogens(bool),
    Position(HydrogenPosition),
    Stereo(bool),
    Seed(String),
    Number,
    ClearNumbers,
    Text(String),
    ApplyText,
    Size(String),
    ApplySize,
    ToolbarStyle,
    PositionIndicators,
    ResetPositions,
    ResetOverrides,
}
impl App {
    pub(super) fn label_ids(&self) -> Vec<u64> {
        self.doc
            .atoms
            .iter()
            .filter(|a| self.labels.scope == Scope::Drawing || self.selected.contains(&a.id))
            .map(|a| a.id)
            .collect()
    }
    pub(super) fn label_action(&mut self, action: Action) {
        if let Err(error) = self.change_labels(action) {
            self.status = error;
            self.error = true;
        }
    }
    fn change_labels(&mut self, action: Action) -> Result<(), String> {
        let before = self.doc.clone();
        let ids = self.label_ids();
        match action {
            Action::Scope(scope) => self.labels.scope = scope,
            Action::Seed(s) => self.labels.seed = s,
            Action::Text(s) => self.labels.number = s,
            Action::Size(s) => self.labels.size = s,
            Action::PositionIndicators => {
                self.selected = ids;
                self.tool = Tool::EditPoints;
                self.status = "Drag a number or stereochemistry handle · Escape finishes".into();
            }
            Action::Carbons(value) => {
                if self.labels.scope == Scope::Drawing {
                    self.doc.atom_labels.carbons = value;
                }
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.carbons = (self.labels.scope == Scope::Selection).then_some(value);
                }
            }
            Action::Hydrogens(value) => {
                if self.labels.scope == Scope::Drawing {
                    self.doc.atom_labels.hydrogens = value;
                }
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.hydrogens = (self.labels.scope == Scope::Selection).then_some(value);
                }
            }
            Action::Position(value) => {
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.hydrogen_position = value;
                }
            }
            Action::Stereo(value) => {
                if self.labels.scope == Scope::Drawing {
                    self.doc.atom_labels.stereo = value;
                }
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.stereo.show =
                        (self.labels.scope == Scope::Selection).then_some(value);
                }
                for b in self
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.show = (self.labels.scope == Scope::Selection).then_some(value);
                }
                self.refresh_due = Some(std::time::Instant::now());
            }
            Action::Number => {
                let sequence = labels::sequence(&self.labels.seed, ids.len())?;
                for (id, text) in ids.iter().zip(sequence) {
                    let atom = self
                        .doc
                        .atom_mut(*id)
                        .ok_or("The numbered atom is no longer available")?;
                    atom.display.number = Some(Number {
                        text,
                        offset: None,
                        style: labels::number_style(),
                    });
                }
            }
            Action::ClearNumbers => {
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.number = None;
                }
            }
            Action::ApplyText => {
                if ids.len() != 1 {
                    return Err("Select one atom to edit its number".into());
                }
                let id = ids
                    .first()
                    .copied()
                    .ok_or("Select one atom to edit its number")?;
                let a = self
                    .doc
                    .atom_mut(id)
                    .ok_or("The numbered atom is no longer available")?;
                let mut display = a.display.clone();
                display
                    .number
                    .get_or_insert_with(|| Number {
                        text: String::new(),
                        offset: None,
                        style: labels::number_style(),
                    })
                    .text = self.labels.number.trim().into();
                display.validate()?;
                a.display = display;
            }
            Action::ApplySize | Action::ToolbarStyle => {
                let size = if matches!(action, Action::ApplySize) {
                    Some(
                        self.labels
                            .size
                            .parse::<f32>()
                            .ok()
                            .filter(|x| x.is_finite() && (4.0..=144.).contains(x))
                            .ok_or("Enter an indicator size from 4 to 144 pt")?,
                    )
                } else {
                    None
                };
                let mut style = self.current_text_style().clone();
                style.script = moruno::typography::Script::Normal;
                style.formula = false;
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    for s in std::iter::once(&mut a.display.stereo.style)
                        .chain(a.display.number.iter_mut().map(|n| &mut n.style))
                    {
                        if let Some(size) = size {
                            s.size_pt = size;
                        } else {
                            *s = style.clone();
                        }
                    }
                }
                for b in self
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    if let Some(size) = size {
                        b.indicator.style.size_pt = size;
                    } else {
                        b.indicator.style = style.clone();
                    }
                }
            }
            Action::ResetPositions => {
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.stereo.offset = None;
                    if let Some(n) = &mut a.display.number {
                        n.offset = None;
                    }
                }
                for b in self
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.offset = None;
                }
            }
            Action::ResetOverrides => {
                for a in self.doc.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
                    a.display.carbons = None;
                    a.display.hydrogens = None;
                    a.display.hydrogen_position = HydrogenPosition::Auto;
                    a.display.stereo.show = None;
                }
                for b in self
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.show = None;
                }
            }
        }
        self.changed(before);
        Ok(())
    }
}
