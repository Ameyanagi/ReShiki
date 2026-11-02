//! β-D-ribofuranosyl nucleobases, using the carbohydrate projection builder.
use super::{Anchor, Template};
use crate::{
    document::{Document, Point},
    haworth::{Anomer, Sugar, sugar_document},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Entry {
    name: String,
    base: String,
    base_anchor: u64,
    deoxy: bool,
    phosphate: bool,
    smiles: String,
    keywords: Vec<String>,
    low_rank_keywords: Vec<String>,
}

fn document(entry: &Entry, base: &Document, length: f32) -> Result<Document, String> {
    let mut base = base.clone();
    if matches!(entry.base.as_str(), "Adenine" | "Guanine") {
        // The bundled 9H purines may be drawn in either handedness. Keep the
        // fused C4 clockwise of C8 as seen from N9, so the pyrimidine ring
        // clears the sugar once N9 takes the glycosidic bond.
        let n9 = base
            .atom(entry.base_anchor)
            .ok_or("Missing purine N9")?
            .position;
        let (mut c4, mut c8) = (None, None);
        for bond in &base.bonds {
            let other = if bond.a == entry.base_anchor {
                bond.b
            } else if bond.b == entry.base_anchor {
                bond.a
            } else {
                continue;
            };
            let position = base.atom(other).ok_or("Missing purine atom")?.position;
            if base
                .bonds
                .iter()
                .filter(|b| b.a == other || b.b == other)
                .count()
                == 3
            {
                c4 = Some(position);
            } else {
                c8 = Some(position);
            }
        }
        let (c4, c8) = (
            c4.ok_or("Missing purine C4")?,
            c8.ok_or("Missing purine C8")?,
        );
        if (c8.x - n9.x) * (c4.y - n9.y) < (c8.y - n9.y) * (c4.x - n9.x) {
            for atom in &mut base.atoms {
                atom.position.x = -atom.position.x;
            }
        }
    } else if entry.base == "Cytosine" {
        // The free-base template is the 3H tautomer. N1 glycosylation needs
        // the 1H keto-amino tautomer, with N3=C4 and C5=C6.
        for (a, b, order) in [(2, 3, 1), (3, 4, 2), (4, 5, 1), (8, 2, 2)] {
            base.add_bond(a, b, order, "plain");
        }
        base.atom_mut(8).ok_or("Missing cytosine N3")?.label_h = 0;
    }
    let sugar = if entry.deoxy {
        Sugar::Deoxyribose
    } else {
        Sugar::Ribose
    };
    let mut doc = sugar_document(sugar, Anomer::Beta, length)?;
    let stereo: Vec<_> = doc
        .atoms
        .iter()
        .filter_map(|a| a.stereo.clone().map(|s| (a.id, s)))
        .collect();
    let anomeric = doc.atoms.first().ok_or("Missing anomeric carbon")?;
    let attachment = *anomeric
        .stereo
        .as_ref()
        .ok_or("Missing anomeric stereo")?
        .neighbors
        .get(2)
        .ok_or("Missing anomeric substituent")?;
    let destination = doc
        .atom(attachment)
        .ok_or("Missing anomeric hydroxyl")?
        .position;
    let source = base
        .atom(entry.base_anchor)
        .ok_or("Missing glycosidic nitrogen")?;
    let neighbors: Vec<_> = base
        .bonds
        .iter()
        .filter_map(|b| {
            let other = if b.a == source.id {
                b.b
            } else if b.b == source.id {
                b.a
            } else {
                return None;
            };
            base.atom(other).map(|a| a.position)
        })
        .collect();
    let center = Point::new(
        neighbors.iter().map(|p| p.x).sum::<f32>() / neighbors.len() as f32,
        neighbors.iter().map(|p| p.y).sum::<f32>() / neighbors.len() as f32,
    );
    // Rotate the nitrogen's exterior bisector downward: the regular base rings
    // then extend above C1′ and clear the leftward C5′ substituent.
    let tilt = if matches!(entry.base.as_str(), "Cytosine" | "Uracil" | "Thymine") {
        // Raise the leftward C2 carbonyl away from the sugar's ring oxygen.
        std::f32::consts::PI / 6.
    } else {
        0.
    };
    let angle = std::f32::consts::FRAC_PI_2 + tilt
        - (source.position.y - center.y).atan2(source.position.x - center.x);
    let (sin, cos) = angle.sin_cos();
    let first = base.bonds.first().ok_or("Missing nucleobase bonds")?;
    let scale = length
        / base
            .atom(first.a)
            .ok_or("Missing base atom")?
            .position
            .distance(base.atom(first.b).ok_or("Missing base atom")?.position);
    let mut ids = HashMap::new();
    for atom in &base.atoms {
        let dx = (atom.position.x - source.position.x) * scale;
        let dy = (atom.position.y - source.position.y) * scale;
        let position = destination.offset(dx * cos - dy * sin, dx * sin + dy * cos);
        let id = if atom.id == source.id {
            attachment
        } else {
            doc.add_atom(&atom.element, position)
        };
        let mut copied = atom.clone();
        copied.id = id;
        copied.position = position;
        if atom.id == source.id {
            copied.explicit_h = 0;
            copied.label_h = 0;
            copied.no_implicit = false;
        }
        *doc.atom_mut(id).ok_or("Missing copied base atom")? = copied;
        ids.insert(atom.id, id);
    }
    for bond in &base.bonds {
        doc.add_bond(
            *ids.get(&bond.a).ok_or("Missing base bond atom")?,
            *ids.get(&bond.b).ok_or("Missing base bond atom")?,
            bond.order,
            "plain",
        );
    }
    if entry.phosphate {
        let c4 = doc.atoms.get(3).ok_or("Missing C4′")?.id;
        let c5 = *stereo
            .iter()
            .find(|(id, _)| *id == c4)
            .ok_or("Missing C4′ stereo")?
            .1
            .neighbors
            .get(2)
            .ok_or("Missing C5′ substituent")?;
        doc.expand_abbreviations(&[c5]);
        let o5 = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == c5 {
                    doc.atom(b.b)
                } else if b.b == c5 {
                    doc.atom(b.a)
                } else {
                    None
                }
            })
            .find(|a| a.element == "O")
            .ok_or("Missing O5′")?
            .id;
        let oxygen = doc.atom_mut(o5).ok_or("Missing O5′")?;
        oxygen.label_h = 0;
        let origin = oxygen.position;
        let phosphorus = doc.add_atom("P", origin.offset(-length, 0.));
        doc.add_bond(o5, phosphorus, 1, "plain");
        for (dx, dy, order) in [(0., -1., 2), (-1., 0., 1), (0., 1., 1)] {
            let oxygen = doc.add_atom("O", origin.offset((dx - 1.) * length, dy * length));
            doc.atom_mut(oxygen)
                .ok_or("Missing phosphate oxygen")?
                .label_h = u32::from(order == 1);
            doc.add_bond(phosphorus, oxygen, order, "plain");
        }
    }
    // Graph construction invalidates neighboring assignments. All attachment
    // substitutions preserve the original clockwise neighbor order.
    for (id, assignment) in stereo {
        doc.atom_mut(id).ok_or("Missing sugar stereocenter")?.stereo = Some(assignment);
    }
    doc.validate()?;
    Ok(doc)
}

pub(super) fn templates(bundled: &[Template]) -> Result<Vec<Template>, String> {
    let entries: Vec<Entry> =
        serde_json::from_str(include_str!("../../../../assets/nucleosides.json"))
            .map_err(|e| format!("Nucleoside facts could not be read: {e}"))?;
    entries.into_iter().map(|entry| {
        // Renamed 9H purines keep their original built-in ids.
        let id = format!("builtin:Nucleobases/{}", entry.base);
        let base = bundled.iter().find(|t| t.group == "Nucleobases" && (t.name == entry.base || t.id == id))
            .ok_or_else(|| format!("Missing bundled nucleobase {}",entry.base))?;
        let document = document(&entry, &base.document, crate::style::DEFAULT.bond_length_world)?;
        Ok(Template {
            id: String::new(),
            group: "Nucleosides".into(),
            name: entry.name,
            smiles: entry.smiles,
            keywords: entry.keywords,
            low_rank_keywords: entry.low_rank_keywords,
            locants: Default::default(),
            note: "β-D-furanose Haworth projection with a real N9 (purine) or N1 (pyrimidine) glycosidic bond. Monophosphates show explicit O5′–P(=O)(OH)2 atoms leftward from C5′. Native/figure output retains perspective; chemical exports retain absolute configuration.".into(),
            document,
            anchor: Anchor::Auto,
        })
    }).collect()
}
