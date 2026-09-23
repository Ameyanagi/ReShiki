//! Bond appearance is independent of chemical order and absolute stereochemistry.
use crate::document::{Bond, Document};
use serde::{Deserialize, Serialize};

/// A continuous wave with smooth tangents, shared by the canvas, icons and exports.
pub fn wavy_path(
    start: crate::document::Point,
    end: crate::document::Point,
    wavelength: f32,
    amplitude: f32,
) -> Vec<crate::graphics::PathCommand> {
    use crate::{document::Point, graphics::PathCommand};
    let length = start.distance(end);
    if length < 0.001 {
        return vec![];
    }
    let cycles = (length / wavelength.max(0.001)).round().clamp(1., 64.) as u32;
    let quarters = cycles * 4;
    let step = length / quarters as f32;
    let amplitude = amplitude.min(length / 6.);
    let tangent = amplitude * std::f32::consts::FRAC_PI_2 / 3.;
    let ux = (end.x - start.x) / length;
    let uy = (end.y - start.y) / length;
    let point = |x: f32, y: f32| Point::new(start.x + ux * x - uy * y, start.y + uy * x + ux * y);
    let phase = |i: u32| match i % 4 {
        0 => (0., tangent),
        1 => (amplitude, 0.),
        2 => (0., -tangent),
        _ => (-amplitude, 0.),
    };
    let mut commands = vec![PathCommand::Move(start)];
    for i in 0..quarters {
        let (y0, d0) = phase(i);
        let (y1, d1) = phase(i + 1);
        let x = i as f32 * step;
        commands.push(PathCommand::Cubic(
            point(x + step / 3., y0 + d0),
            point(x + step * 2. / 3., y1 - d1),
            if i + 1 == quarters {
                end
            } else {
                point(x + step, y1)
            },
        ));
    }
    commands
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoublePosition {
    #[default]
    Auto,
    Center,
    Left,
    Right,
}
impl DoublePosition {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Center, Self::Left, Self::Right];
    pub fn cycled(self) -> Self {
        match self {
            Self::Auto | Self::Center => Self::Left,
            Self::Left => Self::Right,
            Self::Right => Self::Center,
        }
    }
    pub fn reversed(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            other => other,
        }
    }
}
impl std::fmt::Display for DoublePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic",
            Self::Center => "Centered",
            Self::Left => "Left of bond",
            Self::Right => "Right of bond",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondPreset {
    Single,
    Double,
    Triple,
    Wedge,
    HashedWedge,
    Wavy,
    Bold,
    Dashed,
    Dotted,
    Hashed,
    HollowWedge,
    DashedDouble,
    DoubleDashed,
    BoldDouble,
    CrossedDouble,
    Dative,
    Quadruple,
}
impl BondPreset {
    pub const ALL: [Self; 17] = [
        Self::Single,
        Self::Double,
        Self::Triple,
        Self::Wedge,
        Self::HashedWedge,
        Self::HollowWedge,
        Self::Wavy,
        Self::Bold,
        Self::Dashed,
        Self::Dotted,
        Self::Hashed,
        Self::DashedDouble,
        Self::DoubleDashed,
        Self::BoldDouble,
        Self::CrossedDouble,
        Self::Dative,
        Self::Quadruple,
    ];
    pub fn parts(self) -> (u8, &'static str, Option<&'static str>) {
        match self {
            Self::Single => (1, "plain", None),
            Self::Double => (2, "plain", None),
            Self::Triple => (3, "plain", None),
            Self::Wedge => (1, "wedge", None),
            Self::HashedWedge => (1, "hash", None),
            Self::Wavy => (1, "wavy", None),
            Self::Bold => (1, "bold", None),
            Self::Dashed => (5, "dashed", None),
            Self::Dotted => (0, "dotted", None),
            Self::Hashed => (1, "hashed", None),
            Self::HollowWedge => (1, "hollow_wedge", None),
            Self::DashedDouble => (7, "plain", Some("dashed")),
            Self::DoubleDashed => (7, "dashed", None),
            Self::BoldDouble => (2, "bold", Some("plain")),
            Self::CrossedDouble => (2, "wavy", None),
            Self::Dative => (5, "plain", None),
            Self::Quadruple => (6, "plain", None),
        }
    }
    pub fn of(bond: &Bond) -> Option<Self> {
        Self::ALL.into_iter().find(|p| {
            let (order, display, secondary) = p.parts();
            (order == bond.order
                || bond.projection && display != "plain" && p.preserves_aromatic_order(bond))
                && display == bond.display
                && secondary.unwrap_or(display)
                    == bond.secondary_display.as_deref().unwrap_or(&bond.display)
        })
    }
    /// Wedge/line appearance on an aromatic edge depicts its ring projection,
    /// not a change to the chemical order or tetrahedral stereochemistry.
    pub fn preserves_aromatic_order(self, bond: &Bond) -> bool {
        bond.order == 4 && self.parts().0 == 1
    }
    pub fn apply(self, bond: &mut Bond) {
        let (order, display, secondary) = self.parts();
        if self.preserves_aromatic_order(bond) {
            bond.projection = display != "plain";
        } else {
            bond.ring_arc = false;
            bond.projection = false;
            bond.order = order;
        }
        bond.display = display.into();
        bond.secondary_display = secondary.map(str::to_string);
    }
    /// Shared by committed bond gestures and their live canvas preview. Avoid
    /// replacing an aromatic edge before deciding whether this is only styling.
    pub fn place(self, doc: &mut Document, a: u64, b: u64) {
        if let Some(bond) = doc.bonds.iter_mut().find(|bond| {
            ((bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
                && self.preserves_aromatic_order(bond)
        }) {
            if bond.a != a {
                bond.reverse();
            }
            self.apply(bond);
            return;
        }
        let (order, display, _) = self.parts();
        doc.add_bond(a, b, order, display);
        if let Some(bond) = doc.bonds.iter_mut().find(|bond| bond.a == a && bond.b == b) {
            self.apply(bond);
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Double => "Double",
            Self::Triple => "Triple",
            Self::Wedge => "Solid wedge",
            Self::HashedWedge => "Hashed wedge",
            Self::HollowWedge => "Hollow wedge",
            Self::Wavy => "Wavy",
            Self::Bold => "Bold",
            Self::Dashed => "Coordination (dashed)",
            Self::Dotted => "Hydrogen bond",
            Self::Hashed => "Hashed",
            Self::DashedDouble => "Partial (solid / dashed)",
            Self::DoubleDashed => "Partial (double dashed)",
            Self::BoldDouble => "Bold double",
            Self::CrossedDouble => "Crossed double",
            Self::Dative => "Dative",
            Self::Quadruple => "Quadruple",
        }
    }
}
impl std::fmt::Display for BondPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl Bond {
    pub fn validate_appearance(&self) -> Result<(), String> {
        let allowed = match self.order {
            1 => [
                "plain",
                "wedge",
                "hash",
                "wavy",
                "bold",
                "hashed",
                "hollow_wedge",
            ]
            .as_slice(),
            2 => ["plain", "wavy", "bold", "dashed"].as_slice(),
            4 if self.projection => [
                "plain",
                "wedge",
                "hash",
                "wavy",
                "bold",
                "hashed",
                "hollow_wedge",
            ]
            .as_slice(),
            0 => ["dotted"].as_slice(),
            5 => ["plain", "dashed"].as_slice(),
            7 => ["plain", "dashed"].as_slice(),
            _ => ["plain"].as_slice(),
        };
        if !allowed.contains(&self.display.as_str())
            || self.secondary_display.as_ref().is_some_and(|display| {
                ![2, 7].contains(&self.order)
                    || self.display == "wavy"
                    || !["plain", "dashed", "bold"].contains(&display.as_str())
            })
        {
            return Err("Unsupported bond appearance for this order".into());
        }
        Ok(())
    }
    pub fn reverse(&mut self) {
        std::mem::swap(&mut self.a, &mut self.b);
        self.stereo_atoms.reverse();
        self.double_position = self.double_position.reversed();
    }
}

/// A hydrogen bond connects a covalently bound explicit H to an acceptor.
pub fn hydrogen_endpoints(doc: &crate::document::Document, a: u64, b: u64) -> bool {
    let Some(h) = doc.atom(a) else { return false };
    let Some(acceptor) = doc.atom(b) else {
        return false;
    };
    h.element == "H"
        && ["N", "O", "F", "S"].contains(&acceptor.element.as_str())
        && acceptor.charge <= 0
        && doc.bonds.iter().any(|bond| {
            bond.order == 1 && ((bond.a == a && bond.b != b) || (bond.b == a && bond.a != b))
        })
}
