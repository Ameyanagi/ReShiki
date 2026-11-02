//! Haworth drawing geometry and chemically defined carbohydrate templates.
//! Perspective edges are presentation, separate from atom stereochemistry.
use crate::{
    document::{AtomStereo, Document, Point},
    templates::{Anchor, Template},
};
use serde::Deserialize;
pub mod interchange;

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
    Deoxyribose,
    Xylose,
    Fucose,
    Rhamnose,
    #[serde(rename = "glcnac")]
    GlcNAc,
    #[serde(rename = "galnac")]
    GalNAc,
    GlucuronicAcid,
}
impl Sugar {
    fn ring(self) -> Ring {
        match self {
            Self::Ribose | Self::Deoxyribose | Self::Fructose => Ring::Five,
            _ => Ring::Six,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Glucose => "glucose",
            Self::Galactose => "galactose",
            Self::Mannose => "mannose",
            Self::Ribose => "ribose",
            Self::Fructose => "fructose",
            Self::Deoxyribose => "deoxyribose",
            Self::Xylose => "xylose",
            Self::Fucose => "fucose",
            Self::Rhamnose => "rhamnose",
            Self::GlcNAc => "GlcNAc",
            Self::GalNAc => "GalNAc",
            Self::GlucuronicAcid => "glucuronic acid",
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

/// Presentation options do not change a sugar's molecular identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HaworthOptions {
    /// Draw every ring-carbon hydrogen, including both faces of an achiral CH2.
    pub ring_hydrogens: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Substituent {
    Hydroxyl,
    Hydroxymethyl,
    Methyl,
    Carboxyl,
    Acetamido,
    Hydrogen,
}

/// One pair of opposite substituents at a ring carbon. Clockwise ring order
/// fixes the stored winding from the face of the first substituent. Two
/// hydrogens describe a CH2 carbon and never create a stereocenter.
#[derive(Debug, Clone, Copy)]
struct CarbonPattern {
    up: bool,
    branch: Substituent,
    opposite: Substituent,
}

fn pattern(sugar: Sugar, anomer: Anomer) -> Vec<CarbonPattern> {
    use Substituent::*;
    let l_sugar = matches!(sugar, Sugar::Fucose | Sugar::Rhamnose);
    let anomer_up = (anomer == Anomer::Beta) != l_sugar;
    let (faces, terminal): (&[bool], Substituent) = match sugar {
        Sugar::Glucose | Sugar::GlcNAc | Sugar::GlucuronicAcid => (
            &[false, true, false, true],
            if matches!(sugar, Sugar::GlucuronicAcid) {
                Carboxyl
            } else {
                Hydroxymethyl
            },
        ),
        Sugar::Galactose | Sugar::GalNAc => (&[false, true, true, true], Hydroxymethyl),
        Sugar::Mannose => (&[true, true, false, true], Hydroxymethyl),
        Sugar::Ribose | Sugar::Deoxyribose => (&[false, false, true], Hydroxymethyl),
        Sugar::Fructose => (&[true, false, true], Hydroxymethyl),
        Sugar::Xylose => (&[false, true, false, true], Hydrogen),
        Sugar::Fucose => (&[true, false, false, false], Methyl),
        Sugar::Rhamnose => (&[false, false, true, false], Methyl),
    };
    std::iter::once(anomer_up)
        .chain(faces.iter().copied())
        .enumerate()
        .map(|(i, up)| {
            let branch = if i == faces.len() {
                terminal
            } else if i == 1 && matches!(sugar, Sugar::Deoxyribose) {
                Hydrogen
            } else if i == 1 && matches!(sugar, Sugar::GlcNAc | Sugar::GalNAc) {
                Acetamido
            } else {
                Hydroxyl
            };
            let opposite = if i == 0 && matches!(sugar, Sugar::Fructose) {
                Hydroxymethyl
            } else {
                Hydrogen
            };
            CarbonPattern {
                up,
                branch,
                opposite,
            }
        })
        .collect()
}

fn substituent(
    doc: &mut Document,
    kind: Substituent,
    end: Point,
    length: f32,
    depth: f32,
) -> Result<u64, String> {
    use Substituent::*;
    let (anchor, members) = match kind {
        Hydroxymethyl => return hydroxymethyl(doc, end, length, depth),
        Hydroxyl => {
            let id = doc.add_atom("O", end);
            doc.atom_mut(id).ok_or("Missing hydroxyl")?.label_h = 1;
            (id, vec![id])
        }
        Hydrogen => {
            let id = doc.add_atom("H", end);
            (id, vec![id])
        }
        Methyl => {
            let id = doc.add_atom("C", end);
            let atom = doc.atom_mut(id).ok_or("Missing methyl")?;
            atom.label_h = 3;
            atom.display.carbons = Some(crate::atom_labels::Carbons::All);
            atom.display.hydrogens = Some(true);
            atom.display.hydrogen_position = crate::atom_labels::HydrogenPosition::Right;
            (id, vec![id])
        }
        Carboxyl => {
            let carbon = doc.add_atom("C", end);
            let oxo = doc.add_atom("O", end.offset(-length * 0.8, -length * 0.6));
            let hydroxyl = doc.add_atom("O", end.offset(length * 0.8, -length * 0.6));
            doc.add_bond(carbon, oxo, 2, "plain");
            doc.add_bond(carbon, hydroxyl, 1, "plain");
            doc.atom_mut(hydroxyl)
                .ok_or("Missing carboxyl hydroxyl")?
                .label_h = 1;
            let members = vec![carbon, oxo, hydroxyl];
            doc.contract(&members, "CO2H", "HO2C")?;
            (carbon, members)
        }
        Acetamido => {
            let nitrogen = doc.add_atom("N", end);
            let carbon = doc.add_atom("C", end.offset(length * 0.8, length * 0.6));
            let oxygen = doc.add_atom("O", end.offset(length * 0.8, length * 1.6));
            let methyl = doc.add_atom("C", end.offset(length * 1.6, 0.));
            doc.add_bond(nitrogen, carbon, 1, "plain");
            doc.add_bond(carbon, oxygen, 2, "plain");
            doc.add_bond(carbon, methyl, 1, "plain");
            doc.atom_mut(nitrogen)
                .ok_or("Missing acetamido nitrogen")?
                .label_h = 1;
            let members = vec![nitrogen, carbon, oxygen, methyl];
            doc.contract(&members, "NHAc", "AcHN")?;
            (nitrogen, members)
        }
    };
    for id in members {
        doc.atom_mut(id).ok_or("Missing substituent atom")?.depth = depth;
    }
    Ok(anchor)
}

/// Ring atoms are ordered from the anomeric carbon around the front to oxygen.
/// The default omits ring H atoms and preserves the original ten drawings.
pub fn sugar_document(sugar: Sugar, anomer: Anomer, length: f32) -> Result<Document, String> {
    sugar_document_with_options(sugar, anomer, length, HaworthOptions::default())
}

/// Build a named sugar from a per-carbon face/substituent pattern. Explicit
/// hydrogens occupy the opposite face and the fourth AtomStereo neighbor.
pub fn sugar_document_with_options(
    sugar: Sugar,
    anomer: Anomer,
    length: f32,
    options: HaworthOptions,
) -> Result<Document, String> {
    if !length.is_finite() || !(1. ..=10_000.).contains(&length) {
        return Err("Haworth bond length must be finite and between 1 and 10000".into());
    }
    // Full pyranoses need a bond length between opposing inward labels.
    // Fructose's inward OH also needs clearance from the sloping back edge.
    let ring_scale = if options.ring_hydrogens && sugar.ring() == Ring::Six {
        3.
    } else if matches!(sugar, Sugar::Fructose) || options.ring_hydrogens {
        2.5
    } else {
        1.8
    };
    let mut doc = sugar.ring().document(length * ring_scale, true);
    let ring: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let patterns = pattern(sugar, anomer);
    let mut stereo = Vec::new();
    for (i, (&id, pattern)) in ring.iter().zip(patterns).enumerate() {
        let atom = doc.atom(id).ok_or("Missing Haworth ring atom")?;
        let origin = atom.position;
        let depth = atom.depth;
        let previous = ring
            .get((i + ring.len() - 1) % ring.len())
            .copied()
            .ok_or("Missing previous Haworth ring atom")?;
        let next = ring
            .get(i + 1)
            .copied()
            .ok_or("Missing next Haworth ring atom")?;
        let mut neighbors = vec![previous, next];
        for (kind, up) in [
            (pattern.branch, pattern.up),
            (pattern.opposite, !pattern.up),
        ] {
            if kind == Substituent::Hydrogen && !options.ring_hydrogens {
                continue;
            }
            let end = origin.offset(0., if up { -length } else { length });
            let branch = substituent(&mut doc, kind, end, length, depth)?;
            if kind == Substituent::Hydroxymethyl
                && i + 2 == ring.len()
                && sugar.ring() == Ring::Five
                && let Some(group) = doc.abbreviations.iter_mut().find(|g| g.anchor == branch)
            {
                group.alignment = crate::abbreviations::LabelAlignment::Right;
            }
            if kind == Substituent::Hydroxyl
                && (sugar.ring() == Ring::Six && i == 3 || i > 0 && up && origin.x > 0.)
            {
                doc.atom_mut(branch)
                    .ok_or("Missing hydroxyl")?
                    .display
                    .hydrogen_position = crate::atom_labels::HydrogenPosition::Left;
            }
            doc.add_bond(id, branch, 1, "plain");
            neighbors.push(branch);
        }
        if pattern.branch != pattern.opposite {
            stereo.push((
                id,
                AtomStereo {
                    // With [previous, next, first branch, opposite] above-plane
                    // first substituents have counterclockwise winding.
                    winding: if pattern.up { "ccw" } else { "cw" }.into(),
                    neighbors,
                },
            ));
        }
    }
    // Adding neighboring bonds invalidates cached stereo. Install only after
    // the graph is complete, including every explicit ring hydrogen.
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
            && doc
                .bonds
                .iter()
                .any(|b| (b.a == atom.id || b.b == atom.id) && b.order == 1)
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

pub fn templates() -> Result<Vec<Template>, String> {
    let entries: Vec<Entry> =
        serde_json::from_str(include_str!("../../../assets/haworth-sugars.json"))
            .map_err(|e| format!("Haworth templates could not be read: {e}"))?;
    let mut result = Vec::new();
    for entry in entries {
        let ring = entry.sugar.ring();
        let anomer = if entry.anomer == Anomer::Alpha {
            "alpha"
        } else {
            "beta"
        };
        let mut keywords = vec![
            "sugar".into(),
            "Haworth".into(),
            entry.sugar.name().into(),
            anomer.into(),
            if ring == Ring::Six {
                "pyranose"
            } else {
                "furanose"
            }
            .into(),
        ];
        keywords.extend(
            match entry.sugar {
                Sugar::Deoxyribose => vec!["dRib", "2-deoxyribose", "deoxy"],
                Sugar::Xylose => vec!["Xyl"],
                Sugar::Fucose => vec!["Fuc", "L-fucose", "6-deoxygalactose"],
                Sugar::Rhamnose => vec!["Rha", "L-rhamnose", "6-deoxymannose"],
                Sugar::GlcNAc => vec!["N-acetylglucosamine", "glucosamine"],
                Sugar::GalNAc => vec!["N-acetylgalactosamine", "galactosamine"],
                Sugar::GlucuronicAcid => vec!["GlcA", "glucuronate", "uronic acid"],
                _ => Vec::new(),
            }
            .into_iter()
            .map(String::from),
        );
        for ring_hydrogens in [false, true] {
            let mut variant_keywords = keywords.clone();
            if ring_hydrogens {
                variant_keywords.extend(["ring H".into(), "explicit hydrogen".into()]);
            }
            result.push(Template {
                low_rank_keywords: Vec::new(),
                id: String::new(),
                group: "Carbohydrates".into(),
                name: format!("{} · Haworth{}", entry.name, if ring_hydrogens { ", ring H" } else { "" }),
                smiles: entry.smiles.clone(),
                keywords: variant_keywords,
                note: "Haworth projection with defined sugar stereochemistry. CH2OH, NHAc and CO2H contain real atoms. The ring H variant includes every ring-carbon hydrogen. Native/figure output retains perspective; cleanup creates a conventional stereochemical drawing.".into(),
                document: sugar_document_with_options(entry.sugar, entry.anomer, crate::style::DEFAULT.bond_length_world, HaworthOptions { ring_hydrogens })?,
                anchor: Anchor::Auto,
            });
        }
    }
    for (ring, name, smiles) in [
        (Ring::Five, "Furanose scaffold", "C1CCOC1"),
        (Ring::Six, "Pyranose scaffold", "C1CCOCC1"),
    ] {
        result.push(Template {
            low_rank_keywords: Vec::new(),
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
mod tests;
