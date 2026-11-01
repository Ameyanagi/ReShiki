//! Bond appearance is independent of chemical order and absolute stereochemistry.
use crate::document::{Bond, Document};
use serde::{Deserialize, Serialize};

/// The directed donor→acceptor coordination order.
pub const DATIVE: u8 = 5;

/// Encode a projection narrowed at its acceptor endpoint.
pub fn narrowed_display(display: &str) -> String {
    format!("{display}_end")
}

/// Dative appearance projects unless it is plain or dashed.
pub fn dative_projects(display: &str) -> bool {
    !matches!(display, "plain" | "dashed")
}

/// Whether an appearance can mark chemical stereochemistry.
pub fn is_stereo_marker(display: &str) -> bool {
    matches!(
        display,
        "wedge" | "hollow_wedge" | "bold" | "hash" | "hashed" | "wavy"
    )
}

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
                || bond.projection && display != "plain" && p.preserves_chemistry(bond))
                && display == bond.base_display()
                && secondary.unwrap_or(display)
                    == bond
                        .secondary_display
                        .as_deref()
                        .unwrap_or_else(|| bond.base_display())
        })
    }
    /// Wedge/line appearance on an aromatic edge depicts its ring projection,
    /// not a change to the chemical order or tetrahedral stereochemistry.
    pub fn preserves_aromatic_order(self, bond: &Bond) -> bool {
        bond.order == 4 && self.parts().0 == 1
    }
    /// Perspective paint on a directed dative edge never assigns tetrahedral
    /// stereochemistry or exchanges its donor and acceptor.
    pub fn preserves_coordination_order(self, bond: &Bond) -> bool {
        bond.is_dative()
            && matches!(
                self,
                Self::Single
                    | Self::Wedge
                    | Self::HashedWedge
                    | Self::HollowWedge
                    | Self::Bold
                    | Self::Hashed
                    | Self::Dative
                    | Self::Dashed
            )
    }
    /// Styling a retained perspective bond must not erase captured stereo.
    pub fn preserves_chemistry(self, bond: &Bond) -> bool {
        self.preserves_aromatic_order(bond)
            || self.preserves_coordination_order(bond)
            || bond.projection && bond.order == 1 && self.parts().0 == 1
    }
    pub fn apply(self, bond: &mut Bond) {
        let (order, display, secondary) = self.parts();
        let narrow_at_acceptor = bond.narrowed_at_acceptor();
        if self.preserves_chemistry(bond) {
            if matches!(bond.order, 4 | 5) {
                // Aromatic projection shares the dative_projects appearance rule.
                bond.projection = dative_projects(display);
            }
        } else {
            bond.ring_arc = false;
            bond.projection = false;
            bond.order = order;
        }
        bond.display = if bond.is_dative()
            && narrow_at_acceptor
            && matches!(display, "wedge" | "hash" | "hollow_wedge")
        {
            narrowed_display(display)
        } else {
            display.into()
        };
        bond.secondary_display = secondary.map(str::to_string);
    }
    /// Shared by committed bond gestures and their live canvas preview. Avoid
    /// replacing an aromatic edge before deciding whether this is only styling.
    pub fn place(self, doc: &mut Document, a: u64, b: u64) {
        let (a, b) = self.initial_wedge_endpoints(doc, a, b);
        if let Some(bond) = doc
            .bonds
            .iter_mut()
            .find(|bond| bond.connects(a, b) && self.preserves_chemistry(bond))
        {
            if bond.a != a && !bond.is_dative() {
                bond.reverse();
            }
            self.apply(bond);
            if bond.is_dative()
                && matches!(self, Self::Wedge | Self::HashedWedge | Self::HollowWedge)
            {
                let display = self.parts().1;
                bond.display = if bond.b == a {
                    narrowed_display(display)
                } else {
                    display.into()
                };
            }
            return;
        }
        let (order, display, _) = self.parts();
        doc.add_bond(a, b, order, display);
        if let Some(bond) = doc.bonds.iter_mut().find(|bond| bond.a == a && bond.b == b) {
            self.apply(bond);
        }
    }
    /// A first wedge on a plain single bond should point from its plausible
    /// stereocenter. Ambiguous endpoints retain the existing bond direction;
    /// new bonds and existing stereo/projection styles retain gesture direction.
    fn initial_wedge_endpoints(self, doc: &Document, a: u64, b: u64) -> (u64, u64) {
        if !matches!(self, Self::Wedge | Self::HashedWedge | Self::HollowWedge) {
            return (a, b);
        }
        let Some(bond) = doc.bonds.iter().find(|bond| bond.connects(a, b)) else {
            return (a, b);
        };
        if bond.order != 1 || bond.display != "plain" || bond.projection {
            return (a, b);
        }
        let degree = |id| {
            doc.bonds
                .iter()
                .filter(|bond| bond.is_covalent() && (bond.a == id || bond.b == id))
                .count()
        };
        let a_degree = degree(bond.a);
        let b_degree = degree(bond.b);
        let qualifies = |id, degree, other_id| {
            !doc.bonds
                .iter()
                .any(|bond| bond.is_covalent() && bond.order != 1 && (bond.a == id || bond.b == id))
                && (heavy_neighbor_count(doc, id) > 1 && heavy_neighbor_count(doc, other_id) == 1
                    || tetrahedral_endpoint(doc, id, degree))
        };
        match (
            qualifies(bond.a, a_degree, bond.b),
            qualifies(bond.b, b_degree, bond.a),
        ) {
            (false, true) => (bond.b, bond.a),
            _ => (bond.a, bond.b),
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

fn heavy_neighbor_count(doc: &Document, id: u64) -> usize {
    doc.bonds
        .iter()
        .filter(|bond| bond.is_covalent() && (bond.a == id || bond.b == id))
        .filter(|bond| {
            doc.atom(if bond.a == id { bond.b } else { bond.a })
                .is_some_and(|neighbor| neighbor.element != "H")
        })
        .count()
}

/// Local property-cache valence avoids depending on stale hydrogen labels or
/// running stereo perception on the entire drawing during a pointer gesture.
fn tetrahedral_endpoint(doc: &Document, id: u64, degree: usize) -> bool {
    use crate::chemistry::{ELEMENTS, graph};
    let Some(atom) = doc.atom(id) else {
        return false;
    };
    let phosphorus_or_sulfur = matches!(atom.element.as_str(), "P" | "S");
    // IUPAC's test counts heavy neighbours; a drawn H is not a substituent.
    let heavy = heavy_neighbor_count(doc, id);
    if heavy < 3
        || degree > if phosphorus_or_sulfur { 6 } else { 4 }
        || atom.aromatic
        || atom.radical_electrons != 0
    {
        return false;
    }
    let Some(atomic_number) = ELEMENTS.iter().position(|e| e.symbol == atom.element) else {
        return false;
    };
    let (Ok(atomic_number), Ok(charge), Ok(explicit_hydrogens)) = (
        u8::try_from(atomic_number),
        i8::try_from(atom.charge),
        u8::try_from(atom.explicit_h),
    ) else {
        return false;
    };
    // Only the focus atom's environment matters. Inert placeholders give each
    // of its explicit single-bond neighbors without their incomplete valences.
    let mut atoms = vec![
        graph::Atom {
            no_implicit: true,
            ..Default::default()
        };
        degree + 1
    ];
    if let Some(focus) = atoms.first_mut() {
        *focus = graph::Atom {
            atomic_number,
            charge,
            explicit_hydrogens,
            no_implicit: atom.no_implicit,
            ..Default::default()
        };
    }
    let local = graph::Graph {
        atoms,
        bonds: (1..=degree)
            .map(|b| graph::Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let Ok(valences) = local.valences() else {
        return false;
    };
    let Some(valence) = valences.first() else {
        return false;
    };
    let total = degree as u32 + atom.explicit_h + valence.implicit_hydrogens;
    total == 4 || phosphorus_or_sulfur && total >= 3
}
impl std::fmt::Display for BondPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl Bond {
    /// A bond with default appearance and no stereochemistry. Pure: it never
    /// touches a document (`Document::add_bond` builds on it).
    pub fn plain(a: u64, b: u64, order: u8, display: &str) -> Bond {
        Bond {
            distance_derived: false,
            highlight: None,
            ring_arc: false,
            projection: false,
            stereo_authoritative: false,
            z_order: 0,
            indicator: Default::default(),
            cip_label: None,
            a,
            b,
            order,
            display: display.into(),
            stereo: None,
            stereo_atoms: vec![],
            double_position: Default::default(),
            secondary_display: None,
            color: Default::default(),
        }
    }
    /// Whether this bond has the directed coordination order.
    pub fn is_dative(&self) -> bool {
        self.order == DATIVE
    }
    /// Whether this directed coordination bond depicts projection depth.
    pub fn is_projected_dative(&self) -> bool {
        self.order == DATIVE && self.projection
    }
    /// Return the appearance with a single narrowed-end suffix removed.
    pub fn base_display(&self) -> &str {
        self.display.strip_suffix("_end").unwrap_or(&self.display)
    }
    /// Whether this coordination appearance narrows at the acceptor.
    pub fn narrowed_at_acceptor(&self) -> bool {
        self.is_dative() && self.display.ends_with("_end")
    }
    /// Whether this bond has a covalent chemical order.
    pub fn is_covalent(&self) -> bool {
        matches!(self.order, 1..=4 | 6 | 7)
    }
    /// Whether captured stereo or nonprojected appearance specifies stereochemistry.
    pub fn specifies_stereo(&self) -> bool {
        self.stereo.is_some()
            || self.stereo_authoritative
            || !self.stereo_atoms.is_empty()
            || (!self.projection && is_stereo_marker(&self.display))
    }
    /// Whether this bond connects the endpoints in either direction.
    pub fn connects(&self, a: u64, b: u64) -> bool {
        (self.a == a && self.b == b) || (self.a == b && self.b == a)
    }
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
            DATIVE if self.projection => [
                "plain",
                "dashed",
                "wedge",
                "hash",
                "bold",
                "hashed",
                "hollow_wedge",
                "wedge_end",
                "hash_end",
                "hollow_wedge_end",
            ]
            .as_slice(),
            DATIVE => ["plain", "dashed"].as_slice(),
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
    /// Flip only the narrowed projection endpoint. Directed chemical a/b stays fixed.
    pub fn reverse_projection(&mut self) -> bool {
        if !self.is_projected_dative() {
            return false;
        }
        self.display = match self.display.as_str() {
            "wedge" => "wedge_end",
            "wedge_end" => "wedge",
            "hash" => "hash_end",
            "hash_end" => "hash",
            "hollow_wedge" => "hollow_wedge_end",
            "hollow_wedge_end" => "hollow_wedge",
            _ => return false,
        }
        .into();
        true
    }
    pub fn reverse(&mut self) {
        std::mem::swap(&mut self.a, &mut self.b);
        self.stereo_atoms.reverse();
        self.double_position = self.double_position.reversed();
    }
    /// Reverse what the user sees, for both bond-tool clicks and Reverse bonds.
    /// Plain datives swap donor and acceptor; projected datives flip only their
    /// projection tip. Other bonds reverse their endpoints.
    pub fn reverse_drawn(&mut self) {
        if self.is_projected_dative() {
            self.reverse_projection();
        } else {
            self.reverse();
        }
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

#[cfg(test)]
mod tests;
