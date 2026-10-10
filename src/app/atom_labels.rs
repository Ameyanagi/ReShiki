use super::*;
use reshiki::atom_labels::{self as labels, Carbons, HydrogenPosition, Number};

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
    Charges(bool),
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
        self.tab
            .doc
            .atoms
            .iter()
            .filter(|a| {
                self.tab.labels.scope == Scope::Drawing || self.tab.selected.contains(&a.id)
            })
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
        let before = self.tab.doc.clone();
        let ids = self.label_ids();
        match action {
            Action::Scope(scope) => self.tab.labels.scope = scope,
            Action::Seed(s) => self.tab.labels.seed = s,
            Action::Text(s) => self.tab.labels.number = s,
            Action::Size(s) => self.tab.labels.size = s,
            Action::PositionIndicators => {
                self.tab.selected = ids;
                self.tool = Tool::EditPoints;
                self.status =
                    "Drag a number, atom map or stereochemistry handle · Escape finishes".into();
            }
            Action::Carbons(value) => {
                if self.tab.labels.scope == Scope::Drawing {
                    self.tab.doc.atom_labels.carbons = value;
                }
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.carbons =
                        (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
            }
            Action::Hydrogens(value) => {
                if self.tab.labels.scope == Scope::Drawing {
                    self.tab.doc.atom_labels.hydrogens = value;
                }
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.hydrogens =
                        (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
            }
            Action::Charges(show) => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.hide_charge = !show;
                }
            }
            Action::Position(value) => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.hydrogen_position = value;
                }
            }
            Action::Stereo(value) => {
                if self.tab.labels.scope == Scope::Drawing {
                    self.tab.doc.atom_labels.stereo = value;
                }
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.stereo.show =
                        (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
                for b in self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.show = (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
                self.tab.labels_dirty = true;
            }
            Action::Number => {
                let sequence = labels::sequence(&self.tab.labels.seed, ids.len())?;
                for (id, text) in ids.iter().zip(sequence) {
                    let atom = self
                        .tab
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
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
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
                    .tab
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
                    .text = self.tab.labels.number.trim().into();
                display.validate()?;
                a.display = display;
            }
            Action::ApplySize | Action::ToolbarStyle => {
                let size = if matches!(action, Action::ApplySize) {
                    Some(
                        self.tab
                            .labels
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
                style.script = reshiki::typography::Script::Normal;
                style.formula = false;
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    for s in std::iter::once(&mut a.display.stereo.style)
                        .chain(std::iter::once(&mut a.display.mapping.style))
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
                    .tab
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
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.stereo.offset = None;
                    a.display.mapping.offset = None;
                    if let Some(n) = &mut a.display.number {
                        n.offset = None;
                    }
                }
                for b in self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.offset = None;
                }
            }
            Action::ResetOverrides => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.carbons = None;
                    a.display.hide_charge = false;
                    a.display.hydrogens = None;
                    a.display.hydrogen_position = HydrogenPosition::Auto;
                    a.display.stereo.show = None;
                    a.display.mapping.show = None;
                }
                for b in self
                    .tab
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
