//! Own equal-length geometry for the chemically defined 21H,23H free base.
//! The frozen graph uses OPSIN locants; vendor coordinates/artwork are not used.
use super::{Anchor, Template};
use crate::{
    atom_labels::HydrogenPosition,
    document::{Document, Point},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Facts {
    group: String,
    name: String,
    smiles: String,
    keywords: Vec<String>,
    note: String,
    bonds: Vec<(u64, u64, u8)>,
}

fn quarter((x, y): (f64, f64), q: u64) -> Point {
    let (x, y) = match q % 4 {
        0 => (x, y),
        1 => (-y, x),
        2 => (-x, -y),
        _ => (y, -x),
    };
    Point::new(x as f32, y as f32)
}

fn geometry(facts: &Facts, length: f32) -> Result<Document, String> {
    if !length.is_finite() || !(1. ..=10_000.).contains(&length) {
        return Err("Porphine bond length must be finite and between 1 and 10000".into());
    }
    let l = f64::from(length);
    let radius = l / (2. * (std::f64::consts::PI / 5.).sin());
    let step = std::f64::consts::TAU / 5.;
    let a = radius * step.sin();
    let b = radius * step.cos();
    // Four regular five-rings joined by equal-length, 120° meso corners.
    let distance = a + b + 1.5_f64.sqrt() * l;
    let mid = (a + distance - b) * 0.5;
    let corner = mid + l / (2. * 2_f64.sqrt());
    let mut doc = Document::default();
    for locant in 1_u64..=20 {
        let offset = (locant - 1) % 5;
        let p = if offset == 4 {
            (corner, -corner)
        } else {
            let angle = (4 - offset) as f64 * step;
            (radius * angle.sin(), -distance + radius * angle.cos())
        };
        doc.add_atom("C", quarter(p, (locant - 1) / 5));
    }
    for q in 0..4 {
        doc.add_atom("N", quarter((0., -distance + radius), q));
    }
    for &(a, b, order) in &facts.bonds {
        if !(1..=24).contains(&a) || !(1..=24).contains(&b) || !matches!(order, 1 | 2) {
            return Err("Invalid frozen porphine bond graph".into());
        }
        doc.add_bond(a, b, order, "plain");
    }
    for (locant, position) in [(21, HydrogenPosition::Below), (23, HydrogenPosition::Above)] {
        let atom = doc.atom_mut(locant).ok_or("Missing porphine nitrogen")?;
        atom.label_h = 1;
        atom.display.hydrogen_position = position;
    }
    doc.validate()?;
    Ok(doc)
}

pub(super) fn template() -> Result<Template, String> {
    let facts: Facts = serde_json::from_str(include_str!("../../../../assets/porphine-core.json"))
        .map_err(|e| format!("Porphine chemical facts could not be read: {e}"))?;
    let document = geometry(&facts, crate::style::DEFAULT.bond_length_world)?;
    Ok(Template {
        id: String::new(),
        group: facts.group,
        name: facts.name,
        smiles: facts.smiles,
        keywords: facts.keywords,
        note: facts.note,
        document,
        anchor: Anchor::Auto,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_core_has_twenty_carbons_opposite_hydrogens_and_equal_bonds() {
        let item = template().unwrap();
        let doc = &item.document;
        assert_eq!(doc.atoms.len(), 24);
        assert_eq!(doc.bonds.len(), 28);
        assert_eq!(doc.atoms.iter().filter(|a| a.element == "C").count(), 20);
        for atom in doc.atoms.iter().filter(|a| a.element == "N") {
            assert_eq!(atom.label_h, u32::from(matches!(atom.id, 21 | 23)));
            let valence: u8 = doc
                .bonds
                .iter()
                .filter(|b| b.a == atom.id || b.b == atom.id)
                .map(|b| b.order)
                .sum();
            assert_eq!(valence, if atom.label_h == 1 { 2 } else { 3 });
        }
        for bond in &doc.bonds {
            let length = doc
                .atom(bond.a)
                .unwrap()
                .position
                .distance(doc.atom(bond.b).unwrap().position);
            assert!((length - 42.).abs() < 0.00003, "{bond:?}: {length}");
        }
        for id in 1..=5 {
            let p = doc.atom(id).unwrap().position;
            let q = doc.atom(id + 5).unwrap().position;
            assert_eq!(q, Point::new(-p.y, p.x));
        }
        for meso in [5, 10, 15, 20] {
            let center = doc.atom(meso).unwrap().position;
            let before = doc.atom(meso - 1).unwrap().position;
            let after = doc.atom(meso % 20 + 1).unwrap().position;
            let a = (before.x - center.x, before.y - center.y);
            let b = (after.x - center.x, after.y - center.y);
            let cosine =
                (a.0 * b.0 + a.1 * b.1) / (before.distance(center) * after.distance(center));
            assert!((cosine + 0.5).abs() < 0.000001, "Meso {meso}: {cosine}");
        }
        let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened, *doc);
    }
}
