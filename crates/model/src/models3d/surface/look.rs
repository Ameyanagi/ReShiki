//! How an orbital surface is drawn: fill, mesh lines and opacity, stored
//! with a placed figure's parameters.
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fill {
    Solid,
    #[default]
    Shaded,
    Outline,
    None,
}

/// How a surface is drawn; stored with a placed figure's parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LookFields")]
pub struct Look {
    pub fill: Fill,
    pub mesh: bool,
    /// Opacity as a percentage.
    pub opacity: u8,
}

const DEFAULT_OPACITY: u8 = 100;

impl Default for Look {
    fn default() -> Self {
        Self {
            fill: Fill::default(),
            mesh: false,
            opacity: DEFAULT_OPACITY,
        }
    }
}

impl Look {
    pub fn new(fill: Fill, mesh: bool, opacity: u8) -> Result<Self, String> {
        if !(20..=100).contains(&opacity) {
            return Err(format!(
                "Surface opacity {opacity} must be between 20 and 100"
            ));
        }
        if fill == Fill::None && !mesh {
            return Err("Surface fill 'none' requires mesh lines".into());
        }
        Ok(Self {
            fill,
            mesh,
            opacity,
        })
    }

    pub fn alpha(self) -> f32 {
        f32::from(self.opacity) / 100.
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct LookFields {
    fill: Fill,
    mesh: bool,
    opacity: u8,
}

impl Default for LookFields {
    fn default() -> Self {
        let Look {
            fill,
            mesh,
            opacity,
        } = Look::default();
        Self {
            fill,
            mesh,
            opacity,
        }
    }
}

impl TryFrom<LookFields> for Look {
    type Error = String;

    fn try_from(fields: LookFields) -> Result<Self, Self::Error> {
        Self::new(fields.fill, fields.mesh, fields.opacity)
    }
}

impl FromStr for Look {
    type Err = String;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        let (fill, mesh) = match token {
            "solid" => (Fill::Solid, false),
            "shaded" => (Fill::Shaded, false),
            "outline" => (Fill::Outline, false),
            "mesh" => (Fill::None, true),
            "solid+mesh" => (Fill::Solid, true),
            "shaded+mesh" => (Fill::Shaded, true),
            "outline+mesh" => (Fill::Outline, true),
            _ => {
                return Err(format!(
                    "Unknown surface look {token:?}; use solid, shaded, outline or mesh (or add +mesh to a fill)"
                ));
            }
        };
        Self::new(fill, mesh, DEFAULT_OPACITY)
    }
}

impl fmt::Display for Look {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match (self.fill, self.mesh) {
            (Fill::Solid, false) => "solid",
            (Fill::Shaded, false) => "shaded",
            (Fill::Outline, false) => "outline",
            (Fill::Solid, true) => "solid+mesh",
            (Fill::Shaded, true) => "shaded+mesh",
            (Fill::Outline, true) => "outline+mesh",
            (Fill::None, _) => "mesh",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json, to_value};

    #[test]
    fn round_trip() {
        let look = Look::new(Fill::Solid, true, 60).unwrap();
        let value = to_value(look).unwrap();
        assert_eq!(value, json!({"fill":"solid","mesh":true,"opacity":60}));
        assert_eq!(from_value::<Look>(value).unwrap(), look);
        assert_eq!(look.alpha(), 0.6);
    }

    #[test]
    fn missing_fields_use_defaults() {
        for (value, expected) in [
            (json!({}), Look::default()),
            (
                json!({"fill":"solid"}),
                Look::new(Fill::Solid, false, 100).unwrap(),
            ),
            (
                json!({"mesh":true}),
                Look::new(Fill::Shaded, true, 100).unwrap(),
            ),
            (
                json!({"opacity":30}),
                Look::new(Fill::Shaded, false, 30).unwrap(),
            ),
        ] {
            assert_eq!(from_value::<Look>(value).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_fields_are_rejected() {
        for opacity in [19, 101] {
            let err = from_value::<Look>(json!({"opacity":opacity})).unwrap_err();
            assert!(err.to_string().contains("between 20 and 100"));
        }
        for value in [json!({"fill":"none"}), json!({"fill":"none","mesh":false})] {
            let err = from_value::<Look>(value).unwrap_err();
            assert!(err.to_string().contains("requires mesh"));
        }
    }

    #[test]
    fn developer_tokens_round_trip() {
        for token in [
            "solid",
            "shaded",
            "outline",
            "mesh",
            "solid+mesh",
            "shaded+mesh",
            "outline+mesh",
        ] {
            assert_eq!(token.parse::<Look>().unwrap().to_string(), token);
        }
    }
}
