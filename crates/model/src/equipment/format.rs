//! The `.reshiki-piece` format: one piece of lab equipment as data only.
//! Built-in and user pieces share it. Parsing is strict: an unknown field
//! names itself in the error, so authors see their typos.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT: &str = "reshiki-piece";
/// Written until the format is frozen; a release that knows only version 1 refuses it.
pub const PREVIEW_VERSION: u32 = 0;
/// The newest format version this build reads.
pub const FORMAT_VERSION: u32 = 1;

pub type Xy = [f64; 2];
pub type Rgb = [u8; 3];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub format: String,
    pub format_version: u32,
    /// `namespace/name`; `reshiki/` is reserved for built-ins.
    pub id: String,
    pub revision: u32,
    pub family: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variant: BTreeMap<String, String>,
    /// By language tag; `en` is required.
    pub names: BTreeMap<String, String>,
    pub category: Category,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub credits: Credits,
    pub units: Units,
    /// Stays put when something attached to it moves: a stand, a hotplate.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub grounded: bool,
    /// A joint clip; its frame is a seat's mating frame (female mouth at the
    /// origin, +y into the socket).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Clip>,
    /// Flexible tubing, drawn along a centre line stored on each placed copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tube: Option<Tube>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<Shape>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joints: Vec<Joint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<Hold>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub liquids: Vec<Liquid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LabelAnchor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Mm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Flask,
    Schlenk,
    Condenser,
    Distillation,
    Adapter,
    Addition,
    Filtration,
    Nmr,
    Chromatography,
    Vessel,
    Heating,
    Holding,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub size: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tube {
    pub width_mm: f64,
    pub material: Material,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    /// The two plug joints at the ends of the centre line.
    pub ends: [String; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Back,
    Body,
    Inner,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Material {
    Glass,
    Frosted,
    Ptfe,
    Rubber,
    Plastic,
    Metal,
    Porcelain,
    Cork,
    Paper,
    Silica,
    Sand,
    Liquid,
    /// Painted like outlines: a stopcock bore, a scale mark.
    Ink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Filled and joined into the piece's one silhouette.
    Glass,
    /// Filled and outlined on its own.
    Solid,
    /// A stroke only.
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleKind {
    Line,
    Shaded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub id: String,
    pub layer: Layer,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: i8,
    pub material: Material,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleKind>,
    /// SVG path data in millimetres.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// Parameter → states in which the shape is drawn.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, Vec<String>>,
}

fn is_zero(v: &i8) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gender {
    M,
    F,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlugFamily {
    Hose,
    Stopper,
    Septum,
    Thermometer,
    Nmr,
    Stir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointDraw {
    Standard,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    pub at: Xy,
    pub dir: Xy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Joint {
    /// Ground glass. Female: `at` is the mouth centre and `dir` points out of
    /// the mouth. Male: `at` is the shoulder centre and `dir` points to the tip.
    Taper {
        id: String,
        size: String,
        gender: Gender,
        at: Xy,
        dir: Xy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        poses: BTreeMap<String, BTreeMap<String, Pose>>,
        #[serde(default = "standard")]
        draw: JointDraw,
    },
    /// Rubber, septa, thermometers, hose barbs: no ground glass.
    Plug {
        id: String,
        family: PlugFamily,
        gender: Gender,
        size_mm: f64,
        at: Xy,
        dir: Xy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        poses: BTreeMap<String, BTreeMap<String, Pose>>,
    },
}

fn standard() -> JointDraw {
    JointDraw::Standard
}

impl Joint {
    pub fn id(&self) -> &str {
        match self {
            Self::Taper { id, .. } | Self::Plug { id, .. } => id,
        }
    }
    pub fn at(&self) -> Xy {
        match self {
            Self::Taper { at, .. } | Self::Plug { at, .. } => *at,
        }
    }
    pub fn dir(&self) -> Xy {
        match self {
            Self::Taper { dir, .. } | Self::Plug { dir, .. } => *dir,
        }
    }
    pub fn gender(&self) -> Gender {
        match self {
            Self::Taper { gender, .. } | Self::Plug { gender, .. } => *gender,
        }
    }
    pub fn part(&self) -> Option<&str> {
        match self {
            Self::Taper { part, .. } | Self::Plug { part, .. } => part.as_deref(),
        }
    }
    pub fn poses(&self) -> &BTreeMap<String, BTreeMap<String, Pose>> {
        match self {
            Self::Taper { poses, .. } | Self::Plug { poses, .. } => poses,
        }
    }
    /// The size-table entry of a standard-drawn taper joint.
    pub fn standard(&self) -> Option<&'static TaperSize> {
        match self {
            Self::Taper {
                size,
                draw: JointDraw::Standard,
                ..
            } => taper(size),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldKind {
    Rail,
    Slider,
    Grip,
    Jaw,
}

/// One diameter, or the range a jaw or slider fits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Diameter {
    One(f64),
    Range([f64; 2]),
}

impl Diameter {
    pub fn range(self) -> [f64; 2] {
        match self {
            Self::One(d) => [d, d],
            Self::Range(r) => r,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub id: String,
    pub kind: HoldKind,
    pub from: Xy,
    pub to: Xy,
    pub diameter_mm: Diameter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// A millimetre range parameter set to the diameter this jaw or slider
    /// closes on: a clamp's fingers meet the neck they hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
}

impl HoldKind {
    /// Rails and grips are held; sliders and jaws hold them.
    pub fn held(self) -> bool {
        matches!(self, Self::Rail | Self::Grip)
    }
    /// The kind this one pairs with.
    pub fn mate(self) -> Self {
        match self {
            Self::Rail => Self::Slider,
            Self::Slider => Self::Rail,
            Self::Grip => Self::Jaw,
            Self::Jaw => Self::Grip,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Mm,
    Deg,
    Fraction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub id: String,
    pub names: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Param {
    Range {
        id: String,
        names: BTreeMap<String, String>,
        min: f64,
        max: f64,
        default: f64,
        unit: Unit,
    },
    States {
        id: String,
        names: BTreeMap<String, String>,
        states: Vec<State>,
        default: String,
    },
    /// The share of a liquid region's drawn area below the surface, 0–1.
    Fill {
        id: String,
        names: BTreeMap<String, String>,
        liquid: String,
        default: f64,
    },
    Color {
        id: String,
        names: BTreeMap<String, String>,
        liquid: String,
        default: Rgb,
    },
}

impl Param {
    pub fn id(&self) -> &str {
        match self {
            Self::Range { id, .. }
            | Self::States { id, .. }
            | Self::Fill { id, .. }
            | Self::Color { id, .. } => id,
        }
    }
    pub fn names(&self) -> &BTreeMap<String, String> {
        match self {
            Self::Range { names, .. }
            | Self::States { names, .. }
            | Self::Fill { names, .. }
            | Self::Color { names, .. } => names,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    Drag,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub at: Xy,
    pub gesture: Gesture,
}

/// Linear in one parameter, measured from its default, so the authored
/// geometry is the default pose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Motion {
    /// Millimetres per unit.
    Translate(Xy),
    Rotate {
        pivot: Xy,
        degrees_per_unit: f64,
    },
    /// Uniform: the factor is 1 + per_unit × (value − default).
    Scale {
        pivot: Xy,
        per_unit: f64,
    },
    /// Along one axis only: a clamp's fingers widening for a thicker neck.
    Stretch {
        pivot: Xy,
        axis: Xy,
        per_unit: f64,
    },
    /// An affine matrix [a, b, c, d, e, f] per state, as in SVG.
    PerState(BTreeMap<String, [f64; 6]>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub param: String,
    pub motion: Motion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<Handle>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Meniscus {
    #[default]
    Concave,
    Convex,
    Flat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Liquid {
    pub id: String,
    pub region: String,
    #[serde(default)]
    pub meniscus: Meniscus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelAnchor {
    pub id: String,
    pub at: Xy,
}

/// A standard-taper size: D/L in millimetres, taper 1:10.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaperSize {
    pub label: &'static str,
    /// Diameter at the wide end.
    pub d: f64,
    /// Ground length.
    pub l: f64,
    /// Outer diameter of the neck tube a socket joins or a cone leaves.
    pub neck: f64,
    /// Drawn socket wall, a little thicker than real glass so it reads at scheme size.
    pub socket_wall: f64,
}

impl TaperSize {
    /// How far a seated cone's shoulder stays outside the socket mouth.
    pub fn exposure(&self) -> f64 {
        0.06 * self.l
    }
    /// Radius of the lip at a socket's mouth.
    pub fn bead_radius(&self) -> f64 {
        self.d / 2. + self.socket_wall + 1.4
    }
}

/// Part of format version 1: pieces from different authors seat cleanly.
pub const TAPER_SIZES: [TaperSize; 9] = [
    size("10/18", 10., 18., 9., 1.7),
    size("14/20", 14., 20., 12., 2.0),
    size("14/23", 14., 23., 12., 2.0),
    size("15/25", 15., 25., 13., 2.1),
    size("19/22", 19., 22., 17., 2.3),
    size("19/28", 19., 28., 17., 2.3),
    size("19/38", 19., 38., 17., 2.3),
    size("24/29", 24., 29., 21., 2.4),
    size("29/42", 29., 42., 26., 2.6),
];

const fn size(label: &'static str, d: f64, l: f64, neck: f64, socket_wall: f64) -> TaperSize {
    TaperSize {
        label,
        d,
        l,
        neck,
        socket_wall,
    }
}

pub fn taper(label: &str) -> Option<&'static TaperSize> {
    TAPER_SIZES.iter().find(|s| s.label == label)
}

/// Parse and validate one `.reshiki-piece` document.
pub fn parse(text: &str) -> Result<Definition, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("The piece file is not valid JSON: {e}."))?;
    if value.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
        return Err("This is not a ReShiki piece file.".into());
    }
    if let Some(v) = value.get("format_version").and_then(|v| v.as_u64())
        && v > u64::from(FORMAT_VERSION)
    {
        return Err(format!(
            "This piece was made with a newer version of ReShiki (piece format {v}). Update ReShiki to open it."
        ));
    }
    // Parse the text, not the value above: a struct refuses a duplicate field,
    // while a JSON value keeps the last of two equal keys.
    let definition: Definition =
        serde_json::from_str(text).map_err(|e| format!("The piece file is not valid: {e}."))?;
    super::validate::validate(&definition)?;
    Ok(definition)
}

/// SHA-256 over the canonical JSON: keys sorted, no whitespace, numbers in
/// shortest round-trip form.
pub fn content_hash(definition: &Definition) -> String {
    use sha2::{Digest, Sha256};
    // A definition holds only strings, numbers and maps with string keys.
    let value = serde_json::to_value(definition).unwrap_or_default();
    let mut text = String::new();
    canonical(&value, &mut text);
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn canonical(value: &serde_json::Value, out: &mut String) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        // -0 and 0 draw alike, so they hash alike.
        Value::Number(n) if n.as_f64() == Some(0.) && n.is_f64() => out.push_str("0.0"),
        other => out.push_str(&other.to_string()),
    }
}
