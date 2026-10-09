//! Persistent atom and bond paint, independent of molecular ink and selection.
use crate::{
    document::{Atom, Bond, Document, Point},
    graphics::{GraphicStyle, PathCommand},
    palette::Color,
    scene::Primitive,
};
use std::collections::{BTreeMap, HashMap, HashSet};

// Native ChemDraw 26 exports at two label sizes match the outer four-rail
// bond envelope for their atom/capsule radius. Label pills add half this radius
// around their ink and use 2:1 elliptical caps. Padding stays fixed when the
// label size changes.
const KAPPA: f32 = 0.552_284_8;
// Cubic circle segments overshoot the true radius by at most 0.0273%.
const CAPSULE_RADIUS_SCALE: f32 = 1.0003;

fn radius(doc: &Document) -> f32 {
    let style = &doc.drawing_style;
    style.bond_length_world * style.bond_spacing_ratio * 1.5 + style.line_width() / 2.
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
    radius(doc).max(envelope + style.line_width())
}

fn label_bounds(doc: &Document, atom: &Atom) -> Option<(Point, Point)> {
    let mut bounds = crate::scene::atom_label_ink_bounds(atom, doc);
    for part in crate::scientific::styled_mark_parts(atom, &doc.drawing_style) {
        let pad = part.style.width() / 2.;
        for point in part.commands.iter().flat_map(PathCommand::iter_points) {
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
    let radius = radius(doc);
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

pub(crate) fn bond_bounds(
    doc: &Document,
    bond: &Bond,
    joins: &crate::bond_joins::Joins<'_>,
) -> Option<(Point, Point)> {
    bond.highlight?;
    if !doc.bond_visible(bond.a, bond.b) {
        return None;
    }
    let a = doc.atom(bond.a)?.position;
    let b = doc.atom(bond.b)?.position;
    let radius = bond_radius(doc, bond, joins) * CAPSULE_RADIUS_SCALE;
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

fn split_polygon(points: &[Point], axis: Point, bound: f32) -> (Vec<Point>, Vec<Point>) {
    let mut negative = Vec::new();
    let mut positive = Vec::new();
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let da = a.x * axis.x + a.y * axis.y - bound;
        let db = b.x * axis.x + b.y * axis.y - bound;
        if da < 0. {
            negative.push(*a);
        } else {
            positive.push(*a);
        }
        if (da < 0.) != (db < 0.) {
            let t = da / (da - db);
            let cut = a.offset((b.x - a.x) * t, (b.y - a.y) * t);
            negative.push(cut);
            positive.push(cut);
        }
    }
    (negative, positive)
}

fn clear_labels(commands: Vec<PathCommand>, boxes: &[(Point, Point)]) -> Vec<PathCommand> {
    if boxes.is_empty() {
        return commands;
    }
    // Subtract glyph boxes from the actual capsule. Moving its round cap would
    // uncover a diagonal bond's already-contrasted stroke. Per-glyph boxes also
    // leave the gaps between stacked labels available for their visible bonds.
    // Only affected capsules use the existing curve tessellation; untouched
    // native highlights retain their original cubic geometry.
    let mut paths = crate::graphics::flattened(&commands);
    for &(lo, hi) in boxes {
        let mut outside = Vec::new();
        for mut inside in paths {
            // Four disjoint pieces outside the rectangle form a transparent
            // cutout. Keep all pieces in one color path to avoid seam overdraw.
            for (axis, bound) in [
                (Point::new(-1., 0.), -lo.x),
                (Point::new(1., 0.), hi.x),
                (Point::new(0., -1.), -lo.y),
                (Point::new(0., 1.), hi.y),
            ] {
                let (rest, part) = split_polygon(&inside, axis, bound);
                if part.len() >= 3 {
                    outside.push(part);
                }
                inside = rest;
                if inside.len() < 3 {
                    break;
                }
            }
        }
        paths = outside;
    }
    let mut out = Vec::new();
    for path in paths {
        if let Some(first) = path.first() {
            out.push(PathCommand::Move(*first));
            out.extend(path.into_iter().skip(1).map(PathCommand::Line));
            out.push(PathCommand::Close);
        }
    }
    out
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

fn append_parts(
    groups: &mut BTreeMap<(Color, u32), Vec<PathCommand>>,
    color: Color,
    parts: Vec<Primitive>,
) {
    for part in parts {
        let (alpha, part) = match part {
            Primitive::Opacity { alpha, primitive } => (alpha, *primitive),
            part => (1., part),
        };
        if let Primitive::Path { commands, .. } = part {
            groups
                .entry((color, alpha.to_bits()))
                .or_default()
                .extend(commands);
        }
    }
}
fn opacity_paths(
    groups: BTreeMap<(Color, u32), Vec<PathCommand>>,
) -> impl Iterator<Item = Primitive> {
    groups.into_iter().map(|((color, alpha), commands)| {
        let primitive = Primitive::Path {
            commands,
            style: GraphicStyle {
                stroke: color,
                fill: Some(color),
                width_pt: 0.,
                ..Default::default()
            },
            filled: true,
        };
        let alpha = f32::from_bits(alpha);
        if alpha >= 1. {
            primitive
        } else {
            Primitive::Opacity {
                alpha,
                primitive: Box::new(primitive),
            }
        }
    })
}

/// Backdrop geometry shared by the native canvas, SVG, PDF, PNG and print.
#[cfg(test)]
pub(crate) fn primitives(doc: &Document) -> Vec<Primitive> {
    primitives_with_opacity(doc, &crate::rear_opacity::Paint::new(doc))
}

pub(crate) fn primitives_with_opacity(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Primitive> {
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
        // Match the text boxes used by scene's bond-stroke clipping. Positioned
        // scientific symbols have independent geometry and are not text boxes.
        let boxes: Vec<_> = [a, b]
            .into_iter()
            .filter(|atom| opacity.atom(atom.id) > 0. && atom_color(doc, atom).is_none())
            .filter(|atom| {
                crate::atom_labels::visible(atom, doc) || doc.abbreviation(atom.id).is_some()
            })
            .flat_map(|atom| crate::scene::atom_label_ink_boxes(atom, doc))
            .collect();
        let commands = clear_labels(
            capsule(a.position, b.position, u, Point::new(-u.y, u.x)),
            &boxes,
        );
        if commands.is_empty() {
            continue;
        }
        if opacity.is_empty() {
            bonds
                .entry((color, 1_f32.to_bits()))
                .or_default()
                .extend(commands);
        } else {
            append_parts(
                &mut bonds,
                color,
                opacity.bond_parts(
                    doc,
                    bond,
                    paths(BTreeMap::from([(color, commands)])).collect(),
                ),
            );
        }
        for atom in [a, b] {
            // A nearby label cutout can remove part of an otherwise covering
            // bond cap. Retain that endpoint's own halo in this case.
            if boxes.iter().any(|(lo, hi)| {
                atom.position.distance(Point::new(
                    atom.position.x.clamp(lo.x, hi.x),
                    atom.position.y.clamp(lo.y, hi.y),
                )) <= radius * CAPSULE_RADIUS_SCALE
            }) {
                continue;
            }
            covered
                .entry((atom.id, color))
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
        if opacity.is_empty()
            && a == b
            && rx == ry
            && bonds.len() == 1
            && covered.get(&(atom.id, color)).is_some_and(|r| *r >= rx)
        {
            continue;
        }
        let commands = capsule(a, b, Point::new(rx, 0.), Point::new(0., ry));
        if opacity.is_empty() {
            atoms
                .entry((color, 1_f32.to_bits()))
                .or_default()
                .extend(commands);
        } else {
            append_parts(
                &mut atoms,
                color,
                opacity.atom_parts(
                    atom.id,
                    paths(BTreeMap::from([(color, commands)])).collect(),
                ),
            );
        }
    }
    if !opacity.is_empty() {
        for (key, commands) in atoms {
            bonds.entry(key).or_default().extend(commands);
        }
        opacity_paths(bonds).collect()
    } else {
        opacity_paths(bonds).chain(opacity_paths(atoms)).collect()
    }
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
