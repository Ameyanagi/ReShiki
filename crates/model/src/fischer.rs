//! Explicit Fischer projections: vertical ligands point away from the viewer,
//! and horizontal ligands point toward the viewer. Projection bonds remain
//! plain single bonds; the chemical configuration lives on the central atom.
pub use crate::haworth::interchange::equivalent;
use crate::{
    document::{AtomStereo, Document, Point},
    templates::{Anchor, Template},
};
use std::collections::{HashMap, HashSet};

/// An axis-aligned, explicitly marked Fischer centre. Bond indices follow the
/// same up, right, down, left order as the ligand IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cross {
    pub center: u64,
    pub up: u64,
    pub right: u64,
    pub down: u64,
    pub left: u64,
    pub bonds: [usize; 4],
}

impl Cross {
    pub fn neighbors(self) -> [u64; 4] {
        [self.up, self.right, self.down, self.left]
    }

    pub fn reading(self) -> AtomStereo {
        reading(self.up, self.right, self.down, self.left)
    }
}

/// Read the Fischer convention in a stable explicit ligand order. This is a
/// geometric parity rule, independent of CIP priorities and D/L names.
pub fn reading(up: u64, right: u64, down: u64, left: u64) -> AtomStereo {
    AtomStereo {
        winding: "ccw".into(),
        neighbors: vec![up, right, down, left],
    }
}

#[derive(Debug, Default)]
pub struct Recognition {
    pub crosses: Vec<Cross>,
    pub bonds: HashSet<usize>,
    pub stereo: HashMap<u64, AtomStereo>,
}

/// A stored configuration plus four plain, projection-marked single bonds
/// identify an explicit Fischer object, even when an edit moves its cross
/// off-axis. Projection marks alone (e.g. emphasized front bonds on a flat
/// drawing) never make a Fischer centre.
pub fn marked_centers(doc: &Document) -> HashSet<u64> {
    if !doc.bonds.iter().any(|bond| bond.projection) {
        return HashSet::new();
    }
    let stored: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.stereo.is_some())
        .map(|a| a.id)
        .collect();
    let mut counts: HashMap<u64, (usize, usize)> = HashMap::new();
    for bond in &doc.bonds {
        let marked = usize::from(bond.projection && bond.order == 1 && bond.display == "plain");
        for id in [bond.a, bond.b] {
            let (total, projected) = counts.entry(id).or_default();
            *total += 1;
            *projected += marked;
        }
    }
    counts
        .into_iter()
        .filter_map(|(id, counts)| (counts == (4, 4) && stored.contains(&id)).then_some(id))
        .collect()
}

/// The axis-aligned cross of four plain, projection-marked single bonds at
/// `center`. Callers decide whether the centre is an explicit Fischer object.
pub fn recognize_center(doc: &Document, center: u64) -> Option<Cross> {
    let incident: Vec<_> = doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| b.a == center || b.b == center)
        .map(|(i, _)| i)
        .collect();
    let positions = doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    cross_from(doc, center, &incident, &positions)
}

fn cross_from(
    doc: &Document,
    center: u64,
    incident: &[usize],
    positions: &HashMap<u64, Point>,
) -> Option<Cross> {
    let origin = positions.get(&center)?;
    if incident.len() != 4 || !origin.is_finite() {
        return None;
    }
    let mut axes = [None; 4];
    // Ten degrees allows ordinary hand placement and coordinate rounding while
    // keeping off-axis drawings from acquiring a new Fischer interpretation.
    const AXIS_TOLERANCE: f32 = 0.176_326_98;
    for &index in incident {
        let bond = doc.bonds.get(index)?;
        let other = if bond.a == center {
            bond.b
        } else if bond.b == center {
            bond.a
        } else {
            continue;
        };
        if bond.order != 1 || bond.display != "plain" || !bond.projection {
            return None;
        }
        let position = positions.get(&other)?;
        let (dx, dy) = (position.x - origin.x, position.y - origin.y);
        if !dx.is_finite() || !dy.is_finite() || dx == 0. && dy == 0. {
            return None;
        }
        let axis = if dx.abs() <= dy.abs() * AXIS_TOLERANCE {
            if dy < 0. { 0 } else { 2 }
        } else if dy.abs() <= dx.abs() * AXIS_TOLERANCE {
            if dx > 0. { 1 } else { 3 }
        } else {
            return None;
        };
        let slot = axes.get_mut(axis)?;
        if slot.replace((other, index)).is_some() {
            return None;
        }
    }
    let [
        Some((up, ub)),
        Some((right, rb)),
        Some((down, db)),
        Some((left, lb)),
    ] = axes
    else {
        return None;
    };
    Some(Cross {
        center,
        up,
        right,
        down,
        left,
        bonds: [ub, rb, db, lb],
    })
}

/// Axis-aligned crosses at centres with a stored configuration.
pub fn recognize(doc: &Document) -> Recognition {
    recognize_where(doc, |atom| atom.stereo.is_some())
}

fn recognize_where(
    doc: &Document,
    candidate: impl Fn(&crate::document::Atom) -> bool,
) -> Recognition {
    let mut result = Recognition::default();
    if !doc.bonds.iter().any(|b| b.projection) {
        return result;
    }
    let positions = doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    let mut incident: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, bond) in doc.bonds.iter().enumerate() {
        incident.entry(bond.a).or_default().push(index);
        incident.entry(bond.b).or_default().push(index);
    }
    for atom in doc.atoms.iter().filter(|a| candidate(a)) {
        if let Some(cross) = incident
            .get(&atom.id)
            .and_then(|bonds| cross_from(doc, atom.id, bonds, &positions))
        {
            result.bonds.extend(cross.bonds);
            result.stereo.insert(atom.id, cross.reading());
            result.crosses.push(cross);
        }
    }
    result
}

/// Re-read an edited axis-aligned Fischer cross. A quarter turn changes its
/// configuration; a half turn retains it. Away from the axes the previously
/// stored chemical configuration remains authoritative. A centre keeps its
/// Fischer identity through an edit that cleared its cached stereo (e.g. a
/// changed substituent), but a cross that was never a Fischer object is not
/// given one.
pub fn reconcile(doc: &mut Document, before: &Document) {
    let previous = marked_centers(before);
    let recognized = recognize_where(doc, |atom| {
        atom.stereo.is_some() || previous.contains(&atom.id)
    });
    for atom in &mut doc.atoms {
        if let Some(reading) = recognized.stereo.get(&atom.id)
            && atom
                .stereo
                .as_ref()
                .is_none_or(|stored| !equivalent(stored, reading))
        {
            atom.stereo = Some(reading.clone());
            atom.cip_label = None;
            atom.undefined_stereo = false;
        }
    }
}

/// Molecules supplied as conventional, open-chain Fischer templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compound {
    DGlyceraldehyde,
    LGlyceraldehyde,
    DGlucose,
    DRibose,
    LAlanine,
    LSerine,
    SnGlycerol3Phosphate,
}

#[derive(Clone, Copy)]
enum Terminal {
    Aldehyde,
    Hydroxymethyl,
    Carboxyl,
    Methyl,
    Phosphoryloxymethyl,
}

fn terminal(
    doc: &mut Document,
    kind: Terminal,
    position: Point,
    length: f32,
) -> Result<u64, String> {
    let carbon = doc.add_atom("C", position);
    let mut members = vec![carbon];
    let mut add = |doc: &mut Document, element, offset: Point, from, order| {
        let atom = doc.add_atom(
            element,
            position.offset(offset.x * length, offset.y * length),
        );
        doc.add_bond(from, atom, order, "plain");
        members.push(atom);
        atom
    };
    let (label, reverse) = match kind {
        Terminal::Aldehyde => {
            add(doc, "O", Point::new(0.7, -0.5), carbon, 2);
            ("CHO", "OHC")
        }
        Terminal::Hydroxymethyl => {
            add(doc, "O", Point::new(0.7, 0.5), carbon, 1);
            ("CH2OH", "HOH2C")
        }
        Terminal::Carboxyl => {
            add(doc, "O", Point::new(0.7, -0.5), carbon, 2);
            add(doc, "O", Point::new(-0.7, -0.5), carbon, 1);
            ("COOH", "HOOC")
        }
        Terminal::Methyl => ("CH3", "H3C"),
        Terminal::Phosphoryloxymethyl => {
            let oxygen = add(doc, "O", Point::new(0.7, 0.5), carbon, 1);
            let phosphorus = add(doc, "P", Point::new(1.4, 1.), oxygen, 1);
            add(doc, "O", Point::new(2.1, 1.5), phosphorus, 2);
            add(doc, "O", Point::new(1.4, 2.), phosphorus, 1);
            add(doc, "O", Point::new(2.1, 0.5), phosphorus, 1);
            ("CH2OPO3H2", "H2O3POH2C")
        }
    };
    doc.contract(&members, label, reverse)?;
    if let Some(group) = doc.abbreviations.iter_mut().find(|g| g.anchor == carbon) {
        group.alignment = crate::abbreviations::LabelAlignment::Center;
    }
    Ok(carbon)
}

/// Build real chemical atoms and explicit horizontal H atoms, then store the
/// Fischer reading only after every bond has been added.
pub fn document(compound: Compound, length: f32) -> Result<Document, String> {
    if !length.is_finite() || length <= 0. {
        return Err("Fischer bond length must be finite and positive".into());
    }
    let (top, bottom, hetero, left): (_, _, _, &[bool]) = match compound {
        Compound::DGlyceraldehyde => (Terminal::Aldehyde, Terminal::Hydroxymethyl, "O", &[false]),
        Compound::LGlyceraldehyde => (Terminal::Aldehyde, Terminal::Hydroxymethyl, "O", &[true]),
        Compound::DGlucose => (
            Terminal::Aldehyde,
            Terminal::Hydroxymethyl,
            "O",
            &[false, true, false, false],
        ),
        Compound::DRibose => (
            Terminal::Aldehyde,
            Terminal::Hydroxymethyl,
            "O",
            &[false, false, false],
        ),
        Compound::LAlanine => (Terminal::Carboxyl, Terminal::Methyl, "N", &[true]),
        Compound::LSerine => (Terminal::Carboxyl, Terminal::Hydroxymethyl, "N", &[true]),
        Compound::SnGlycerol3Phosphate => (
            Terminal::Hydroxymethyl,
            Terminal::Phosphoryloxymethyl,
            "O",
            &[true],
        ),
    };
    let mut doc = Document::default();
    let centers: Vec<_> = left
        .iter()
        .enumerate()
        .map(|(i, _)| doc.add_atom("C", Point::new(0., i as f32 * length * 1.5)))
        .collect();
    let top = terminal(&mut doc, top, Point::new(0., -length * 1.5), length)?;
    let bottom = terminal(
        &mut doc,
        bottom,
        Point::new(0., centers.len() as f32 * length * 1.5),
        length,
    )?;
    let chain: Vec<_> = std::iter::once(top)
        .chain(centers.iter().copied())
        .chain(std::iter::once(bottom))
        .collect();
    for pair in chain.windows(2) {
        if let [a, b] = pair {
            doc.add_bond(*a, *b, 1, "plain");
            if let Some(bond) = doc.bonds.last_mut() {
                bond.projection = true;
            }
        }
    }
    for (&center, &on_left) in centers.iter().zip(left) {
        let origin = doc.atom(center).ok_or("Missing Fischer centre")?.position;
        for (element, is_left) in [(hetero, on_left), ("H", !on_left)] {
            let branch = doc.add_atom(
                element,
                origin.offset(if is_left { -length } else { length }, 0.),
            );
            if element != "H"
                && let Some(atom) = doc.atom_mut(branch)
            {
                atom.label_h = if element == "N" { 2 } else { 1 };
                atom.display.hydrogens = Some(true);
                atom.display.hydrogen_position = if is_left {
                    crate::atom_labels::HydrogenPosition::Left
                } else {
                    crate::atom_labels::HydrogenPosition::Right
                };
            }
            doc.add_bond(center, branch, 1, "plain");
            if let Some(bond) = doc.bonds.last_mut() {
                bond.projection = true;
            }
        }
    }
    for id in centers {
        let cross = recognize_center(&doc, id).ok_or("Missing Fischer centre")?;
        doc.atom_mut(id).ok_or("Missing Fischer centre")?.stereo = Some(cross.reading());
    }
    doc.validate()?;
    Ok(doc)
}

pub fn templates() -> Result<Vec<Template>, String> {
    [
        (Compound::DGlyceraldehyde, "D-glyceraldehyde", "O=C[C@H](O)CO"),
        (Compound::LGlyceraldehyde, "L-glyceraldehyde", "O=C[C@@H](O)CO"),
        (Compound::DGlucose, "aldehydo-D-glucose", "O=C[C@H](O)[C@@H](O)[C@H](O)[C@H](O)CO"),
        (Compound::DRibose, "aldehydo-D-ribose", "O=C[C@H](O)[C@H](O)[C@H](O)CO"),
        (Compound::LAlanine, "L-alanine", "C[C@H](N)C(=O)O"),
        (Compound::LSerine, "L-serine", "N[C@@H](CO)C(=O)O"),
        (Compound::SnGlycerol3Phosphate, "sn-glycerol 3-phosphate", "OC[C@@H](O)COP(=O)(O)O"),
    ].into_iter().map(|(compound, name, smiles)| {
        let mut keywords = vec!["Fischer".into(), "projection".into(), name.into()];
        keywords.extend(match compound {
            Compound::DGlyceraldehyde | Compound::LGlyceraldehyde => vec!["glyceraldehyde", "aldose", "sugar"],
            Compound::DGlucose => vec!["D-glucose", "glucose", "Glc", "aldose", "sugar", "open chain"],
            Compound::DRibose => vec!["D-ribose", "ribose", "Rib", "aldose", "sugar", "open chain"],
            Compound::LAlanine => vec!["alanine", "Ala", "amino acid"],
            Compound::LSerine => vec!["serine", "Ser", "amino acid"],
            Compound::SnGlycerol3Phosphate => vec!["glycerol phosphate", "sn", "glycerophosphate"],
        }.into_iter().map(String::from));
        if let Some(series) = name.split('-').find(|s| matches!(*s, "D" | "L")) {
            keywords.push(series.into());
        }
        Ok(Template {
            id: String::new(), group: "Fischer projections".into(), name: format!("{name} · Fischer"),
            smiles: smiles.into(), keywords, low_rank_keywords: Vec::new(),
            note: "Fischer projection with explicit stereochemistry: horizontal bonds point toward the viewer and vertical bonds point away. A 90° turn inverts the configuration; a 180° turn retains it. Terminal labels contain real atoms.".into(),
            locants: Default::default(), document: document(compound, crate::style::DEFAULT.bond_length_world)?, anchor: Anchor::Auto,
        })
    }).collect()
}

#[cfg(test)]
mod tests;
