use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrangement {
    #[default]
    Rows,
    Central,
    Grid,
    Branching,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Main,
    Example,
    #[default]
    Reaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Composition {
    pub arrangement: Arrangement,
    pub columns: usize,
    pub width_pt: f32,
    pub preserve_details: bool,
}
impl Default for Composition {
    fn default() -> Self {
        Self {
            arrangement: Arrangement::Rows,
            columns: 2,
            width_pt: 540.,
            preserve_details: false,
        }
    }
}
impl Composition {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=3).contains(&self.columns)
            || !self.width_pt.is_finite()
            || !(100. ..=1800.).contains(&self.width_pt)
        {
            return Err("Use 1–3 columns and a figure width between 100 and 1800 pt".into());
        }
        Ok(())
    }
}
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"properties":{
        "arrangement":{"type":"string","enum":["rows","central","grid","branching"]},
        "preserve_details":{"type":"boolean","description":"True when the user explicitly requests full chains or structural detail. Disables automatic chain compaction throughout generation and review."},
        "columns":{"type":"integer","minimum":1,"maximum":3},
        "width_pt":{"type":"number","minimum":100,"maximum":1800,"description":"Intended figure width in points; 540 for a page. Use compact chains and separate panels to fit; bond lengths are preserved."}
    },"required":["arrangement","columns","width_pt","preserve_details"]})
}

/// Place complete measured panels, preserving physical bond lengths and internal layout.
pub fn arrange(doc: &mut Document, panels: &[(Vec<u64>, Role)], config: &Composition) {
    if config.arrangement == Arrangement::Branching {
        let directions: Vec<_> = doc
            .arrows
            .iter()
            .map(|a| {
                (a.end.y - a.start.y)
                    .atan2(a.end.x - a.start.x)
                    .to_degrees()
            })
            .collect();
        super::branching::arrange(doc, &directions);
        return;
    }
    let measured: Vec<_> = panels
        .iter()
        .filter_map(|(ids, role)| {
            crate::scene::selection_bounds(doc, ids).map(|(lo, hi)| (ids.clone(), *role, lo, hi))
        })
        .collect();
    if measured.is_empty() {
        return;
    }
    let gap = doc.drawing_style.bond_length_world * 2.;
    let move_to = |doc: &mut Document, panel: &(Vec<u64>, Role, Point, Point), x: f32, y: f32| {
        doc.translate(&panel.0, x - panel.2.x, y - panel.2.y);
    };
    if config.arrangement == Arrangement::Rows {
        let anchor = measured
            .iter()
            .filter_map(|p| {
                doc.arrows
                    .iter()
                    .find(|a| p.0.contains(&a.id))
                    .map(|a| a.start.x - p.2.x)
            })
            .fold(0., f32::max);
        let mut y = 0.;
        for panel in &measured {
            let offset = doc
                .arrows
                .iter()
                .find(|a| panel.0.contains(&a.id))
                .map(|a| a.start.x - panel.2.x)
                .unwrap_or(0.);
            move_to(doc, panel, anchor - offset, y);
            y += panel.3.y - panel.2.y + gap;
        }
        return;
    }
    let main = if config.arrangement == Arrangement::Central {
        measured.iter().position(|p| p.1 == Role::Main).or(Some(0))
    } else {
        None
    };
    let others: Vec<_> = measured
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != main)
        .map(|(_, p)| p)
        .collect();
    let width = crate::style::DEFAULT.world(config.width_pt);
    let widest = measured.iter().map(|p| p.3.x - p.2.x).fold(1., f32::max);
    let columns = config
        .columns
        .min(((width + gap) / (widest + gap)).floor().max(1.) as usize)
        .max(1);
    let mut y = 0.;
    let place_grid_row =
        |doc: &mut Document, row: &[&(Vec<u64>, Role, Point, Point)], y: &mut f32| {
            let row_width = row.iter().map(|p| p.3.x - p.2.x).sum::<f32>()
                + gap * row.len().saturating_sub(1) as f32;
            let mut x = (width.max(row_width) - row_width) / 2.;
            let height = row.iter().map(|p| p.3.y - p.2.y).fold(0., f32::max);
            for p in row {
                move_to(doc, p, x, *y);
                x += p.3.x - p.2.x + gap;
            }
            *y += height + gap;
        };
    let split = if main.is_some() {
        others.len().div_ceil(2)
    } else {
        others.len()
    };
    for row in others.get(..split).unwrap_or_default().chunks(columns) {
        place_grid_row(doc, row, &mut y);
    }
    if let Some(p) = main.and_then(|i| measured.get(i)) {
        move_to(doc, p, (width.max(p.3.x - p.2.x) - (p.3.x - p.2.x)) / 2., y);
        y += p.3.y - p.2.y + gap;
    }
    for row in others.get(split..).unwrap_or_default().chunks(columns) {
        place_grid_row(doc, row, &mut y);
    }
}

/// Contract terminal, unbranched carbon chains. The original atoms, bonds and stereo remain intact.
pub fn compact_chains(doc: &mut Document, allowed: &[u64]) -> Result<usize, String> {
    let mut used = std::collections::HashSet::new();
    let mut chains = Vec::new();
    for atom in &doc.atoms {
        if !allowed.contains(&atom.id)
            || used.contains(&atom.id)
            || doc
                .bonds
                .iter()
                .filter(|b| b.a == atom.id || b.b == atom.id)
                .count()
                != 1
        {
            continue;
        }
        let mut chain = Vec::new();
        let mut next = Some(atom.id);
        while let Some(id) = next {
            let Some(a) = doc.atom(id) else {
                break;
            };
            let bonds: Vec<_> = doc
                .bonds
                .iter()
                .filter(|b| b.a == id || b.b == id)
                .collect();
            if !allowed.contains(&id)
                || a.element != "C"
                || a.aromatic
                || a.isotope != 0
                || a.charge != 0
                || a.radical_electrons != 0
                || a.map_num != 0
                || a.stereo.is_some()
                || bonds.len() > 2
                || doc.abbreviations.iter().any(|g| g.members.contains(&id))
            {
                break;
            }
            if bonds.iter().any(|b| {
                doc.atom(if b.a == id { b.b } else { b.a })
                    .is_none_or(|a| a.element != "C")
            }) {
                break;
            }
            chain.push(id);
            next = bonds
                .iter()
                .map(|b| if b.a == id { b.b } else { b.a })
                .find(|id| !chain.contains(id));
        }
        if chain.len() >= 6 && !chain.iter().any(|id| used.contains(id)) {
            used.extend(chain.iter().copied());
            chains.push(chain);
        }
    }
    for chain in &chains {
        let hydrogens: u32 = chain
            .iter()
            .filter_map(|id| doc.atom(*id))
            .map(|a| a.label_h)
            .sum();
        let label = format!("C{}H{}", chain.len(), hydrogens);
        doc.contract(chain, &label, &format!("H{hydrogens}C{}", chain.len()))?;
    }
    Ok(chains.len())
}

/// Remove a common small rotation from conventional 30-degree bond geometry.
/// A circular mean avoids the discontinuity at 0/180 degrees; irregular ring
/// geometry is left intact. Only the complete molecular unit is rotated.
pub fn straighten(doc: &mut Document, ids: &[u64]) -> bool {
    let mut sin = 0.;
    let mut cos = 0.;
    let mut count = 0;
    for bond in &doc.bonds {
        if !ids.contains(&bond.a) || !ids.contains(&bond.b) || !doc.bond_visible(bond.a, bond.b) {
            continue;
        }
        if let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) {
            let angle = (b.position.y - a.position.y).atan2(b.position.x - a.position.x) * 12.;
            sin += angle.sin();
            cos += angle.cos();
            count += 1;
        }
    }
    if count == 0 || sin.hypot(cos) / (count as f32) < 0.85 {
        return false;
    }
    let correction = -sin.atan2(cos).to_degrees() / 12.;
    if correction.abs() < 0.01 {
        return false;
    }
    crate::editing::transform(doc, ids, crate::editing::Transform::Rotate(correction));
    true
}

pub fn straighten_all(doc: &mut Document) -> usize {
    let molecules = crate::reactions::molecules(doc, &doc.all_ids());
    molecules.iter().filter(|ids| straighten(doc, ids)).count()
}
