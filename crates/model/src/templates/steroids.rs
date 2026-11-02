//! Own regular-ring geometry in steroid orientation 2a (IUPAC 3S-1.4).
//! Chemical facts and locant-based parity are frozen from PubChem SMILES;
//! source coordinates and artwork are never used.
use super::{Anchor, Template};
use crate::{
    atom_labels::HydrogenPosition,
    document::{AtomStereo, Document, Point},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Stereo {
    center: u64,
    neighbors: Vec<u64>,
    winding: String,
}

#[derive(Deserialize)]
struct Facts {
    name: String,
    smiles: String,
    keywords: Vec<String>,
    note: String,
    atoms: Vec<(u64, String)>,
    bonds: Vec<(u64, u64, u8)>,
    stereo: Vec<Stereo>,
}

/// Steroid locants are also document atom IDs. Three regular hexagons and
/// one regular pentagon share their fusion edges; A is lower left, D upper right.
fn position(locant: u64) -> Result<(f64, f64), String> {
    let s = 3_f64.sqrt() / 2.;
    let (x15, y15) = (
        4. * s + (std::f64::consts::PI / 10.).cos(),
        -1. + (std::f64::consts::PI / 10.).sin(),
    );
    let (x16, y16) = (
        x15 + (std::f64::consts::PI / 5.).sin(),
        y15 - (std::f64::consts::PI / 5.).cos(),
    );
    let (x17, y17) = (
        x16 - (std::f64::consts::PI / 5.).sin(),
        y16 - (std::f64::consts::PI / 5.).cos(),
    );
    Ok(match locant {
        1 => (0., -1.),
        2 => (-s, -0.5),
        3 => (-s, 0.5),
        4 => (0., 1.),
        5 => (s, 0.5),
        6 => (2. * s, 1.),
        7 => (3. * s, 0.5),
        8 => (3. * s, -0.5),
        9 => (2. * s, -1.),
        10 => (s, -0.5),
        11 => (2. * s, -2.),
        12 => (3. * s, -2.5),
        13 => (4. * s, -2.),
        14 => (4. * s, -1.),
        15 => (x15, y15),
        16 => (x16, y16),
        17 => (x17, y17),
        18 => (4. * s, -3.),
        19 => (s, -1.5),
        20 => (x17 + s, y17 - 0.5),
        21 => (x17 + s, y17 - 1.5),
        22 => (x17 + 2. * s, y17),
        23 => (x17 + 3. * s, y17 - 0.5),
        24 => (x17 + 4. * s, y17),
        25 => (x17 + 5. * s, y17 - 0.5),
        26 => (x17 + 6. * s, y17 - 1.),
        27 => (x17 + 5. * s, y17 + 0.5),
        28 => (-2. * s, 1.),
        29 => (x17 + s, y17 - 0.5),
        _ => return Err(format!("Unknown steroid locant {locant}")),
    })
}

fn geometry(facts: &Facts, length: f32, implicit_junction_h: bool) -> Result<Document, String> {
    if !length.is_finite() || !(1. ..=10_000.).contains(&length) {
        return Err("Steroid bond length must be finite and between 1 and 10000".into());
    }
    let mut doc = Document::default();
    for (locant, element) in &facts.atoms {
        let (x, y) = position(*locant)?;
        let id = doc.add_atom(
            element,
            Point::new(
                (x * f64::from(length)) as f32,
                (y * f64::from(length)) as f32,
            ),
        );
        doc.atom_mut(id).ok_or("Missing steroid atom")?.id = *locant;
    }
    for &(a, b, order) in &facts.bonds {
        if doc.atom(a).is_none() || doc.atom(b).is_none() || !matches!(order, 1 | 2) {
            return Err("Invalid frozen steroid bond graph".into());
        }
        let display = match (a, b, order) {
            (13, 18, 1) | (10, 19, 1) | (17, 20, 1) => "bold",
            (17, 29, 1) => "wedge",
            (3, 28, 1) if facts.stereo.iter().any(|s| s.center == 3) => "wedge",
            (20, 21, 1) if facts.stereo.iter().any(|s| s.center == 20) => "hash",
            _ => "plain",
        };
        doc.add_bond(a, b, order, display);
    }
    for oxygen in [28, 29] {
        let valence: u8 = doc
            .bonds
            .iter()
            .filter(|b| b.a == oxygen || b.b == oxygen)
            .map(|b| b.order)
            .sum();
        if let Some(atom) = doc.atom_mut(oxygen) {
            atom.label_h = u32::from(valence == 1);
            atom.display.hydrogen_position = if oxygen == 28 {
                HydrogenPosition::Left
            } else {
                HydrogenPosition::Right
            };
        }
    }
    // Note 1 permits omission of only the conventional 8β/9α/14α H atoms.
    // Retain specified C5 and parent C10/C13 hydrogen labels in both variants.
    // Gonane/estrane deliberately leave C5 unspecified, so omit that C5–H.
    for center in [5, 8, 9, 10, 13, 14] {
        if center == 5 && !facts.stereo.iter().any(|s| s.center == 5) {
            continue;
        }
        if implicit_junction_h && matches!(center, 8 | 9 | 14) {
            continue;
        }
        let valence: u8 = doc
            .bonds
            .iter()
            .filter(|b| b.a == center || b.b == center)
            .map(|b| b.order)
            .sum();
        if valence != 3 {
            continue;
        }
        let origin = doc.atom(center).ok_or("Missing steroid junction")?.position;
        let hydrogen = doc.add_atom(
            "H",
            origin.offset(
                0.,
                if matches!(center, 5 | 9 | 14) {
                    length
                } else {
                    -length
                },
            ),
        );
        let atom = doc.atom_mut(hydrogen).ok_or("Missing steroid hydrogen")?;
        atom.id = center + 100;
        atom.no_implicit = true;
        let display = if matches!(center, 8 | 10 | 13) {
            "bold"
        } else {
            "hash"
        };
        doc.add_bond(center, center + 100, 1, display);
    }
    // Graph mutation invalidates cached parity. Install the source assignments
    // last, replacing the implicit fourth neighbor by the real H where drawn.
    for stereo in &facts.stereo {
        let mut neighbors = stereo.neighbors.clone();
        if doc.atom(stereo.center + 100).is_some() {
            neighbors.push(stereo.center + 100);
        }
        doc.atom_mut(stereo.center)
            .ok_or("Missing steroid stereocenter")?
            .stereo = Some(AtomStereo {
            winding: stereo.winding.clone(),
            neighbors,
        });
    }
    doc.validate()?;
    Ok(doc)
}

pub(super) fn templates() -> Result<Vec<Template>, String> {
    let facts: Vec<Facts> = serde_json::from_str(include_str!("../../../../assets/steroids.json"))
        .map_err(|e| format!("Steroid chemical facts could not be read: {e}"))?;
    let mut templates = Vec::new();
    for item in facts {
        for implicit in [false, true] {
            templates.push(Template {
                id: String::new(),
                group: "Steroids".into(),
                name: if implicit { format!("{} · implicit 8,9,14-H", item.name) }
                    else { item.name.clone() },
                smiles: item.smiles.clone(),
                keywords: item.keywords.clone(),
                low_rank_keywords: Vec::new(),
                locants: Default::default(),
                note: if implicit { format!("{} C-8, C-9 and C-14 H atoms are omitted; their stereochemistry is retained.", item.note) }
                    else { item.note.clone() },
                document: geometry(&item, crate::style::DEFAULT.bond_length_world, implicit)?,
                anchor: Anchor::Auto,
            });
        }
    }
    Ok(templates)
}

#[cfg(test)]
mod tests;
