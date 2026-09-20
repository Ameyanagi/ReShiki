//! Cleanup request options; these are session state, not document formatting.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    SelectedAtoms,
    SelectedMolecules,
    #[default]
    Drawing,
}
impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SelectedAtoms => "Selected atoms",
            Self::SelectedMolecules => "Selected molecules",
            Self::Drawing => "Whole drawing",
        })
    }
}
impl Scope {
    pub fn hint(self) -> &'static str {
        match self {
            Self::SelectedAtoms => "Unselected atoms stay fixed. Labels and arrows stay in place.",
            Self::SelectedMolecules => {
                "Whole molecules containing selected atoms. Other objects stay in place."
            }
            Self::Drawing => "Each molecule keeps its center. Labels and arrows stay in place.",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub scope: Scope,
    pub keep_orientation: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            scope: Scope::Drawing,
            keep_orientation: true,
        }
    }
}
