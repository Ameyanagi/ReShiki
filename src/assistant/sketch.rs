//! Bounded vector reconstruction for source diagrams that SMILES cannot lay out.
//! These are explicitly reviewed diagrams, not a claim of chemical perception.
use super::DrawingSettings;
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle, LinePattern},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const REVIEW_NOTE: &str = "Reconstructed as an editable diagram. Check chemical assignments before exporting molecular data; ring-centre contacts use tracked centroids, not validated multicentre chemical bonds.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sketch {
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    pub shapes: Vec<Shape>,
    #[serde(default)]
    pub tilts: Vec<Tilt>,
    #[serde(default)]
    pub centroids: Vec<Centroid>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tilt {
    pub atoms: Vec<usize>,
    pub shapes: Vec<usize>,
    pub x_degrees: f32,
    pub y_degrees: f32,
    pub depth_bonds: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Centroid {
    pub atoms: Vec<usize>,
    pub contact: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Atom {
    pub element: String,
    pub x: f32,
    pub y: f32,
    pub charge: i32,
    pub isotope: u32,
    pub hydrogens: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bond {
    pub a: usize,
    pub b: usize,
    pub order: u8,
    pub display: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub kind: ShapeKind,
    pub start: Point,
    pub end: Point,
    pub dashed: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Line,
    Ellipse,
}

impl Sketch {
    pub fn validate(&self) -> Result<(), String> {
        if self.atoms.is_empty()
            || self.atoms.len() > 300
            || self.bonds.len() > 600
            || self.shapes.len() > 100
        {
            return Err("Use a diagram with 1–300 atoms, at most 600 bonds and 100 shapes".into());
        }
        let bounded = |x: f32| x.is_finite() && x.abs() <= 50.;
        for a in &self.atoms {
            if !bounded(a.x)
                || !bounded(a.y)
                || a.charge.unsigned_abs() > 8
                || a.isotope > 300
                || a.hydrogens > 8
                || a.element.is_empty()
                || a.element.len() > 3
                || (a.element != "*" && !a.element.chars().all(|c| c.is_ascii_alphabetic()))
                || crate::chemistry::smiles::parse(&format!("[{}]", a.element)).is_err()
            {
                return Err("Invalid diagram atom or position".into());
            }
        }
        let mut pairs = std::collections::HashSet::new();
        for b in &self.bonds {
            if b.a >= self.atoms.len()
                || b.b >= self.atoms.len()
                || b.a == b.b
                || !(1..=7).contains(&b.order)
                || !["plain", "bold", "wedge", "hashed", "dashed", "wavy"]
                    .contains(&b.display.as_str())
                || !pairs.insert((b.a.min(b.b), b.a.max(b.b)))
            {
                return Err("Invalid or duplicate diagram bond".into());
            }
            let a = self.atoms.get(b.a).ok_or("Unknown diagram atom")?;
            let c = self.atoms.get(b.b).ok_or("Unknown diagram atom")?;
            if (a.x - c.x).hypot(a.y - c.y) < 0.1 {
                return Err("Diagram bond endpoints overlap".into());
            }
        }
        for s in &self.shapes {
            if ![s.start.x, s.start.y, s.end.x, s.end.y]
                .into_iter()
                .all(bounded)
                || s.start.distance(s.end) < 0.05
                || matches!(s.kind, ShapeKind::Ellipse)
                    && (s.end.x <= s.start.x || s.end.y <= s.start.y)
            {
                return Err("Invalid diagram shape".into());
            }
        }
        if self.tilts.len() > 32 || self.centroids.len() > 32 {
            return Err("Too many projection controls".into());
        }
        for t in &self.tilts {
            if t.atoms.is_empty()
                || t.atoms.len() > 300
                || t.shapes.len() > 100
                || t.atoms.iter().any(|i| *i >= self.atoms.len())
                || t.shapes.iter().any(|i| *i >= self.shapes.len())
                || [t.x_degrees, t.y_degrees]
                    .iter()
                    .any(|d| !d.is_finite() || d.abs() > 85.)
            {
                return Err("Invalid 3D tilt".into());
            }
        }
        for c in &self.centroids {
            if c.atoms.len() < 2
                || c.atoms.len() > 300
                || c.atoms
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != c.atoms.len()
                || c.atoms
                    .iter()
                    .any(|i| self.atoms.get(*i).is_none_or(|a| a.element == "*"))
                || c.contact
                    .is_some_and(|i| i >= self.atoms.len() || c.atoms.contains(&i))
            {
                return Err("Invalid ring centroid or contact".into());
            }
        }
        Ok(())
    }
    pub fn render(&self, settings: &DrawingSettings) -> Result<Document, String> {
        self.validate()?;
        if !settings.bond_length.is_finite() || !(4.0..=600.).contains(&settings.bond_length) {
            return Err("Invalid diagram bond length".into());
        }
        let mut doc = Document {
            drawing_style: settings.drawing_style.clone(),
            atom_labels: settings.labels.clone(),
            ..Default::default()
        };
        let point = |p: Point| Point::new(p.x * settings.bond_length, p.y * settings.bond_length);
        for a in &self.atoms {
            let id = doc.add_atom(&a.element, point(Point::new(a.x, a.y)));
            let atom = doc.atom_mut(id).ok_or("Missing diagram atom")?;
            atom.charge = a.charge;
            atom.isotope = a.isotope;
            atom.explicit_h = a.hydrogens;
            atom.label_h = a.hydrogens;
            atom.no_implicit = true;
            atom.text_style = Some(settings.format.style.clone());
        }
        for b in &self.bonds {
            doc.add_bond(b.a as u64 + 1, b.b as u64 + 1, b.order, &b.display);
        }
        for b in &mut doc.bonds {
            b.color = settings.bond_color;
            b.projection = b.display == "bold";
        }
        for s in &self.shapes {
            doc.graphics.push(Graphic::dragged(
                doc.next_id(),
                match s.kind {
                    ShapeKind::Line => GraphicKind::Line,
                    ShapeKind::Ellipse => GraphicKind::Ellipse,
                },
                point(s.start),
                point(s.end),
                GraphicStyle {
                    stroke: settings.bond_color,
                    pattern: if s.dashed {
                        LinePattern::Dashed
                    } else {
                        LinePattern::Solid
                    },
                    ..Default::default()
                },
                Default::default(),
                false,
            ));
        }
        for tilt in &self.tilts {
            let mut ids: Vec<_> = tilt.atoms.iter().map(|i| *i as u64 + 1).collect();
            ids.extend(
                tilt.shapes
                    .iter()
                    .filter_map(|i| doc.graphics.get(*i).map(|g| g.id)),
            );
            crate::projection::tilt(&mut doc, &ids, tilt.x_degrees, true);
            crate::projection::tilt(&mut doc, &ids, tilt.y_degrees, false);
            if tilt.depth_bonds {
                crate::projection::depth_bonds(&mut doc, &ids);
            }
        }
        for centroid in &self.centroids {
            let members: Vec<_> = centroid.atoms.iter().map(|i| *i as u64 + 1).collect();
            let id = crate::projection::add_centroid(&mut doc, &members)?;
            if let Some(contact) = centroid.contact {
                doc.add_bond(id, contact as u64 + 1, 5, "dashed");
                if let Some(b) = doc.bonds.last_mut() {
                    b.z_order = -1;
                }
            }
        }
        doc.groups.push(crate::grouping::Group {
            id: doc.next_id(),
            members: doc.all_ids(),
            integral: true,
        });
        doc.validate()?;
        Ok(doc)
    }
}

pub fn schema() -> Value {
    let number = json!({"type":"number","minimum":-50,"maximum":50});
    let point = json!({"type":"object","additionalProperties":false,"properties":{"x":number,"y":number},"required":["x","y"]});
    let indices = json!({"type":"array","maxItems":300,"items":{"type":"integer","minimum":0}});
    json!({"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,
        "description":"Use ONLY when SMILES cannot preserve the source projection (e.g. a metallocene sandwich). Return null for ordinary molecules/reactions. All coordinates are in bond-length units, x right, y down, with a typical bond length of 1. Build regular planar rings and circles, then use tilts to project them together; never hand-squash rings that can be tilted. Use centroids for ring-centre contacts, not loose lines. Atom element * is a dummy wildcard. All indices are zero-based. Keep the original arrangement. This is a diagram requiring manual chemical review, not validated molecular data. Leave molecules and reactions empty.",
        "properties":{
            "tilts":{"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,"properties":{"atoms":indices,"shapes":indices,"x_degrees":{"type":"number","minimum":-85,"maximum":85},"y_degrees":{"type":"number","minimum":-85,"maximum":85},"depth_bonds":{"type":"boolean","description":"Bold the foreground single bonds according to their retained depth; preserves stereo wedges."}},"required":["atoms","shapes","x_degrees","y_degrees","depth_bonds"]}},
            "centroids":{"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,"properties":{"atoms":indices,"contact":{"anyOf":[{"type":"null"},{"type":"integer","minimum":0}],"description":"Optional metal atom index. Creates a dashed nonchemical contact to the tracked centroid."}},"required":["atoms","contact"]}},
            "atoms":{"type":"array","maxItems":300,"items":{"type":"object","additionalProperties":false,"properties":{"element":{"type":"string"},"x":number,"y":number,"charge":{"type":"integer","minimum":-8,"maximum":8},"isotope":{"type":"integer","minimum":0,"maximum":300},"hydrogens":{"type":"integer","minimum":0,"maximum":8}},"required":["element","x","y","charge","isotope","hydrogens"]}},
            "bonds":{"type":"array","maxItems":600,"items":{"type":"object","additionalProperties":false,"properties":{"a":{"type":"integer","minimum":0,"description":"Zero-based index in atoms"},"b":{"type":"integer","minimum":0},"order":{"type":"integer","minimum":1,"maximum":7,"description":"1 single, 2 double, 3 triple, 4 aromatic, 5 dative, 6 quadruple, 7 partial"},"display":{"type":"string","enum":["plain","bold","wedge","hashed","dashed","wavy"]}},"required":["a","b","order","display"]}},
            "shapes":{"type":"array","maxItems":100,"items":{"type":"object","additionalProperties":false,"properties":{"kind":{"type":"string","enum":["line","ellipse"]},"start":point,"end":point,"dashed":{"type":"boolean"}},"required":["kind","start","end","dashed"]}}
        },"required":["atoms","bonds","shapes","tilts","centroids"]}]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sandwich() -> Sketch {
        let mut atoms = vec![Atom {
            element: "Fe".into(),
            x: 0.,
            y: 0.,
            charge: 0,
            isotope: 0,
            hydrogens: 0,
        }];
        let mut bonds = Vec::new();
        let mut shapes = Vec::new();
        for cy in [-1.5, 1.5] {
            let first = atoms.len();
            for i in 0..5 {
                let angle = (-90. + i as f32 * 72.).to_radians();
                atoms.push(Atom {
                    element: "C".into(),
                    x: angle.cos(),
                    y: cy + angle.sin() * 0.65,
                    charge: 0,
                    isotope: 0,
                    hydrogens: 1,
                });
                bonds.push(Bond {
                    a: first + i,
                    b: first + (i + 1) % 5,
                    order: 1,
                    display: if i == 2 { "bold" } else { "plain" }.into(),
                });
            }
            shapes.push(Shape {
                kind: ShapeKind::Ellipse,
                start: Point::new(-0.6, cy - 0.35),
                end: Point::new(0.6, cy + 0.35),
                dashed: false,
            });
            shapes.push(Shape {
                kind: ShapeKind::Line,
                start: Point::new(0., cy),
                end: Point::new(0., cy.signum() * 0.35),
                dashed: true,
            });
        }
        Sketch {
            atoms,
            bonds,
            shapes,
            tilts: vec![],
            centroids: vec![],
        }
    }
    #[test]
    fn native_ring_centroids_and_tilts_render_valid_contacts() {
        let mut sketch = sandwich();
        sketch
            .shapes
            .retain(|s| matches!(s.kind, ShapeKind::Ellipse));
        for (index, atoms) in [
            (0, (1..6).collect::<Vec<_>>()),
            (1, (6..11).collect::<Vec<_>>()),
        ] {
            sketch.tilts.push(Tilt {
                atoms: atoms.clone(),
                shapes: vec![index],
                x_degrees: 30.,
                y_degrees: 0.,
                depth_bonds: true,
            });
            sketch.centroids.push(Centroid {
                atoms,
                contact: Some(0),
            });
        }
        let doc = sketch.render(&Default::default()).unwrap();
        doc.validate().unwrap();
        assert_eq!(doc.atoms.len(), 13);
        assert_eq!(doc.bonds.len(), 12);
        for centroid in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            assert_eq!(centroid.centroid.len(), 5);
            let contact = doc
                .bonds
                .iter()
                .find(|b| b.a == centroid.id || b.b == centroid.id)
                .unwrap();
            assert_eq!(contact.display, "dashed");
            assert_eq!(contact.order, 5);
            assert!(contact.a == 1 || contact.b == 1);
        }
        assert!(crate::assistant::canvas_tools::image(&doc).is_ok());
        assert!(
            crate::chemistry::document::prepare(&doc).is_err(),
            "No fabricated haptic molecular data"
        );
    }

    #[test]
    fn sandwich_stays_editable_grouped_and_requires_chemical_review() {
        let sketch = sandwich();
        let doc = sketch.render(&Default::default()).unwrap();
        assert_eq!(doc.atoms.len(), 11);
        assert_eq!(doc.bonds.len(), 10);
        assert_eq!(doc.graphics.len(), 4);
        assert!(
            doc.bonds.iter().all(|b| b.a != 1 && b.b != 1),
            "No invented Fe-carbon sigma bonds"
        );
        assert!(doc.graphics.iter().all(|g| g.picture.is_none()));
        let targets = super::super::review::targets(&doc);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, "diagram");
        let moved = super::super::review::apply(
            &doc,
            &[super::super::review::Edit::Move {
                target: targets[0].name.clone(),
                dx_pt: 18.,
                dy_pt: 0.,
            }],
            false,
        )
        .unwrap();
        let dx = moved.atoms[0].position.x - doc.atoms[0].position.x;
        assert!(dx > 0.);
        assert!((moved.graphics[0].origin.x - doc.graphics[0].origin.x - dx).abs() < 0.001);
        let mut straightened = doc.clone();
        assert_eq!(
            super::super::composition::straighten_all(&mut straightened),
            0
        );
        assert_eq!(doc, straightened);
        let roundtrip: Document =
            serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(doc, roundtrip);
        assert!(
            super::super::review::quality(&doc, &Default::default()).contains(&REVIEW_NOTE.into())
        );
        assert!(
            super::super::canvas_tools::image(&doc)
                .unwrap()
                .starts_with(b"\x89PNG")
        );
    }
    #[test]
    fn malformed_diagrams_are_rejected_without_panics() {
        let mut s = sandwich();
        s.atoms[0].charge = i32::MIN;
        assert!(s.validate().is_err());
        s = sandwich();
        s.atoms[0].x = f32::NAN;
        assert!(s.validate().is_err());
        s = sandwich();
        s.bonds[0].b = usize::MAX;
        assert!(s.validate().is_err());
        s = sandwich();
        s.bonds.push(s.bonds[0].clone());
        assert!(s.validate().is_err());
        s = sandwich();
        s.shapes[0].end = s.shapes[0].start;
        assert!(s.validate().is_err());
        let proposal = super::super::Proposal {
            sketch: Some(sandwich()),
            molecules: vec![super::super::Molecule {
                smiles: "O".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(proposal.validate().is_err());
    }
}
