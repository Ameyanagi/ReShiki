//! Build known haptic ligands from chemistry first, then project their geometry.
use super::{ContactStyle, DrawingSettings};
use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LigandKind {
    Cp,
    #[serde(rename = "Cp*")]
    CpStar,
}
impl LigandKind {
    fn label(self) -> &'static str {
        match self {
            Self::Cp => "Cp",
            Self::CpStar => "Cp*",
        }
    }
    pub(super) fn atom_count(self) -> usize {
        match self {
            Self::Cp => 6,
            Self::CpStar => 11,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ligand {
    pub kind: LigandKind,
    pub center: Point,
    /// Rotate within the flat ring before applying perspective.
    #[serde(default)]
    pub phase_degrees: f32,
    pub x_degrees: f32,
    pub y_degrees: f32,
    pub rotation_degrees: f32,
    pub depth_bonds: bool,
    #[serde(default = "show_charge_default")]
    pub show_charge: bool,
    pub contact: Option<usize>,
    pub contact_style: Option<ContactStyle>,
    #[serde(default)]
    pub contact_in_front: bool,
}
fn show_charge_default() -> bool {
    true
}
impl Ligand {
    pub(super) fn validate(&self, atom_count: usize) -> Result<(), String> {
        if [self.center.x, self.center.y]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 50.)
            || [self.x_degrees, self.y_degrees]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 85.)
            || !self.rotation_degrees.is_finite()
            || self.rotation_degrees.abs() > 360.
            || !self.phase_degrees.is_finite()
            || self.phase_degrees.abs() > 360.
            || self.contact.is_some_and(|i| i >= atom_count)
            || (self.contact.is_none() && self.contact_style.is_some())
        {
            return Err("Invalid defined ligand position, tilt or contact".into());
        }
        Ok(())
    }

    pub(super) fn append(
        &self,
        doc: &mut Document,
        settings: &DrawingSettings,
    ) -> Result<(), String> {
        let center = Point::new(
            self.center.x * settings.bond_length,
            self.center.y * settings.bond_length,
        );
        let anchor = doc.add_atom("C", center);
        // Use the existing Cp/Cp* definition: five ring atoms, explicit ligand
        // hydrogens, charge -1, and a real ALL-target attachment node.
        *doc = crate::ligands::replace(doc, anchor, self.kind.label())?;
        let members = doc
            .abbreviation(anchor)
            .ok_or("Missing defined ligand")?
            .members
            .clone();
        let ring = doc
            .atom(anchor)
            .ok_or("Missing ligand attachment")?
            .centroid
            .clone();
        doc.expand_abbreviations(&[anchor]);
        let scale = settings.bond_length / doc.drawing_style.bond_length_world;
        crate::editing::transform_about(doc, &members, center, scale, 0.);
        for atom in doc.atoms.iter_mut().filter(|a| members.contains(&a.id)) {
            atom.aromatic = ring.contains(&atom.id);
            atom.text_style = Some(settings.format.style.clone());
            if ring.contains(&atom.id) {
                atom.display.hide_charge = !self.show_charge;
            }
        }
        for bond in doc
            .bonds
            .iter_mut()
            .filter(|b| members.contains(&b.a) && members.contains(&b.b))
        {
            if ring.contains(&bond.a) && ring.contains(&bond.b) {
                bond.order = 4;
            }
            bond.color = settings.bond_color;
        }
        // The aromatic circle follows the ring's stored XYZ plane. No ellipse
        // or manually squashed 2D ring is substituted for the chemical graph.
        crate::editing::transform_about(doc, &members, center, 1., self.phase_degrees);
        crate::projection::tilt(doc, &members, self.x_degrees, true);
        crate::projection::tilt(doc, &members, self.y_degrees, false);
        crate::editing::transform_about(doc, &members, center, 1., self.rotation_degrees);
        if self.depth_bonds {
            crate::projection::depth_bonds(doc, &ring);
        }
        if let Some(contact) = self.contact {
            let (order, display) = self.contact_style.unwrap_or(ContactStyle::Single).parts();
            doc.add_bond(anchor, contact as u64 + 1, order, display);
            if let Some(bond) = doc.bonds.last_mut() {
                bond.z_order = if self.contact_in_front { 1 } else { -1 };
                bond.color = settings.bond_color;
            }
        }
        doc.validate()
    }
}
