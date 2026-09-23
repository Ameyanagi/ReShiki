//! Bounded vector reconstruction for source diagrams that SMILES cannot lay out.
//! These are explicitly reviewed diagrams, not a claim of chemical perception.
use super::DrawingSettings;
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle, LinePattern},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
mod ligand;
pub use ligand::{Ligand, LigandKind};

pub const REVIEW_NOTE: &str = "Reconstructed as an editable diagram. Check bond orders, charges and coordination assignments before exporting molecular data.";

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
    #[serde(default)]
    pub arrows: Vec<Arrow>,
    #[serde(default)]
    pub captions: Vec<Caption>,
    #[serde(default)]
    pub abbreviations: Vec<Abbreviation>,
    #[serde(default)]
    pub ligands: Vec<Ligand>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arrow {
    pub start: Point,
    pub end: Point,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caption {
    pub text: String,
    pub position: Point,
    pub color: Option<[u8; 3]>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Abbreviation {
    pub label: String,
    pub anchor: usize,
    pub atoms: Vec<usize>,
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
    #[serde(default)]
    pub kind: Option<crate::attachments::Kind>,
    pub atoms: Vec<usize>,
    pub contact: Option<usize>,
    #[serde(default)]
    pub contact_style: Option<ContactStyle>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactStyle {
    Single,
    Dashed,
    Dative,
}
impl ContactStyle {
    fn parts(self) -> (u8, &'static str) {
        match self {
            Self::Single => (1, "plain"),
            Self::Dashed => (5, "dashed"),
            Self::Dative => (5, "plain"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Atom {
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub variable: Option<String>,
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
    #[serde(default)]
    pub ring_arc: bool,
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
        if (self.atoms.is_empty() && self.ligands.is_empty())
            || self.ligands.len() > 16
            || self.atoms.len() > 300
            || self.bonds.len() > 600
            || self.shapes.len() > 100
        {
            return Err("Use a diagram with 1–300 atoms, at most 600 bonds and 100 shapes".into());
        }
        let ligand_atoms: usize = self.ligands.iter().map(|l| l.kind.atom_count()).sum();
        if self.atoms.len() + ligand_atoms + self.centroids.len() > 300 {
            return Err("Defined ligands and attachment points exceed the 300 atom limit".into());
        }
        let ligand_bonds: usize = self
            .ligands
            .iter()
            .map(|l| l.kind.atom_count() - 1 + usize::from(l.contact.is_some()))
            .sum();
        let contacts = self
            .centroids
            .iter()
            .filter(|c| c.contact.is_some())
            .count();
        if self.bonds.len() + ligand_bonds + contacts > 600 {
            return Err("Defined ligands and contacts exceed the 600 bond limit".into());
        }
        for ligand in &self.ligands {
            ligand.validate(self.atoms.len())?;
        }
        let bounded = |x: f32| x.is_finite() && x.abs() <= 50.;
        for a in &self.atoms {
            if let Some(variable) = &a.variable
                && (a.element != "*"
                    || variable.is_empty()
                    || variable.chars().count() > 8
                    || !variable.chars().all(|c| c.is_alphanumeric()))
            {
                return Err(
                    "Use a wildcard atom for a variable label such as E; do not guess its element"
                        .into(),
                );
            }
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
            if b.ring_arc && (!matches!(b.order, 1 | 2 | 4) || b.display != "plain") {
                return Err(
                    "Inner ring curves require plain single, double or aromatic bonds".into(),
                );
            }
            if b.a >= self.atoms.len()
                || b.b >= self.atoms.len()
                || b.a == b.b
                || !(1..=7).contains(&b.order)
                || !["plain", "bold", "wedge", "hash", "hashed", "dashed", "wavy"]
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
        if self.arrows.len() > 16 || self.captions.len() > 64 || self.abbreviations.len() > 32 {
            return Err("Too many diagram arrows, captions or abbreviations".into());
        }
        for a in &self.arrows {
            if ![a.start.x, a.start.y, a.end.x, a.end.y]
                .into_iter()
                .all(bounded)
                || a.start.distance(a.end) < 0.5
            {
                return Err("Invalid diagram reaction arrow".into());
            }
        }
        for c in &self.captions {
            if c.text.trim().is_empty()
                || c.text.len() > 500
                || !bounded(c.position.x)
                || !bounded(c.position.y)
                || c.text.chars().any(|c| c.is_control() && c != '\n')
            {
                return Err("Invalid diagram caption".into());
            }
        }
        for a in &self.abbreviations {
            if a.atoms.is_empty()
                || a.atoms.len() > 100
                || !a.atoms.contains(&a.anchor)
                || a.atoms.iter().any(|i| *i >= self.atoms.len())
                || a.label.is_empty()
                || a.label.chars().count() > 32
            {
                return Err("Invalid diagram abbreviation".into());
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
            if (c.contact.is_none() && c.contact_style.is_some())
                || c.atoms.len() < 2
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
            if let Some(color) = a.color {
                atom.text_style.as_mut().ok_or("Missing atom style")?.color = color;
            }
            atom.display.variable = a.variable.clone();
        }
        for b in &self.bonds {
            doc.add_bond(b.a as u64 + 1, b.b as u64 + 1, b.order, &b.display);
            if let Some(bond) = doc.bonds.last_mut() {
                bond.ring_arc = b.ring_arc;
            }
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
            let id = if let Some(kind) = centroid.kind {
                crate::attachments::add(&mut doc, &members, kind)?
            } else {
                crate::projection::add_centroid(&mut doc, &members)?
            };
            if let Some(contact) = centroid.contact {
                // Keep ALL/ANY target semantics while matching the source's
                // contact appearance. Old proposals retain their defaults.
                let style = centroid
                    .contact_style
                    .unwrap_or(if centroid.kind.is_some() {
                        ContactStyle::Single
                    } else {
                        ContactStyle::Dashed
                    });
                let (order, display) = style.parts();
                doc.add_bond(id, contact as u64 + 1, order, display);
                if let Some(b) = doc.bonds.last_mut() {
                    b.z_order = -1;
                    b.color = settings.bond_color;
                }
            }
        }
        for ligand in &self.ligands {
            ligand.append(&mut doc, settings)?;
        }
        for a in &self.arrows {
            doc.arrows.push(crate::document::Arrow {
                id: doc.next_id(),
                start: point(a.start),
                end: point(a.end),
                kind: "forward".into(),
                control: None,
                style: None,
            });
        }
        for c in &self.captions {
            let mut format = settings.format.clone();
            if let Some(color) = c.color {
                format.style.color = color;
            }
            doc.annotations.push(crate::document::Annotation {
                id: doc.next_id(),
                position: point(c.position),
                text: c.text.clone(),
                format,
            });
        }
        for a in &self.abbreviations {
            doc.abbreviations.push(crate::abbreviations::Abbreviation {
                alignment: Default::default(),
                label: a.label.clone(),
                reverse_label: a.label.clone(),
                anchor: a.anchor as u64 + 1,
                members: a.atoms.iter().map(|i| *i as u64 + 1).collect(),
            });
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
    let color = json!({"anyOf":[{"type":"null"},{"type":"array","minItems":3,"maxItems":3,"items":{"type":"integer","minimum":0,"maximum":255}}],"description":"RGB label color from the source; null uses the drawing style."});
    json!({"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,
        "description":"Use explicit coordinates for coordination complexes, macrocycles, projected organometallics, and source diagrams whose arrangement SMILES layout cannot preserve. An entire reaction scheme can be one sketch: use arrows and captions and place all participants in the same coordinate system. Keep molecules and reactions empty when using sketch. For corresponding ligand/complex panels, reuse the same ligand coordinates translated horizontally, then add the metal and its contacts; do not fold or rotate the ligand around the metal. Place donor atoms around the metal as in the source. Keep ring sizes, labels and inner delocalization curves. For ordinary simple molecules use SMILES. All coordinates are in bond-length units, x right, y down, with a typical bond length of 1. Default to a flat 2D drawing with tilts empty. Only when the source actually shows perspective, build planar rings and circles and project them together using explicit tilts. Metal coordination alone is not a reason to tilt. Use centroids with kind multi_center for haptic contacts (all target atoms), kind variable for alternative attachment positions, and null only for nonchemical drawing anchors. Do not use loose lines for attachments. Atom element * is a dummy wildcard: set variable to E or another visible generic label instead of inventing an element. For tBu or similar groups include the full atom graph then use abbreviations to collapse it. Use ring_arc on consecutive ring bonds for partial inner curves, preserving their underlying bond orders. Captions use top-left positions. Reaction arrows require empty space of at least one bond length on either side. All indices are zero-based. Keep the original arrangement. This is a diagram requiring manual chemical review, not validated molecular data. Leave molecules and reactions empty.",
        "properties":{
            "ligands":{"type":"array","maxItems":16,"description":"For Cp and Cp* use these defined ligands instead of tracing ring atoms or drawing ellipses. Each creates a real aromatic five-member ring (charge -1), five hydrogens for Cp or five methyl groups for Cp*, an aromatic circle and a five-center attachment. Apply X tilt, Y tilt, then screen rotation. Do not duplicate the generated atoms in atoms/bonds/centroids/shapes. Metal charges remain as entered.","items":{"type":"object","additionalProperties":false,"properties":{"kind":{"type":"string","enum":["Cp","Cp*"]},"center":point,"x_degrees":{"type":"number","minimum":-85,"maximum":85},"y_degrees":{"type":"number","minimum":-85,"maximum":85},"rotation_degrees":{"type":"number","minimum":-360,"maximum":360,"description":"Screen rotation after the X/Y tilts; sets the projected ellipse direction."},"depth_bonds":{"type":"boolean","description":"Emphasize the foreground ring/substituent bonds without changing aromatic orders or assigning tetrahedral stereo."},"contact":{"anyOf":[{"type":"null"},{"type":"integer","minimum":0}],"description":"Index of the metal in atoms, or null for an unbound ligand."},"contact_style":{"anyOf":[{"type":"null"},{"type":"string","enum":["single","dashed","dative"]}],"description":"Match the source contact; null defaults to a solid single line. Must be null when contact is null."}},"required":["kind","center","x_degrees","y_degrees","rotation_degrees","depth_bonds","contact","contact_style"]}},
            "arrows":{"type":"array","maxItems":16,"items":{"type":"object","additionalProperties":false,"properties":{"start":point,"end":point},"required":["start","end"]}},
            "captions":{"type":"array","maxItems":64,"items":{"type":"object","additionalProperties":false,"properties":{"text":{"type":"string","maxLength":500},"position":point,"color":color},"required":["text","position","color"]}},
            "abbreviations":{"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,"properties":{"label":{"type":"string"},"anchor":{"type":"integer","minimum":0},"atoms":indices},"required":["label","anchor","atoms"]}},
            "tilts":{"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,"properties":{"atoms":indices,"shapes":indices,"x_degrees":{"type":"number","minimum":-85,"maximum":85},"y_degrees":{"type":"number","minimum":-85,"maximum":85},"depth_bonds":{"type":"boolean","description":"Bold the foreground single bonds according to their retained depth; preserves stereo wedges."}},"required":["atoms","shapes","x_degrees","y_degrees","depth_bonds"]}},
            "centroids":{"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,"properties":{"kind":{"anyOf":[{"type":"null"},{"type":"string","enum":["multi_center","variable"]}],"description":"multi_center means all atoms (eta bonding); variable means one of the listed positions; null is a nonchemical centroid."},"atoms":indices,"contact":{"anyOf":[{"type":"null"},{"type":"integer","minimum":0}],"description":"Optional metal/substituent index outside the target set. Creates a bond from the attachment point to this atom."},"contact_style":{"anyOf":[{"type":"null"},{"type":"string","enum":["single","dashed","dative"]}],"description":"single is a solid line, dashed is a dashed coordination contact (no arrowhead), dative is a donor-to-metal arrow. Match the source. Null preserves the default: single for typed attachments, dashed for legacy centroids. Must be null without a contact."}},"required":["atoms","contact","kind","contact_style"]}},
            "atoms":{"type":"array","maxItems":300,"items":{"type":"object","additionalProperties":false,"properties":{"color":color,"variable":{"anyOf":[{"type":"null"},{"type":"string","maxLength":8}],"description":"Only for element *. Visible variable label such as E; null otherwise."},"element":{"type":"string"},"x":number,"y":number,"charge":{"type":"integer","minimum":-8,"maximum":8},"isotope":{"type":"integer","minimum":0,"maximum":300},"hydrogens":{"type":"integer","minimum":0,"maximum":8}},"required":["element","x","y","charge","isotope","hydrogens","color","variable"]}},
            "bonds":{"type":"array","maxItems":600,"items":{"type":"object","additionalProperties":false,"properties":{"ring_arc":{"type":"boolean","description":"Replace the inner line along this ring edge by an inner curve; use on consecutive edges for the partial N-C-N delocalization convention. Does not change bond order."},"a":{"type":"integer","minimum":0,"description":"Zero-based index in atoms"},"b":{"type":"integer","minimum":0},"order":{"type":"integer","minimum":1,"maximum":7,"description":"1 single, 2 double, 3 triple, 4 aromatic, 5 dative (a donor to b acceptor), 6 quadruple, 7 partial"},"display":{"type":"string","enum":["plain","bold","wedge","hash","hashed","dashed","wavy"],"description":"Match the source: hash is a tapered hashed wedge; hashed has uniform width. Orders 1 supports plain/bold/wedge/hash/hashed/wavy; order 2 plain/bold/dashed/wavy; orders 5 and 7 plain/dashed; other orders plain."}},"required":["a","b","order","display","ring_arc"]}},
            "shapes":{"type":"array","maxItems":100,"items":{"type":"object","additionalProperties":false,"properties":{"kind":{"type":"string","enum":["line","ellipse"]},"start":point,"end":point,"dashed":{"type":"boolean"}},"required":["kind","start","end","dashed"]}}
        },"required":["atoms","bonds","shapes","tilts","centroids","arrows","captions","abbreviations","ligands"]}]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sandwich() -> Sketch {
        let mut atoms = vec![Atom {
            color: None,
            variable: None,
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
                    color: None,
                    variable: None,
                    element: "C".into(),
                    x: angle.cos(),
                    y: cy + angle.sin() * 0.65,
                    charge: 0,
                    isotope: 0,
                    hydrogens: 1,
                });
                bonds.push(Bond {
                    ring_arc: false,
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
            arrows: vec![],
            captions: vec![],
            abbreviations: vec![],
            ligands: vec![],
        }
    }
    #[test]
    fn flat_scheme_keeps_variables_colors_arrows_and_ring_curves() {
        let mut sketch = sandwich();
        sketch.shapes.clear();
        sketch.atoms[0].element = "Cu".into();
        sketch.atoms[0].color = Some([0, 0, 255]);
        sketch.atoms[1].element = "*".into();
        sketch.atoms[1].variable = Some("E".into());
        sketch.atoms[1].color = Some([220, 40, 40]);
        sketch.bonds[0].ring_arc = true;
        sketch.bonds[1].ring_arc = true;
        sketch.arrows.push(Arrow {
            start: Point::new(2., 0.),
            end: Point::new(5., 0.),
        });
        sketch.captions.push(Caption {
            text: "Cu(acac)₂".into(),
            position: Point::new(2., -0.8),
            color: None,
        });
        let doc = sketch.render(&Default::default()).unwrap();
        assert!(doc.atoms.iter().all(|a| a.depth == 0.));
        assert_eq!(doc.arrows.len(), 1);
        assert_eq!(doc.annotations[0].text, "Cu(acac)₂");
        assert_eq!(doc.atoms[1].element, "*");
        assert_eq!(doc.atoms[1].display.variable.as_deref(), Some("E"));
        assert_eq!(crate::ring_arcs::render(&doc).bonds.len(), 2);
        let svg = crate::scene::svg(&doc);
        assert!(svg.contains(">E</text>"));
        assert!(svg.contains("rgb(0,0,255)"));
        assert_eq!(
            serde_json::from_str::<Document>(&serde_json::to_string(&doc).unwrap()).unwrap(),
            doc
        );
        sketch.atoms[1].element = "O".into();
        assert!(sketch.validate().is_err());
    }

    #[test]
    fn native_ring_centroids_and_tilts_render_valid_contacts() -> Result<(), String> {
        for kind in [
            None,
            Some(crate::attachments::Kind::MultiCenter),
            Some(crate::attachments::Kind::Variable),
        ] {
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
                    kind,
                    atoms,
                    contact: Some(0),
                    contact_style: None,
                });
            }
            let doc = sketch.render(&Default::default())?;
            doc.validate()?;
            assert_eq!(doc.atoms.len(), 13);
            assert_eq!(doc.bonds.len(), 12);
            for centroid in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
                assert_eq!(centroid.centroid.len(), 5);
                let contact = doc
                    .bonds
                    .iter()
                    .find(|b| b.a == centroid.id || b.b == centroid.id)
                    .ok_or("Missing ring contact")?;
                assert_eq!(centroid.attachment, kind);
                let (order, display) = if kind.is_some() {
                    (1, "plain")
                } else {
                    (5, "dashed")
                };
                assert_eq!(contact.display, display);
                assert_eq!(contact.order, order);
                assert!(contact.a == 1 || contact.b == 1);
            }
            assert!(crate::assistant::canvas_tools::image(&doc).is_ok());
            assert!(
                crate::chemistry::document::prepare(&doc).is_err(),
                "No fabricated haptic molecular data"
            );
        }
        Ok(())
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
