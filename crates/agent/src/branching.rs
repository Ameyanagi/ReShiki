//! Radial reaction branches share a molecular graph and retain independent products.
use super::Step;
use crate::{
    document::{Document, Point},
    scene,
};

pub fn merge(doc: &mut Document, steps: &[Step]) -> Result<(), String> {
    let Some(first) = doc.reactions.first().cloned() else {
        return Ok(());
    };
    let source = first.reactants.clone();
    let notes: Vec<_> = first
        .annotations
        .iter()
        .copied()
        .filter(|id| *id < first.arrow)
        .collect();
    let mut remove = Vec::new();
    for r in doc.reactions.iter_mut().skip(1) {
        remove.extend(r.reactants.iter().flat_map(|p| p.atoms.iter().copied()));
        remove.extend(r.annotations.iter().copied().filter(|id| *id < r.arrow));
        r.annotations.retain(|id| *id > r.arrow);
        r.annotations.extend(notes.iter().copied());
        r.reactants = source.clone();
    }
    // These are duplicate complete molecules, not chemical edits to the shared source.
    doc.atoms.retain(|a| !remove.contains(&a.id));
    doc.bonds
        .retain(|b| !remove.contains(&b.a) && !remove.contains(&b.b));
    doc.annotations.retain(|a| !remove.contains(&a.id));
    doc.abbreviations
        .retain(|g| !g.members.iter().any(|id| remove.contains(id)));
    let count = steps.len();
    let directions: Vec<_> = steps
        .iter()
        .map(|s| s.direction)
        .enumerate()
        .map(|(i, d)| d.unwrap_or(i as f32 * 360. / count as f32))
        .collect();
    arrange(doc, &directions);
    doc.validate()
}

pub fn shared_source(doc: &Document) -> Vec<u64> {
    let Some(first) = doc.reactions.first() else {
        return vec![];
    };
    first
        .reactants
        .iter()
        .flat_map(|p| p.atoms.iter().copied())
        .filter(|id| {
            doc.reactions
                .iter()
                .all(|r| r.reactants.iter().any(|p| p.atoms.contains(id)))
        })
        .collect()
}

pub fn arrange(doc: &mut Document, directions: &[f32]) {
    let mut source = shared_source(doc);
    if source.is_empty() {
        return;
    }
    if let Some(first) = doc.reactions.first() {
        source.extend(
            first
                .annotations
                .iter()
                .copied()
                .filter(|id| *id < first.arrow),
        );
    }
    let Some((lo, hi)) = scene::selection_bounds(doc, &source) else {
        return;
    };
    let center = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    doc.translate(&source, -center.x, -center.y);
    let half = Point::new((hi.x - lo.x) / 2., (hi.y - lo.y) / 2.);
    let gap = doc.drawing_style.bond_length_world;
    let reactions = doc.reactions.clone();
    let mut placed = Vec::new();
    for (index, r) in reactions.iter().enumerate() {
        let condition = doc
            .annotations
            .iter()
            .find(|a| a.id == r.arrow + 1)
            .map(|a| a.id);
        let mut products: Vec<_> = r
            .products
            .iter()
            .chain(&r.agents)
            .flat_map(|p| p.atoms.iter().copied())
            .collect();
        products.extend(
            r.annotations
                .iter()
                .copied()
                .filter(|id| !source.contains(id) && Some(*id) != condition),
        );
        let Some((a, b)) = scene::selection_bounds(doc, &products) else {
            continue;
        };
        let product_center = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
        let product_half = Point::new((b.x - a.x) / 2., (b.y - a.y) / 2.);
        let angle = directions.get(index).copied().unwrap_or(0.).to_radians();
        let u = Point::new(angle.cos(), angle.sin());
        let edge = |half: Point| {
            let x = if u.x.abs() < 0.001 {
                f32::INFINITY
            } else {
                half.x / u.x.abs()
            };
            let y = if u.y.abs() < 0.001 {
                f32::INFINITY
            } else {
                half.y / u.y.abs()
            };
            x.min(y)
        };
        let source_edge = edge(half);
        let product_edge = edge(product_half);
        let mut distance = source_edge + product_edge + gap * 6.;
        // Keep measured product panels separated without changing molecular scale.
        for _ in 0..32 {
            let c = Point::new(u.x * distance, u.y * distance);
            let bounds = (
                c.offset(-product_half.x - gap * 0.5, -product_half.y - gap * 0.5),
                c.offset(product_half.x + gap * 0.5, product_half.y + gap * 0.5),
            );
            if placed.iter().all(|(lo, hi): &(Point, Point)| {
                bounds.1.x < lo.x || hi.x < bounds.0.x || bounds.1.y < lo.y || hi.y < bounds.0.y
            }) {
                placed.push(bounds);
                break;
            }
            distance += gap;
        }
        doc.translate(
            &products,
            u.x * distance - product_center.x,
            u.y * distance - product_center.y,
        );
        let start = Point::new(u.x * (source_edge + gap), u.y * (source_edge + gap));
        let end = Point::new(
            u.x * (distance - product_edge - gap),
            u.y * (distance - product_edge - gap),
        );
        if let Some(arrow) = doc.arrows.iter_mut().find(|a| a.id == r.arrow) {
            arrow.start = start;
            arrow.end = end;
            arrow.control = None;
        }
        if let Some(id) = condition
            && let Some(note) = doc.annotations.iter_mut().find(|a| a.id == id)
        {
            let (w, h) = note.size();
            let normal = Point::new(u.y, -u.x);
            let offset = (normal.x.abs() * w + normal.y.abs() * h) / 2. + gap * 0.45;
            note.position = Point::new(
                (start.x + end.x) / 2. - w / 2. + normal.x * offset,
                (start.y + end.y) / 2. - h / 2. + normal.y * offset,
            );
        }
    }
}
