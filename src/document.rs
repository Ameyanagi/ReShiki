use crate::typography::{TextFormat, TextStyle};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

/// Winding is relative to the explicit neighbor order, not a toolkit index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomStereo {
    pub winding: String,
    pub neighbors: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    #[serde(
        default,
        skip_serializing_if = "crate::atom_labels::AtomDisplay::is_default"
    )]
    pub display: crate::atom_labels::AtomDisplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cip_label: Option<String>,
    pub id: u64,
    pub element: String,
    pub position: Point,
    /// Projection depth in drawing units, retained when tilting back.
    #[serde(default)]
    pub depth: f32,
    /// A nonchemical attachment point tracking the mean of these atom IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub centroid: Vec<u64>,
    #[serde(default)]
    pub charge: i32,
    #[serde(default)]
    pub radical_electrons: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<crate::scientific::AtomMark>,
    #[serde(default)]
    pub isotope: u32,
    #[serde(default)]
    pub explicit_h: u32,
    #[serde(default)]
    pub no_implicit: bool,
    #[serde(default)]
    pub aromatic: bool,
    #[serde(default)]
    pub stereo: Option<AtomStereo>,
    #[serde(default)]
    pub map_num: u32,
    #[serde(default)]
    pub label_h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_style: Option<TextStyle>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    /// Boldness describes projection depth, never a tetrahedral wedge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub projection: bool,
    #[serde(default)]
    pub z_order: i16,
    #[serde(
        default,
        skip_serializing_if = "crate::atom_labels::StereoDisplay::is_default"
    )]
    pub indicator: crate::atom_labels::StereoDisplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cip_label: Option<String>,
    pub a: u64,
    pub b: u64,
    /// 0 = hydrogen interaction; 1–3 = order; 4 = aromatic; 5 = dative;
    /// 6 = quadruple; 7 = nonaromatic partial order 1.5.
    pub order: u8,
    #[serde(default = "plain")]
    pub display: String,
    #[serde(default)]
    pub stereo: Option<String>,
    #[serde(default)]
    pub stereo_atoms: Vec<u64>,
    #[serde(default)]
    pub double_position: crate::bonds::DoublePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_display: Option<String>,
    #[serde(default)]
    pub color: [u8; 3],
}
fn plain() -> String {
    "plain".into()
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: u64,
    pub position: Point,
    pub text: String,
    #[serde(default)]
    pub format: TextFormat,
}
impl Annotation {
    pub fn size(&self) -> (f32, f32) {
        let layout = crate::typography::layout(&self.text, &self.format);
        (layout.width, layout.height)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arrow {
    pub id: u64,
    pub start: Point,
    pub end: Point,
    #[serde(default = "forward")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<crate::arrows::ArrowStyle>,
}
fn forward() -> String {
    "forward".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    #[serde(
        default,
        skip_serializing_if = "crate::style::DrawingStyle::is_default"
    )]
    pub drawing_style: crate::style::DrawingStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_layout: Option<crate::pages::Layout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abbreviations: Vec<crate::abbreviations::Abbreviation>,
    #[serde(default)]
    pub atom_labels: crate::atom_labels::Settings,
    pub version: u32,
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub arrows: Vec<Arrow>,
    #[serde(default)]
    pub graphics: Vec<crate::graphics::Graphic>,
    #[serde(default)]
    pub groups: Vec<crate::grouping::Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<crate::reactions::Reaction>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 15,
            drawing_style: Default::default(),
            page_layout: None,
            abbreviations: vec![],
            atom_labels: Default::default(),
            atoms: vec![],
            bonds: vec![],
            annotations: vec![],
            arrows: vec![],
            graphics: vec![],
            groups: vec![],
            reactions: vec![],
        }
    }
}
impl Document {
    pub fn next_id(&self) -> u64 {
        self.atoms
            .iter()
            .map(|a| a.id)
            .chain(self.annotations.iter().map(|a| a.id))
            .chain(self.arrows.iter().map(|a| a.id))
            .chain(self.graphics.iter().map(|a| a.id))
            .chain(self.groups.iter().map(|a| a.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
    pub fn add_atom(&mut self, element: &str, position: Point) -> u64 {
        let id = self.next_id();
        self.atoms.push(Atom {
            display: Default::default(),
            cip_label: None,
            id,
            element: element.into(),
            position,
            depth: 0.,
            centroid: vec![],
            charge: 0,
            radical_electrons: 0,
            marks: vec![],
            isotope: 0,
            explicit_h: 0,
            no_implicit: element == "*",
            aromatic: false,
            stereo: None,
            map_num: 0,
            label_h: 0,
            text_style: None,
        });
        id
    }
    pub fn atom(&self, id: u64) -> Option<&Atom> {
        self.atoms.iter().find(|a| a.id == id)
    }
    pub fn atom_mut(&mut self, id: u64) -> Option<&mut Atom> {
        self.atoms.iter_mut().find(|a| a.id == id)
    }
    pub fn nearest(&self, point: Point, radius: f32) -> Option<u64> {
        self.atoms
            .iter()
            .filter(|a| self.atom_visible(a.id) && a.position.distance(point) < radius)
            .min_by(|a, b| {
                a.position
                    .distance(point)
                    .total_cmp(&b.position.distance(point))
            })
            .map(|a| a.id)
            .or_else(|| crate::abbreviations::label_hit(self, point, radius))
    }
    pub fn add_bond(&mut self, a: u64, b: u64, order: u8, display: &str) {
        if a == b || self.atom(a).is_none() || self.atom(b).is_none() {
            return;
        }
        self.invalidate_chemistry(&[a, b]);
        if let Some(bond) = self
            .bonds
            .iter_mut()
            .find(|x| (x.a == a && x.b == b) || (x.a == b && x.b == a))
        {
            *bond = Bond {
                projection: false,
                z_order: bond.z_order,
                indicator: bond.indicator.clone(),
                cip_label: None,
                a,
                b,
                order,
                display: display.into(),
                stereo: None,
                stereo_atoms: vec![],
                double_position: bond.double_position,
                secondary_display: None,
                color: bond.color,
            };
        } else {
            self.bonds.push(Bond {
                projection: false,
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
                color: [0, 0, 0],
            });
        }
    }
    pub fn invalidate_chemistry(&mut self, affected: &[u64]) {
        if !self.atoms.iter().any(|a| affected.contains(&a.id)) {
            return;
        }
        crate::atom_labels::clear_computed(self);
        for atom in &mut self.atoms {
            atom.label_h = 0;
            if affected.contains(&atom.id)
                || atom
                    .stereo
                    .as_ref()
                    .is_some_and(|s| s.neighbors.iter().any(|n| affected.contains(n)))
            {
                atom.stereo = None;
            }
        }
        for bond in &mut self.bonds {
            if affected.contains(&bond.a)
                || affected.contains(&bond.b)
                || bond.stereo_atoms.iter().any(|a| affected.contains(a))
            {
                bond.stereo = None;
                bond.stereo_atoms.clear();
            }
        }
    }
    pub fn delete(&mut self, ids: &[u64]) {
        let ids = self.expand_abbreviation_selection(ids);
        let ids = ids.as_slice();
        self.expand_abbreviations(ids);
        self.invalidate_chemistry(ids);
        self.atoms.retain(|a| !ids.contains(&a.id));
        self.bonds
            .retain(|b| !ids.contains(&b.a) && !ids.contains(&b.b));
        self.annotations.retain(|a| !ids.contains(&a.id));
        self.arrows.retain(|a| !ids.contains(&a.id));
        self.graphics.retain(|a| !ids.contains(&a.id));
        crate::projection::prune_centroids(self);
        self.prune_groups();
        crate::reactions::prune(self);
    }
    pub fn translate(&mut self, ids: &[u64], dx: f32, dy: f32) {
        let ids = self.expand_abbreviation_selection(ids);
        let ids = ids.as_slice();
        for graphic in &mut self.graphics {
            if ids.contains(&graphic.id) {
                graphic.origin = graphic.origin.offset(dx, dy);
            }
        }
        for a in &mut self.atoms {
            if ids.contains(&a.id) {
                a.position = a.position.offset(dx, dy);
            }
        }
        for a in &mut self.annotations {
            if ids.contains(&a.id) {
                a.position = a.position.offset(dx, dy);
            }
        }
        for a in &mut self.arrows {
            if ids.contains(&a.id) {
                a.map_points(|p| p.offset(dx, dy));
            }
        }
        crate::projection::sync_centroids(self);
    }
    pub fn all_ids(&self) -> Vec<u64> {
        self.atoms
            .iter()
            .map(|a| a.id)
            .chain(self.annotations.iter().map(|a| a.id))
            .chain(self.arrows.iter().map(|a| a.id))
            .chain(self.graphics.iter().map(|a| a.id))
            .collect()
    }
    pub fn validate(&self) -> Result<(), String> {
        crate::projection::validate(self)?;
        self.drawing_style.validate()?;
        let mut picture_bytes = 0_usize;
        let mut picture_pixels = 0_u64;
        for picture in self.graphics.iter().filter_map(|g| g.picture.as_ref()) {
            picture_bytes = picture_bytes.saturating_add(picture.png().len());
            picture_pixels = picture_pixels
                .saturating_add(u64::from(picture.width()) * u64::from(picture.height()));
            if picture_bytes > 64 * 1024 * 1024 || picture_pixels > 64_000_000 {
                return Err("Drawing pictures exceed 64 MB or 64 million pixels".into());
            }
        }
        if !(1..=15).contains(&self.version) {
            return Err(format!("Unsupported document version {}", self.version));
        }
        if let Some(layout) = &self.page_layout {
            layout.validate()?;
        }
        self.validate_groups()?;
        crate::reactions::validate(self)?;
        self.validate_abbreviations()?;
        let mut ids = HashSet::new();
        for id in self.all_ids() {
            if id == 0 || id == u64::MAX || !ids.insert(id) {
                return Err("Duplicate or zero object ID".into());
            }
        }
        let atom_ids: HashSet<_> = self.atoms.iter().map(|a| a.id).collect();
        let mut neighbors: std::collections::HashMap<_, HashSet<_>> =
            std::collections::HashMap::new();
        for bond in &self.bonds {
            neighbors.entry(bond.a).or_default().insert(bond.b);
            neighbors.entry(bond.b).or_default().insert(bond.a);
        }
        let empty_neighbors = HashSet::new();
        for a in &self.atoms {
            a.display.validate()?;
            if a.cip_label
                .as_ref()
                .is_some_and(|s| !["R", "S", "r", "s"].contains(&s.as_str()))
            {
                return Err("Unsupported atom CIP label".into());
            }
            if a.radical_electrons > 2
                || a.marks.len() > 12
                || a.marks.iter().any(|m| {
                    !m.offset.x.is_finite()
                        || !m.offset.y.is_finite()
                        || !m.angle.is_finite()
                        || m.size_pt
                            .is_some_and(|n| !n.is_finite() || !(0.5..=96.).contains(&n))
                })
            {
                return Err("Invalid atom radical count or mark position".into());
            }
            if let Some(style) = &a.text_style {
                style.validate()?;
            }
            if !a.position.x.is_finite() || !a.position.y.is_finite() {
                return Err("Non-finite atom position".into());
            }
            if a.element.is_empty() || a.element.len() > 3 {
                return Err("Invalid element symbol".into());
            }
            if let Some(s) = &a.stereo {
                let actual = neighbors.get(&a.id).unwrap_or(&empty_neighbors);
                if !["cw", "ccw"].contains(&s.winding.as_str())
                    || actual != &s.neighbors.iter().copied().collect()
                    || actual.len() != s.neighbors.len()
                {
                    return Err("Invalid stereocenter neighbor mapping".into());
                }
            }
        }
        let mut pairs = HashSet::new();
        for b in &self.bonds {
            b.indicator.validate()?;
            if b.cip_label
                .as_ref()
                .is_some_and(|s| !["E", "Z"].contains(&s.as_str()))
            {
                return Err("Unsupported bond CIP label".into());
            }
            if b.a == b.b || !atom_ids.contains(&b.a) || !atom_ids.contains(&b.b) || b.order > 7 {
                return Err("Invalid bond endpoints or order".into());
            }
            if !pairs.insert((b.a.min(b.b), b.a.max(b.b))) {
                return Err("Duplicate bond".into());
            }
            b.validate_appearance()?;
            if b.stereo.is_some()
                && (b.stereo_atoms.len() != 2
                    || b.stereo_atoms.iter().any(|id| !atom_ids.contains(id)))
            {
                return Err("Invalid bond stereo references".into());
            }
        }
        for p in self
            .annotations
            .iter()
            .map(|a| a.position)
            .chain(self.arrows.iter().flat_map(|a| [a.start, a.end]))
        {
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err("Non-finite drawing position".into());
            }
        }
        for a in &self.annotations {
            a.format.validate(&a.text)?;
        }
        for graphic in &self.graphics {
            graphic.validate()?;
        }
        for arrow in &self.arrows {
            arrow.validate()?;
        }
        Ok(())
    }
    pub fn bounds(&self) -> (Point, Point) {
        match crate::scene::selection_bounds(self, &self.all_ids()) {
            Some((lo, hi)) => (lo.offset(-30., -30.), hi.offset(80., 30.)),
            None => (Point::new(-100., -75.), Point::new(100., 75.)),
        }
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Document>,
    redo: Vec<Document>,
}
impl History {
    pub fn commit(&mut self, before: Document, after: &Document) -> bool {
        self.commit_continuing(before, after, false)
    }
    /// A continuous gesture keeps its first undo snapshot while updating the drawing live.
    pub fn commit_continuing(
        &mut self,
        before: Document,
        after: &Document,
        continuing: bool,
    ) -> bool {
        if before == *after {
            return false;
        }
        if !continuing || self.undo.is_empty() {
            self.undo.push(before);
        }
        self.redo.clear();
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        true
    }
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(doc, prev));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(doc, next));
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleting_atom_removes_bonds_and_undo_restores_exact_document() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("O", Point::new(40.0, 0.0));
        doc.add_bond(a, b, 1, "plain");
        let original = doc.clone();
        let mut history = History::default();
        doc.delete(&[a]);
        history.commit(original.clone(), &doc);
        assert!(doc.bonds.is_empty());
        assert!(doc.validate().is_ok());
        assert!(history.undo(&mut doc));
        assert_eq!(doc, original);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.atoms.len(), 1);
    }
    #[test]
    fn rejects_dangling_bond_and_duplicate_ids() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        doc.bonds.push(Bond {
            projection: false,
            z_order: 0,
            indicator: Default::default(),
            cip_label: None,
            a,
            b: 50,
            order: 1,
            display: plain(),
            stereo: None,
            stereo_atoms: vec![],
            double_position: Default::default(),
            secondary_display: None,
            color: [0, 0, 0],
        });
        assert!(doc.validate().is_err());
        doc.bonds.clear();
        doc.atoms.push(doc.atoms[0].clone());
        assert!(doc.validate().is_err());
    }
}
