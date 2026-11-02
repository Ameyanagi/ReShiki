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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Keep,
    Free,
    Iupac,
}
impl std::fmt::Display for Orientation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Keep => "Keep orientation",
            Self::Free => "Free",
            Self::Iupac => "IUPAC (fused rings)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub scope: Scope,
    pub keep_orientation: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub iupac_orientation: bool,
}
impl Options {
    pub fn orientation(self) -> Orientation {
        if self.iupac_orientation {
            Orientation::Iupac
        } else if self.keep_orientation {
            Orientation::Keep
        } else {
            Orientation::Free
        }
    }

    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.keep_orientation = orientation == Orientation::Keep;
        self.iupac_orientation = orientation == Orientation::Iupac;
    }
}
impl Default for Options {
    fn default() -> Self {
        Self {
            scope: Scope::Drawing,
            keep_orientation: true,
            iupac_orientation: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_preserves_existing_json_and_defaults() -> serde_json::Result<()> {
        for (keep, orientation) in [(true, Orientation::Keep), (false, Orientation::Free)] {
            let json = format!("{{\"scope\":\"drawing\",\"keep_orientation\":{keep}}}");
            let mut options: Options = serde_json::from_str(&json)?;
            assert_eq!(options.orientation(), orientation);
            assert_eq!(serde_json::to_string(&options)?, json);
            options.set_orientation(Orientation::Iupac);
            let encoded = serde_json::to_string(&options)?;
            assert!(encoded.contains("\"iupac_orientation\":true"));
            assert_eq!(serde_json::from_str::<Options>(&encoded)?, options);
            options.set_orientation(orientation);
            assert_eq!(serde_json::to_string(&options)?, json);
        }
        assert_eq!(Options::default().orientation(), Orientation::Keep);
        assert_eq!(
            Options {
                iupac_orientation: true,
                ..Options::default()
            }
            .orientation(),
            Orientation::Iupac
        );
        Ok(())
    }
}
