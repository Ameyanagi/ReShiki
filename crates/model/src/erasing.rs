//! Erase the geometry crossed by a pointer stroke, including between motion events.
use crate::{
    document::{Document, Point},
    graphics::{PathCommand, flattened},
};

fn distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    let t = if length > 0.000001 {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / length
    } else {
        0.
    };
    p.distance(a.offset(dx * t.clamp(0., 1.), dy * t.clamp(0., 1.)))
}
fn crossed(a: Point, b: Point, c: Point, d: Point, radius: f32) -> bool {
    if distance(a, c, d)
        .min(distance(b, c, d))
        .min(distance(c, a, b))
        .min(distance(d, a, b))
        <= radius
    {
        return true;
    }
    let (ux, uy, vx, vy) = (b.x - a.x, b.y - a.y, d.x - c.x, d.y - c.y);
    let det = ux * vy - uy * vx;
    if det.abs() < 0.000001 {
        return false;
    }
    let t = ((c.x - a.x) * vy - (c.y - a.y) * vx) / det;
    let s = ((c.x - a.x) * uy - (c.y - a.y) * ux) / det;
    (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s)
}
fn path_hit(commands: &[PathCommand], from: Point, to: Point, radius: f32) -> bool {
    flattened(commands).iter().any(|path| {
        path.windows(2)
            .any(|pair| matches!(pair, [a, b] if crossed(from, to, *a, *b, radius)))
    })
}
fn box_hit(lo: Point, hi: Point, from: Point, to: Point, radius: f32) -> bool {
    let inside = |p: Point| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    inside(from)
        || inside(to)
        || [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y), lo]
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if crossed(from, to, *a, *b, radius)))
}

pub fn stroke(doc: &mut Document, from: Point, to: Point, radius: f32) {
    let mut ids: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| doc.atom_visible(a.id) && distance(a.position, from, to) <= radius)
        .map(|a| a.id)
        .collect();
    // Atom labels and attached marks belong to the atom, even away from its center.
    for atom in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        if crate::scene::atom_label_bounds(atom, doc)
            .is_some_and(|(lo, hi)| box_hit(lo, hi, from, to, radius))
            || crate::scientific::styled_mark_parts(atom, &doc.drawing_style)
                .iter()
                .any(|part| path_hit(&part.commands, from, to, radius))
        {
            ids.push(atom.id);
        }
    }
    ids.extend(
        doc.annotations
            .iter()
            .filter(|a| {
                let (w, h) = a.size();
                box_hit(a.position, a.position.offset(w, h), from, to, radius)
            })
            .map(|a| a.id),
    );
    ids.extend(
        doc.arrows
            .iter()
            .filter(|a| {
                a.paths().iter().any(|part| {
                    path_hit(&part.commands, from, to, radius + part.style.width() / 2.)
                })
            })
            .map(|a| a.id),
    );
    ids.extend(
        doc.graphics
            .iter()
            .filter(|g| {
                g.hit(from, radius)
                    || g.hit(to, radius)
                    || g.parts().iter().any(|part| {
                        path_hit(&part.commands, from, to, radius + part.style.width() / 2.)
                    })
            })
            .map(|g| g.id),
    );
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| doc.bond_visible(b.a, b.b))
        .filter(|b| {
            doc.atom(b.a)
                .zip(doc.atom(b.b))
                .is_some_and(|(a, b)| crossed(from, to, a.position, b.position, radius))
        })
        .map(|b| (b.a, b.b))
        .collect();
    if !ids.is_empty() {
        doc.delete(&ids);
    }
    if !bonds.is_empty() {
        let affected: Vec<_> = bonds.iter().flat_map(|(a, b)| [*a, *b]).collect();
        doc.invalidate_chemistry(&affected);
        doc.bonds.retain(|b| !bonds.contains(&(b.a, b.b)));
        crate::ring_fills::prune(doc);
    }
}

#[cfg(test)]
mod tests;
