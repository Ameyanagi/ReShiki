//! Persistent atom and bond paint, independent of molecular ink and selection.
use crate::{
    document::{Atom, Bond, Document, Point},
    graphics::{GraphicStyle, PathCommand},
    palette::Color,
    scene::Primitive,
};
use std::collections::{BTreeMap, HashMap, HashSet};

// Native ChemDraw 26 SVG at Arial 10 pt has a 3.092 pt atom/capsule radius.
// A label adds half that radius above/below its ink, with 2:1 elliptical caps.
// Scale with label size and widen only when the molecular strokes require it.
const ATOM_RADIUS_EM: f32 = 0.3092;
const KAPPA: f32 = 0.552_284_8;

fn radius(doc: &Document, atom: Option<&Atom>) -> f32 {
    doc.drawing_style.world(
        atom.and_then(|a| a.text_style.as_ref())
            .map_or(doc.drawing_style.font_size_pt, |style| style.size_pt),
    ) * ATOM_RADIUS_EM
}

fn bond_radius(doc: &Document, bond: &Bond, joins: &crate::bond_joins::Joins<'_>) -> f32 {
    let style = &doc.drawing_style;
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let offset = match bond.order {
        2 | 7 => match crate::scene::effective_double_position(doc, bond) {
            crate::bonds::DoublePosition::Center => spacing / 2.,
            _ => spacing,
        },
        3 | 4 => spacing,
        6 => spacing * 1.5,
        _ => 0.,
    };
    let ink = if matches!(
        bond.display.as_str(),
        "bold" | "wedge" | "hash" | "hashed" | "hollow_wedge"
    ) || bond.secondary_display.as_deref() == Some("bold")
    {
        style.world(style.bold_width_pt) / 2.
    } else {
        style.line_width() / 2.
    };
    let wave = if bond.display == "wavy" && bond.order != 2 {
        style.line_width() * 1.25
    } else {
        0.
    };
    let mut envelope = offset.max(wave) + ink;
    // A sharp joined corner can extend farther than half the bold width.
    // Reuse the actual bounded outline, rather than estimating its angle.
    if joins.needed(bond)
        && let Some((a, b)) = doc.atom(bond.a).zip(doc.atom(bond.b))
    {
        for point in joins.polygon(bond, a.position, b.position) {
            envelope = envelope.max(crate::graphics::segment_distance(
                point, a.position, b.position,
            ));
        }
    }
    radius(doc, None).max(envelope + style.line_width())
}

fn label_bounds(doc: &Document, atom: &Atom) -> Option<(Point, Point)> {
    let mut bounds = crate::scene::atom_label_ink_bounds(atom, doc);
    for part in crate::scientific::styled_mark_parts(atom, &doc.drawing_style) {
        let pad = part.style.width() / 2.;
        for point in part.commands.iter().flat_map(PathCommand::points) {
            let a = point.offset(-pad, -pad);
            let b = point.offset(pad, pad);
            bounds = Some(bounds.map_or((a, b), |(lo, hi)| {
                (
                    Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                    Point::new(hi.x.max(b.x), hi.y.max(b.y)),
                )
            }));
        }
    }
    bounds
}

/// Elliptical label caps are centered on the left and right ink bounds.
fn label_shape(doc: &Document, atom: &Atom) -> (Point, Point, f32, f32) {
    let radius = radius(doc, Some(atom));
    if let Some((lo, hi)) = label_bounds(doc, atom) {
        let ry = (hi.y - lo.y + radius) / 2.;
        let y = (lo.y + hi.y) / 2.;
        (Point::new(lo.x, y), Point::new(hi.x, y), ry / 2., ry)
    } else {
        (atom.position, atom.position, radius, radius)
    }
}

pub(crate) fn atom_bounds(doc: &Document, atom: &Atom) -> Option<(Point, Point)> {
    atom_color(doc, atom)?;
    if !doc.atom_visible(atom.id) {
        return None;
    }
    let (a, b, rx, ry) = label_shape(doc, atom);
    Some((a.offset(-rx, -ry), b.offset(rx, ry)))
}

pub(crate) fn bond_bounds(doc: &Document, bond: &Bond) -> Option<(Point, Point)> {
    bond.highlight?;
    if !doc.bond_visible(bond.a, bond.b) {
        return None;
    }
    let a = doc.atom(bond.a)?.position;
    let b = doc.atom(bond.b)?.position;
    // The cubic circle approximation can extend 0.0273% beyond its radius.
    let radius = bond_radius(doc, bond, &crate::bond_joins::Joins::new(doc)) * 1.0003;
    Some((
        Point::new(a.x.min(b.x) - radius, a.y.min(b.y) - radius),
        Point::new(a.x.max(b.x) + radius, a.y.max(b.y) + radius),
    ))
}

/// A capsule with its cap axes in u/v directions. Degenerate endpoints make
/// one ellipse; filled paths give identical geometry in all figure backends.
fn capsule(a: Point, b: Point, u: Point, v: Point) -> Vec<PathCommand> {
    let p = |center: Point, x: f32, y: f32| center.offset(u.x * x + v.x * y, u.y * x + v.y * y);
    vec![
        PathCommand::Move(p(a, 0., -1.)),
        PathCommand::Line(p(b, 0., -1.)),
        PathCommand::Cubic(p(b, KAPPA, -1.), p(b, 1., -KAPPA), p(b, 1., 0.)),
        PathCommand::Cubic(p(b, 1., KAPPA), p(b, KAPPA, 1.), p(b, 0., 1.)),
        PathCommand::Line(p(a, 0., 1.)),
        PathCommand::Cubic(p(a, -KAPPA, 1.), p(a, -1., KAPPA), p(a, -1., 0.)),
        PathCommand::Cubic(p(a, -1., -KAPPA), p(a, -KAPPA, -1.), p(a, 0., -1.)),
        PathCommand::Close,
    ]
}

fn paths(groups: BTreeMap<Color, Vec<PathCommand>>) -> impl Iterator<Item = Primitive> {
    groups.into_iter().map(|(color, commands)| Primitive::Path {
        commands,
        style: GraphicStyle {
            stroke: color,
            fill: Some(color),
            width_pt: 0.,
            ..Default::default()
        },
        filled: true,
    })
}

/// Backdrop geometry shared by the native canvas, SVG, PDF, PNG and print.
pub(crate) fn primitives(doc: &Document) -> Vec<Primitive> {
    if !any(doc) {
        return Vec::new();
    }
    let mut bonds: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut covered: HashMap<(u64, Color), f32> = HashMap::new();
    let joins = crate::bond_joins::Joins::new(doc);
    for bond in &doc.bonds {
        let Some(color) = bond.highlight.filter(|_| doc.bond_visible(bond.a, bond.b)) else {
            continue;
        };
        let Some((a, b)) = doc.atom(bond.a).zip(doc.atom(bond.b)) else {
            continue;
        };
        let radius = bond_radius(doc, bond, &joins);
        let length = a.position.distance(b.position);
        let u = if length > 0.001 {
            Point::new(
                (b.position.x - a.position.x) / length * radius,
                (b.position.y - a.position.y) / length * radius,
            )
        } else {
            Point::new(radius, 0.)
        };
        bonds.entry(color).or_default().extend(capsule(
            a.position,
            b.position,
            u,
            Point::new(-u.y, u.x),
        ));
        for id in [bond.a, bond.b] {
            covered
                .entry((id, color))
                .and_modify(|r| *r = r.max(radius))
                .or_insert(radius);
        }
    }
    let mut atoms: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for atom in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        let Some(color) = atom_color(doc, atom) else {
            continue;
        };
        let (a, b, rx, ry) = label_shape(doc, atom);
        // A matching bond cap covers an unlabeled atom when no other bond
        // color can overpaint it (including unrelated crossing bonds). Avoid
        // drawing that same boundary twice and darkening its antialiased edge.
        if a == b
            && rx == ry
            && bonds.len() == 1
            && covered.get(&(atom.id, color)).is_some_and(|r| *r >= rx)
        {
            continue;
        }
        atoms.entry(color).or_default().extend(capsule(
            a,
            b,
            Point::new(rx, 0.),
            Point::new(0., ry),
        ));
    }
    paths(bonds).chain(paths(atoms)).collect()
}

/// Whether a drawing carries any highlight paint, including collapsed members.
pub fn any(doc: &Document) -> bool {
    doc.atoms.iter().any(|a| a.display.highlight.is_some())
        || doc.bonds.iter().any(|b| b.highlight.is_some())
        || doc.abbreviations.iter().any(|g| g.highlight.is_some())
}

/// A contracted label can have its own paint without recoloring its anchor
/// inside the expanded fragment.
pub(crate) fn atom_color(doc: &Document, atom: &Atom) -> Option<Color> {
    doc.abbreviation(atom.id)
        .and_then(|group| group.highlight)
        .or(atom.display.highlight)
}

/// Visible highlight colors for the selected objects. Hidden abbreviation
/// members retain their own colors without making the label's swatch mixed.
pub fn selected_colors(doc: &Document, ids: &[u64]) -> Vec<Option<Color>> {
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    doc.atoms
        .iter()
        .filter(|a| selected.contains(&a.id) && doc.atom_visible(a.id))
        .map(|a| atom_color(doc, a))
        .chain(
            doc.bonds
                .iter()
                .filter(|b| {
                    selected.contains(&b.a) && selected.contains(&b.b) && doc.bond_visible(b.a, b.b)
                })
                .map(|b| b.highlight),
        )
        .collect()
}

/// Apply or clear paint on selected atoms and bonds connecting them. Contracted
/// labels propagate new paint to unpainted internal members while preserving
/// their existing colors; the visible label has independent highlight paint.
/// Clearing a selected group removes all its highlights, including its members.
pub fn apply(doc: &mut Document, ids: &[u64], color: Option<Color>) -> usize {
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    let mut contracted = HashSet::new();
    for group in &mut doc.abbreviations {
        if group.members.iter().any(|id| selected.contains(id)) {
            group.highlight = color;
            contracted.extend(group.members.iter().copied());
        }
    }
    let visible_atoms: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| selected.contains(&a.id) && doc.atom_visible(a.id))
        .map(|a| a.id)
        .collect();
    let visible_bonds: HashSet<_> = doc
        .bonds
        .iter()
        .filter(|b| {
            selected.contains(&b.a) && selected.contains(&b.b) && doc.bond_visible(b.a, b.b)
        })
        .map(|b| (b.a, b.b))
        .collect();
    for atom in &mut doc.atoms {
        if selected.contains(&atom.id)
            && (color.is_none()
                || atom.display.highlight.is_none()
                || !contracted.contains(&atom.id))
        {
            atom.display.highlight = color;
        }
    }
    for bond in &mut doc.bonds {
        if selected.contains(&bond.a)
            && selected.contains(&bond.b)
            && (color.is_none()
                || bond.highlight.is_none()
                || visible_bonds.contains(&(bond.a, bond.b)))
        {
            bond.highlight = color;
        }
    }
    visible_atoms.len() + visible_bonds.len()
}

/// A shared atom/bond keeps the destination's paint when both were painted;
/// otherwise it inherits the source paint before that duplicate is removed.
pub(crate) fn inherit_merged(doc: &mut Document, mapping: &HashMap<u64, u64>) {
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .filter_map(|atom| Some((*mapping.get(&atom.id)?, atom.display.highlight?)))
        .collect();
    for (id, color) in atoms {
        if let Some(atom) = doc.atom_mut(id) {
            atom.display.highlight.get_or_insert(color);
        }
    }
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|bond| {
            Some((
                *mapping.get(&bond.a)?,
                *mapping.get(&bond.b)?,
                bond.highlight?,
            ))
        })
        .collect();
    for (a, b, color) in bonds {
        if let Some(bond) = doc
            .bonds
            .iter_mut()
            .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
        {
            bond.highlight.get_or_insert(color);
        }
    }
}
