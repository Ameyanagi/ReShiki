//! Haworth drawing geometry and chemically defined carbohydrate templates.
//! Perspective edges are presentation, separate from atom stereochemistry.
use crate::{
    document::{AtomStereo, Document, Point},
    templates::{Anchor, Template},
};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    Five,
    Six,
}

impl Ring {
    /// Carbon outlines for the ring tool; oxygen scaffolds for carbohydrates.
    /// The long front edge and foreshortened sides deliberately differ in length.
    pub fn document(self, length: f32, oxygen: bool) -> Document {
        let mut doc = Document::default();
        if !length.is_finite() || length <= 0. {
            return doc;
        }
        let points: &[(f32, f32)] = match self {
            Self::Five => &[(1., 0.), (0.65, 0.5), (-0.65, 0.5), (-1., 0.), (0., -0.6)],
            Self::Six => &[
                (1., 0.),
                (0.5, 0.5),
                (-0.5, 0.5),
                (-1., 0.),
                (-0.5, -0.5),
                (0.5, -0.5),
            ],
        };
        let mut ids = Vec::new();
        for (i, &(x, y)) in points.iter().enumerate() {
            let element = if oxygen && i + 1 == points.len() {
                "O"
            } else {
                "C"
            };
            let id = doc.add_atom(element, Point::new(x * length, y * length));
            if let Some(atom) = doc.atom_mut(id) {
                atom.depth = y * length * 3_f32.sqrt();
            }
            ids.push(id);
        }
        for (i, (&a, &b)) in ids
            .iter()
            .zip(ids.iter().cycle().skip(1))
            .take(ids.len())
            .enumerate()
        {
            let (a, b, display) = match i {
                0 => (a, b, "wedge"),
                1 => (a, b, "bold"),
                2 => (b, a, "wedge"),
                _ => (a, b, "plain"),
            };
            doc.add_bond(a, b, 1, display);
            if let Some(bond) = doc.bonds.last_mut() {
                bond.projection = true;
            }
        }
        doc
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sugar {
    Glucose,
    Galactose,
    Mannose,
    Ribose,
    Fructose,
}
impl Sugar {
    fn ring(self) -> Ring {
        match self {
            Self::Glucose | Self::Galactose | Self::Mannose => Ring::Six,
            Self::Ribose | Self::Fructose => Ring::Five,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Glucose => "glucose",
            Self::Galactose => "galactose",
            Self::Mannose => "mannose",
            Self::Ribose => "ribose",
            Self::Fructose => "fructose",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Anomer {
    Alpha,
    Beta,
}

fn hydroxymethyl(
    doc: &mut Document,
    position: Point,
    length: f32,
    depth: f32,
) -> Result<u64, String> {
    let carbon = doc.add_atom("C", position);
    let oxygen = doc.add_atom("O", position.offset(-length * 0.8, -length * 0.6));
    doc.add_bond(carbon, oxygen, 1, "plain");
    for id in [carbon, oxygen] {
        if let Some(atom) = doc.atom_mut(id) {
            atom.depth = depth;
        }
    }
    doc.contract(&[carbon, oxygen], "CH2OH", "HOCH2")?;
    Ok(carbon)
}

/// Ring atoms are ordered from the anomeric carbon around the front to oxygen.
/// α/β affects only the anomeric substituents; D configuration is fixed.
pub fn sugar_document(sugar: Sugar, anomer: Anomer, length: f32) -> Result<Document, String> {
    if !length.is_finite() || !(1. ..=10_000.).contains(&length) {
        return Err("Haworth bond length must be finite and between 1 and 10000".into());
    }
    // Fructose's inward OH needs more clearance from the sloping back edge.
    // Keep substituent bond lengths unchanged while opening the ring outline.
    let ring_scale = if matches!(sugar, Sugar::Fructose) {
        2.5
    } else {
        1.8
    };
    let mut doc = sugar.ring().document(length * ring_scale, true);
    let ring: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let carbons = ring.len().saturating_sub(1);
    let mut stereo = Vec::new();
    for (i, &id) in ring.iter().take(carbons).enumerate() {
        let up = match (sugar, i) {
            (_, 0) => anomer == Anomer::Beta,
            (Sugar::Glucose | Sugar::Galactose | Sugar::Mannose, 1) => {
                matches!(sugar, Sugar::Mannose)
            }
            (Sugar::Glucose | Sugar::Galactose | Sugar::Mannose, 2) => true,
            (Sugar::Glucose | Sugar::Galactose | Sugar::Mannose, 3) => {
                matches!(sugar, Sugar::Galactose)
            }
            (Sugar::Ribose, 1 | 2) | (Sugar::Fructose, 2) => false,
            _ => true,
        };
        let atom = doc.atom(id).ok_or("Missing Haworth ring atom")?;
        let origin = atom.position;
        let depth = atom.depth;
        let end = origin.offset(0., if up { -length } else { length });
        let branch = if i + 1 == carbons {
            let carbon = hydroxymethyl(&mut doc, end, length, depth)?;
            if sugar.ring() == Ring::Five
                && let Some(group) = doc.abbreviations.iter_mut().find(|g| g.anchor == carbon)
            {
                group.alignment = crate::abbreviations::LabelAlignment::Right;
            }
            carbon
        } else {
            let oxygen = doc.add_atom("O", end);
            if let Some(atom) = doc.atom_mut(oxygen) {
                atom.label_h = 1;
                if sugar.ring() == Ring::Six && i == 3 || i > 0 && up && origin.x > 0. {
                    atom.display.hydrogen_position = crate::atom_labels::HydrogenPosition::Left;
                }
            }
            oxygen
        };
        if let Some(atom) = doc.atom_mut(branch) {
            atom.depth = depth;
        }
        doc.add_bond(id, branch, 1, "plain");
        let previous = ring
            .get(if i == 0 { ring.len() - 1 } else { i - 1 })
            .copied()
            .ok_or("Missing previous ring atom")?;
        let next = ring.get(i + 1).copied().ok_or("Missing next ring atom")?;
        let mut neighbors = vec![previous, next, branch];
        if matches!(sugar, Sugar::Fructose) && i == 0 {
            let extra = hydroxymethyl(
                &mut doc,
                origin.offset(0., if up { length } else { -length }),
                length,
                depth,
            )?;
            if let Some(atom) = doc.atom_mut(extra) {
                atom.depth = depth;
            }
            doc.add_bond(id, extra, 1, "plain");
            neighbors.push(extra);
        }
        stereo.push((
            id,
            AtomStereo {
                // With [previous, next, substituent] around the clockwise drawn
                // ring, above-plane substituents have counterclockwise winding.
                winding: if up { "ccw" } else { "cw" }.into(),
                neighbors,
            },
        ));
    }
    // Adding a neighboring bond invalidates cached stereo; install assignments
    // only after the graph is complete.
    for (id, assignment) in stereo {
        doc.atom_mut(id).ok_or("Missing stereocenter")?.stereo = Some(assignment);
    }
    for atom in &mut doc.atoms {
        if atom.element == "O"
            && doc
                .bonds
                .iter()
                .filter(|b| b.a == atom.id || b.b == atom.id)
                .count()
                == 1
        {
            atom.label_h = 1;
        }
    }
    doc.validate()?;
    Ok(doc)
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    sugar: Sugar,
    anomer: Anomer,
    smiles: String,
}

pub(crate) fn templates() -> Result<Vec<Template>, String> {
    let entries: Vec<Entry> = serde_json::from_str(include_str!("../assets/haworth-sugars.json"))
        .map_err(|e| format!("Haworth templates could not be read: {e}"))?;
    let mut result = Vec::new();
    for entry in entries {
        let ring = entry.sugar.ring();
        let anomer = if entry.anomer == Anomer::Alpha {
            "alpha"
        } else {
            "beta"
        };
        result.push(Template {
            id: String::new(),
            group: "Carbohydrates".into(),
            name: format!("{} · Haworth", entry.name),
            smiles: entry.smiles,
            keywords: vec!["sugar".into(), "Haworth".into(), entry.sugar.name().into(), anomer.into(), if ring == Ring::Six { "pyranose" } else { "furanose" }.into()],
            note: "Haworth projection with defined D-sugar stereochemistry. CH2OH contains real atoms. Use native/figure output to retain perspective; cleanup creates a conventional stereochemical drawing.".into(),
            document: sugar_document(entry.sugar, entry.anomer, crate::style::DEFAULT.bond_length_world)?,
            anchor: Anchor::Auto,
        });
    }
    for (ring, name, smiles) in [
        (Ring::Five, "Furanose scaffold", "C1CCOC1"),
        (Ring::Six, "Pyranose scaffold", "C1CCOCC1"),
    ] {
        result.push(Template {
            id: String::new(),
            group: "Carbohydrates".into(),
            name: format!("{name} · Haworth"),
            smiles: smiles.into(),
            keywords: vec!["Haworth".into(), "sugar".into(), "scaffold".into()],
            note: "Oxygen-containing Haworth ring scaffold. Add substituents to define a sugar; no stereochemistry is assigned to this blank scaffold.".into(),
            document: ring.document(crate::style::DEFAULT.bond_length_world, true),
            anchor: Anchor::Auto,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sugar_labels_do_not_touch_unrelated_ring_edges() -> anyhow::Result<()> {
        use anyhow::Context;
        for template in templates().map_err(anyhow::Error::msg)? {
            let doc = &template.document;
            let overlaps = crate::assistant::review::internal_overlaps(doc);
            assert!(overlaps.is_empty(), "{}: {overlaps:?}", template.name);
            for atom in &doc.atoms {
                let Some((lo, hi)) = crate::scene::atom_label_bounds(atom, doc) else {
                    continue;
                };
                for bond in doc
                    .bonds
                    .iter()
                    .filter(|b| b.projection && b.a != atom.id && b.b != atom.id)
                {
                    let a = doc.atom(bond.a).context("Ring endpoint")?.position;
                    let b = doc.atom(bond.b).context("Ring endpoint")?.position;
                    if a.x.max(b.x) < lo.x
                        || a.x.min(b.x) > hi.x
                        || a.y.max(b.y) < lo.y
                        || a.y.min(b.y) > hi.y
                    {
                        continue;
                    }
                    let sides = [lo, Point::new(lo.x, hi.y), hi, Point::new(hi.x, lo.y)]
                        .map(|p| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x));
                    assert!(
                        sides.iter().all(|v| *v > 0.) || sides.iter().all(|v| *v < 0.),
                        "{} label {} intersects ring edge {}–{}",
                        template.name,
                        atom.id,
                        bond.a,
                        bond.b
                    );
                }
            }
        }
        Ok(())
    }
}
