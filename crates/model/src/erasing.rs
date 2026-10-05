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
mod tests {
    use super::*;
    #[test]
    fn fast_stroke_cuts_both_ring_edges_without_deleting_off_path_atoms() {
        let mut doc = Document::default();
        crate::editing::ring(&mut doc, Point::default(), 6, false, 0.);
        let atoms = doc.atoms.clone();
        stroke(&mut doc, Point::new(0., 100.), Point::new(0., -100.), 2.);
        assert_eq!(doc.atoms, atoms);
        assert_eq!(doc.bonds.len(), 4);
        doc.validate().unwrap();
    }
    #[test]
    fn stroke_removes_crossed_atoms_and_objects_but_preserves_nearby_geometry() {
        use crate::{
            arrows::{ArrowStyle, Preset},
            document::{Annotation, Arrow},
            graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
        };
        let mut doc = Document::default();
        let crossed = doc.add_atom("N", Point::new(0., 0.));
        let untouched = doc.add_atom("O", Point::new(30., 0.));
        doc.add_bond(crossed, untouched, 1, "plain");
        doc.arrows.push(Arrow::new(
            10,
            Point::new(-40., 30.),
            Point::new(40., 30.),
            Preset::Forward,
            ArrowStyle::default(),
        ));
        doc.annotations.push(Annotation {
            id: 11,
            position: Point::new(-10., 50.),
            text: "label".into(),
            format: Default::default(),
        });
        doc.graphics.push(Graphic::dragged(
            12,
            GraphicKind::Rectangle,
            Point::new(-20., 80.),
            Point::new(20., 120.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        ));
        stroke(&mut doc, Point::new(0., -30.), Point::new(0., 150.), 2.);
        assert!(doc.atom(crossed).is_none());
        assert!(doc.atom(untouched).is_some());
        assert!(
            doc.bonds.is_empty()
                && doc.arrows.is_empty()
                && doc.annotations.is_empty()
                && doc.graphics.is_empty()
        );
        doc.validate().unwrap();
    }
    #[test]
    fn parallel_and_degenerate_strokes_do_not_erase_distant_segments() {
        assert!(!crossed(
            Point::new(0., 0.),
            Point::new(0., 0.),
            Point::new(30., 0.),
            Point::new(60., 0.),
            3.
        ));
        assert!(!crossed(
            Point::new(0., 0.),
            Point::new(60., 0.),
            Point::new(0., 10.),
            Point::new(60., 10.),
            3.
        ));
        assert!(crossed(
            Point::new(0., 0.),
            Point::new(60., 0.),
            Point::new(30., -50.),
            Point::new(30., 50.),
            0.
        ));
    }
}
