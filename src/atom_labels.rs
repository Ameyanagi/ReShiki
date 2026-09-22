//! Atom-label appearance and owned number/stereochemistry indicators.
//! These settings never change the molecular graph or reaction atom mapping.
use crate::{
    document::{Atom, Document, Point},
    typography::TextStyle,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Carbons {
    #[default]
    Skeletal,
    Terminal,
    Internal,
    All,
}
impl Carbons {
    pub const ALL: [Self; 4] = [Self::Skeletal, Self::Terminal, Self::Internal, Self::All];
}
impl std::fmt::Display for Carbons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Skeletal => "Skeletal",
            Self::Terminal => "Terminal carbons",
            Self::Internal => "Internal carbons",
            Self::All => "All carbons",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HydrogenPosition {
    #[default]
    Auto,
    Left,
    Right,
    Above,
    Below,
}
impl HydrogenPosition {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Left,
        Self::Right,
        Self::Above,
        Self::Below,
    ];
}
impl std::fmt::Display for HydrogenPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Above => "Above",
            Self::Below => "Below",
        })
    }
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub carbons: Carbons,
    pub hydrogens: bool,
    pub stereo: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            carbons: Carbons::Skeletal,
            hydrogens: yes(),
            stereo: false,
        }
    }
}
pub fn number_style() -> TextStyle {
    TextStyle {
        size_pt: 7.5,
        ..Default::default()
    }
}
pub fn stereo_style() -> TextStyle {
    TextStyle {
        italic: true,
        ..number_style()
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Number {
    pub text: String,
    #[serde(default)]
    pub offset: Option<Point>,
    #[serde(default = "number_style")]
    pub style: TextStyle,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AtomDisplay {
    pub carbons: Option<Carbons>,
    pub hydrogens: Option<bool>,
    pub hydrogen_position: HydrogenPosition,
    pub number: Option<Number>,
    pub stereo: StereoDisplay,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StereoDisplay {
    pub show: Option<bool>,
    pub offset: Option<Point>,
    pub style: TextStyle,
}
impl Default for StereoDisplay {
    fn default() -> Self {
        Self {
            show: None,
            offset: None,
            style: stereo_style(),
        }
    }
}
impl AtomDisplay {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        self.stereo.validate()?;
        if let Some(n) = &self.number {
            if n.text.is_empty()
                || n.text.chars().count() > 32
                || n.text.chars().any(char::is_control)
            {
                return Err("Atom numbers must contain 1–32 printable characters".into());
            }
            n.style.validate()?;
            validate_offset(n.offset)?;
        }
        Ok(())
    }
}
impl StereoDisplay {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        self.style.validate()?;
        validate_offset(self.offset)
    }
}
fn validate_offset(offset: Option<Point>) -> Result<(), String> {
    if offset.is_some_and(|p| {
        !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 10_000. || p.y.abs() > 10_000.
    }) {
        Err("Invalid atom-indicator position".into())
    } else {
        Ok(())
    }
}
pub fn visible(a: &Atom, doc: &Document) -> bool {
    let degree = doc
        .bonds
        .iter()
        .filter(|b| b.a == a.id || b.b == a.id)
        .count();
    a.element != "C"
        || a.radical_electrons != 0
        || a.isotope != 0
        || degree == 0
        || match a.display.carbons.unwrap_or(doc.atom_labels.carbons) {
            Carbons::Skeletal => false,
            Carbons::Terminal => degree == 1,
            Carbons::Internal => degree > 1,
            Carbons::All => true,
        }
}
pub fn hydrogens(a: &Atom, doc: &Document) -> bool {
    a.display.hydrogens.unwrap_or(doc.atom_labels.hydrogens)
}

/// A user-facing sequence, independent of atom IDs and reaction mapping.
pub fn sequence(seed: &str, count: usize) -> Result<Vec<String>, String> {
    if count > 100_000 {
        return Err("Number at most 100,000 atoms at a time".into());
    }
    let seed = seed.trim();
    if seed.is_empty() || seed.chars().count() > 24 || seed.chars().any(char::is_control) {
        return Err("Start with a number, a letter, or a prefix ending in a number".into());
    }
    let split = seed.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if split < seed.len() {
        let (prefix, digits) = seed
            .split_at_checked(split)
            .ok_or("Invalid number prefix")?;
        let start: u64 = digits.parse().map_err(|_| "Atom number is too large")?;
        return (0..count)
            .map(|i| {
                start
                    .checked_add(i as u64)
                    .map(|n| format!("{prefix}{n}"))
                    .ok_or_else(|| "Atom number is too large".into())
            })
            .collect();
    }
    for alphabet in [
        "abcdefghijklmnopqrstuvwxyz",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "αβγδεζηθικλμνξοπρστυφχψω",
        "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ",
    ] {
        let chars: Vec<_> = alphabet.chars().collect();
        if seed.chars().count() == 1
            && let Some(start) = chars.iter().position(|c| seed.starts_with(*c))
        {
            let end = start
                .checked_add(count)
                .ok_or("Atom number sequence is too large")?;
            return Ok((start..end)
                .map(|mut i| {
                    let mut result = String::new();
                    loop {
                        if let Some(c) = chars.get(i % chars.len()) {
                            result.insert(0, *c);
                        }
                        if i < chars.len() {
                            break;
                        }
                        i = i / chars.len() - 1;
                    }
                    result
                })
                .collect());
        }
    }
    Err("Use 1, atom1, a, A, α or Α as the sequence start".into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    Number(u64),
    AtomStereo(u64),
    BondStereo(u64, u64),
}
impl Owner {
    pub fn selected(self, ids: &[u64]) -> bool {
        match self {
            Self::Number(id) | Self::AtomStereo(id) => ids.contains(&id),
            Self::BondStereo(a, b) => ids.contains(&a) && ids.contains(&b),
        }
    }
    pub fn anchor(self, doc: &Document) -> Option<Point> {
        match self {
            Self::Number(id) | Self::AtomStereo(id) => doc.atom(id).map(|a| a.position),
            Self::BondStereo(a, b) => {
                let a = doc.atom(a)?.position;
                let b = doc.atom(b)?.position;
                Some(Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.))
            }
        }
    }
    pub fn set_offset(self, doc: &mut Document, offset: Option<Point>) {
        match self {
            Self::Number(id) => {
                if let Some(n) = doc.atom_mut(id).and_then(|a| a.display.number.as_mut()) {
                    n.offset = offset;
                }
            }
            Self::AtomStereo(id) => {
                if let Some(a) = doc.atom_mut(id) {
                    a.display.stereo.offset = offset;
                }
            }
            Self::BondStereo(a, b) => {
                if let Some(bond) = doc
                    .bonds
                    .iter_mut()
                    .find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
                {
                    bond.indicator.offset = offset;
                }
            }
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Indicator {
    pub owner: Owner,
    pub center: Point,
    pub origin: Point,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub style: TextStyle,
}
impl Indicator {
    pub fn primitive(&self) -> crate::scene::Primitive {
        crate::scene::Primitive::Text {
            position: self.origin,
            text: self.text.clone(),
            size: self.height,
            color: self.style.color,
            style: self.style.clone(),
        }
    }
}

/// Try outward positions first, then minimize label/bond collisions. Manual
/// offsets remain exact. The same positions are used by canvas and all exports.
pub fn indicators(doc: &Document) -> Vec<Indicator> {
    let mut labels = Vec::new();
    for a in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        if let Some(n) = &a.display.number {
            labels.push((
                Owner::Number(a.id),
                n.text.clone(),
                n.offset,
                n.style.clone(),
            ));
        }
        if a.display.stereo.show.unwrap_or(doc.atom_labels.stereo)
            && let Some(cip) = &a.cip_label
        {
            labels.push((
                Owner::AtomStereo(a.id),
                format!("({cip})"),
                a.display.stereo.offset,
                a.display.stereo.style.clone(),
            ));
        }
    }
    for b in doc.bonds.iter().filter(|b| doc.bond_visible(b.a, b.b)) {
        if b.indicator.show.unwrap_or(doc.atom_labels.stereo)
            && let Some(cip) = &b.cip_label
        {
            labels.push((
                Owner::BondStereo(b.a, b.b),
                format!("({cip})"),
                b.indicator.offset,
                b.indicator.style.clone(),
            ));
        }
    }
    if labels.is_empty() {
        return vec![];
    }
    let mut occupied: Vec<_> = doc
        .atoms
        .iter()
        .map(|a| {
            crate::scene::atom_label_bounds(a, doc)
                .unwrap_or((a.position.offset(-3., -3.), a.position.offset(3., 3.)))
        })
        .collect();
    // Reserve all manual indicators first, including owners later in atom order.
    for (owner, text, offset, style) in &labels {
        if let Some(offset) = offset
            && let Some(anchor) = owner.anchor(doc)
        {
            let height = crate::style::DEFAULT.world(style.size_pt);
            let width = crate::style::styled_text_width(text, height, style);
            let center = anchor.offset(offset.x, offset.y);
            occupied.push((
                center.offset(-width / 2. - 3., -height / 2. - 3.),
                center.offset(width / 2. + 3., height / 2. + 3.),
            ));
        }
    }
    let positions: std::collections::HashMap<_, _> =
        doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    let segments: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| Some((*positions.get(&b.a)?, *positions.get(&b.b)?)))
        .collect();
    let mut result = Vec::new();
    for (owner, text, offset, style) in labels {
        let Some(anchor) = owner.anchor(doc) else {
            continue;
        };
        let height = crate::style::DEFAULT.world(style.size_pt);
        let width = crate::style::styled_text_width(&text, height, &style);
        let bounds = |center: Point| {
            (
                center.offset(-width / 2. - 3., -height / 2. - 3.),
                center.offset(width / 2. + 3., height / 2. + 3.),
            )
        };
        let center = if let Some(p) = offset {
            anchor.offset(p.x, p.y)
        } else {
            let neighbors: Vec<_> = match owner {
                Owner::Number(id) | Owner::AtomStereo(id) => doc
                    .bonds
                    .iter()
                    .filter_map(|b| {
                        if b.a == id {
                            positions.get(&b.b).copied()
                        } else if b.b == id {
                            positions.get(&b.a).copied()
                        } else {
                            None
                        }
                    })
                    .collect(),
                Owner::BondStereo(a, b) => [positions.get(&a).copied(), positions.get(&b).copied()]
                    .into_iter()
                    .flatten()
                    .collect(),
            };
            let preferred = crate::editing::open_angle(anchor, &neighbors);
            let mut best = (f32::INFINITY, anchor.offset(0., -height));
            for radius in [
                height * 0.85 + 6.,
                height * 1.35 + 6.,
                height * 1.85 + 6.,
                height * 2.5 + 6.,
                height * 3.25 + 6.,
            ] {
                for i in 0..24 {
                    let angle = preferred + i as f32 * std::f32::consts::TAU / 24.;
                    let center = anchor.offset(radius * angle.cos(), radius * angle.sin());
                    let (lo, hi) = bounds(center);
                    let mut score = radius * 0.08 + i as f32 * 0.001;
                    for (a, b) in &occupied {
                        let overlap = (hi.x.min(b.x) - lo.x.max(a.x)).max(0.)
                            * (hi.y.min(b.y) - lo.y.max(a.y)).max(0.);
                        score += overlap * 10.;
                    }
                    for &(a, b) in &segments {
                        // Include space for the secondary line of double bonds.
                        if segment_hits_rect(a, b, lo.offset(-4., -4.), hi.offset(4., 4.)) {
                            score += 2000.;
                        }
                    }
                    if score < best.0 {
                        best = (score, center);
                    }
                }
            }
            best.1
        };
        occupied.push(bounds(center));
        result.push(Indicator {
            owner,
            center,
            origin: center.offset(-width / 2., -height / 2.),
            width,
            height,
            text,
            style,
        });
    }
    result
}

pub fn clear_computed(doc: &mut Document) {
    for a in &mut doc.atoms {
        a.cip_label = None;
    }
    for b in &mut doc.bonds {
        b.cip_label = None;
    }
}
pub fn refresh_computed(doc: &mut Document, checked: &Document) {
    for a in &mut doc.atoms {
        if let Some(source) = checked.atom(a.id) {
            a.label_h = source.label_h;
            a.cip_label = source.cip_label.clone();
        }
    }
    for b in &mut doc.bonds {
        b.cip_label = checked
            .bonds
            .iter()
            .find(|s| (s.a == b.a && s.b == b.b) || (s.a == b.b && s.b == b.a))
            .and_then(|s| s.cip_label.clone());
    }
}

/// Slab clipping catches a bond crossing a label between sample points.
fn segment_hits_rect(a: Point, b: Point, lo: Point, hi: Point) -> bool {
    let (mut enter, mut exit) = (0_f32, 1_f32);
    for (start, end, low, high) in [(a.x, b.x, lo.x, hi.x), (a.y, b.y, lo.y, hi.y)] {
        let delta = end - start;
        if delta.abs() < 0.0001 {
            if start < low || start > high {
                return false;
            }
        } else {
            let first = (low - start) / delta;
            let last = (high - start) / delta;
            enter = enter.max(first.min(last));
            exit = exit.min(first.max(last));
            if enter > exit {
                return false;
            }
        }
    }
    true
}
