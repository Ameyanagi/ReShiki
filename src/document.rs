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
    pub id: u64,
    pub element: String,
    pub position: Point,
    #[serde(default)]
    pub charge: i32,
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
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    pub a: u64,
    pub b: u64,
    /// 1, 2, 3 = bond order; 4 = aromatic.
    pub order: u8,
    #[serde(default = "plain")]
    pub display: String,
    #[serde(default)]
    pub stereo: Option<String>,
    #[serde(default)]
    pub stereo_atoms: Vec<u64>,
}
fn plain() -> String {
    "plain".into()
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: u64,
    pub position: Point,
    pub text: String,
}
impl Annotation {
    pub fn size(&self) -> (f32, f32) {
        (
            self.text
                .lines()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as f32
                * 7.0,
            self.text.lines().count().max(1) as f32 * 16.0,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arrow {
    pub id: u64,
    pub start: Point,
    pub end: Point,
    #[serde(default = "forward")]
    pub kind: String,
}
fn forward() -> String {
    "forward".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub arrows: Vec<Arrow>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 2,
            atoms: vec![],
            bonds: vec![],
            annotations: vec![],
            arrows: vec![],
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
            .max()
            .unwrap_or(0)
            + 1
    }
    pub fn add_atom(&mut self, element: &str, position: Point) -> u64 {
        let id = self.next_id();
        self.atoms.push(Atom {
            id,
            element: element.into(),
            position,
            charge: 0,
            isotope: 0,
            explicit_h: 0,
            no_implicit: false,
            aromatic: false,
            stereo: None,
            map_num: 0,
            label_h: 0,
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
            .filter(|a| a.position.distance(point) < radius)
            .min_by(|a, b| {
                a.position
                    .distance(point)
                    .total_cmp(&b.position.distance(point))
            })
            .map(|a| a.id)
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
                a,
                b,
                order,
                display: display.into(),
                stereo: None,
                stereo_atoms: vec![],
            };
        } else {
            self.bonds.push(Bond {
                a,
                b,
                order,
                display: display.into(),
                stereo: None,
                stereo_atoms: vec![],
            });
        }
    }
    pub fn invalidate_chemistry(&mut self, affected: &[u64]) {
        if !self.atoms.iter().any(|a| affected.contains(&a.id)) {
            return;
        }
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
        self.invalidate_chemistry(ids);
        self.atoms.retain(|a| !ids.contains(&a.id));
        self.bonds
            .retain(|b| !ids.contains(&b.a) && !ids.contains(&b.b));
        self.annotations.retain(|a| !ids.contains(&a.id));
        self.arrows.retain(|a| !ids.contains(&a.id));
    }
    pub fn translate(&mut self, ids: &[u64], dx: f32, dy: f32) {
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
                a.start = a.start.offset(dx, dy);
                a.end = a.end.offset(dx, dy);
            }
        }
    }
    pub fn all_ids(&self) -> Vec<u64> {
        self.atoms
            .iter()
            .map(|a| a.id)
            .chain(self.annotations.iter().map(|a| a.id))
            .chain(self.arrows.iter().map(|a| a.id))
            .collect()
    }
    pub fn validate(&self) -> Result<(), String> {
        if ![1, 2].contains(&self.version) {
            return Err(format!("Unsupported document version {}", self.version));
        }
        let mut ids = HashSet::new();
        for id in self.all_ids() {
            if id == 0 || id == u64::MAX || !ids.insert(id) {
                return Err("Duplicate or zero object ID".into());
            }
        }
        for a in &self.atoms {
            if !a.position.x.is_finite() || !a.position.y.is_finite() {
                return Err("Non-finite atom position".into());
            }
            if a.element.is_empty() || a.element.len() > 3 {
                return Err("Invalid element symbol".into());
            }
            if let Some(s) = &a.stereo {
                let actual: HashSet<_> = self
                    .bonds
                    .iter()
                    .filter_map(|b| {
                        if b.a == a.id {
                            Some(b.b)
                        } else if b.b == a.id {
                            Some(b.a)
                        } else {
                            None
                        }
                    })
                    .collect();
                if !["cw", "ccw"].contains(&s.winding.as_str())
                    || actual != s.neighbors.iter().copied().collect()
                    || actual.len() != s.neighbors.len()
                {
                    return Err("Invalid stereocenter neighbor mapping".into());
                }
            }
        }
        let mut pairs = HashSet::new();
        for b in &self.bonds {
            if b.a == b.b
                || self.atom(b.a).is_none()
                || self.atom(b.b).is_none()
                || !(1..=4).contains(&b.order)
            {
                return Err("Invalid bond endpoints or order".into());
            }
            if !pairs.insert((b.a.min(b.b), b.a.max(b.b))) {
                return Err("Duplicate bond".into());
            }
            if !["plain", "wedge", "hash", "wavy"].contains(&b.display.as_str()) {
                return Err("Unsupported bond display".into());
            }
            if b.stereo.is_some()
                && (b.stereo_atoms.len() != 2
                    || b.stereo_atoms.iter().any(|id| self.atom(*id).is_none()))
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
        if self.arrows.iter().any(|a| {
            !["forward", "equilibrium", "resonance", "retro", "curved"].contains(&a.kind.as_str())
        }) {
            return Err("Unsupported arrow style".into());
        }
        Ok(())
    }
    pub fn bounds(&self) -> (Point, Point) {
        let points: Vec<_> = self
            .atoms
            .iter()
            .map(|a| a.position)
            .chain(self.annotations.iter().flat_map(|a| {
                [
                    a.position,
                    a.position.offset(
                        a.text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 9.0,
                        a.text.lines().count().max(1) as f32 * 16.0,
                    ),
                ]
            }))
            .chain(self.arrows.iter().flat_map(|a| {
                let mid = Point::new((a.start.x + a.end.x) / 2.0, (a.start.y + a.end.y) / 2.0);
                let control = if a.kind == "curved" {
                    mid.offset(-(a.end.y - a.start.y) * 0.5, (a.end.x - a.start.x) * 0.5)
                } else {
                    mid
                };
                [a.start, a.end, control]
            }))
            .collect();
        if points.is_empty() {
            return (Point::new(-100.0, -75.0), Point::new(100.0, 75.0));
        }
        let mut lo = points[0];
        let mut hi = lo;
        for p in points {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        (lo.offset(-30.0, -30.0), hi.offset(80.0, 30.0))
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Document>,
    redo: Vec<Document>,
}
impl History {
    pub fn commit(&mut self, before: Document, after: &Document) -> bool {
        if before == *after {
            return false;
        }
        self.undo.push(before);
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
            a,
            b: 50,
            order: 1,
            display: plain(),
            stereo: None,
            stereo_atoms: vec![],
        });
        assert!(doc.validate().is_err());
        doc.bonds.clear();
        doc.atoms.push(doc.atoms[0].clone());
        assert!(doc.validate().is_err());
    }
}
