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
pub(crate) const CAPSULE_RADIUS_SCALE: f32 = 1.0003;

/// How atom, bond and contracted-label highlights are painted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightStyle {
    /// ChemDraw's flat highlight: a band along bonds, a pill around atoms.
    #[default]
    Flat,
    /// The flat highlight grown and blurred into a soft halo.
    Glow,
    /// A round disc behind each highlighted atom.
    Disc,
}
impl HighlightStyle {
    pub const ALL: [Self; 3] = [Self::Flat, Self::Glow, Self::Disc];
    pub fn is_flat(&self) -> bool {
        *self == Self::Flat
    }
}
impl std::fmt::Display for HighlightStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Flat => "Flat",
            Self::Glow => "Soft glow",
            Self::Disc => "Disc",
        })
    }
}
/// A disc's radius over the flat highlight's.
const DISC: f32 = 1.6;
/// A glow's spread and blur over the flat highlight's radius.
const GLOW_SPREAD: f32 = 0.4;
const GLOW_BLUR: f32 = 0.5;

pub(crate) fn radius(style: &crate::style::DrawingStyle) -> f32 {
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
    // A sharp joined corner can extend farther than the nominal bond envelope.
    // Reuse the actual bounded outline, rather than estimating its angle.
    if (joins.needed(bond) || joins.centered(bond))
        && let Some((a, b)) = doc.atom(bond.a).zip(doc.atom(bond.b))
    {
        let mut include = |points: Vec<Point>| {
            for point in points {
                envelope = envelope.max(crate::graphics::segment_distance(
                    point, a.position, b.position,
                ));
            }
        };
        if joins.needed(bond) {
            include(joins.polygon(bond, a.position, b.position));
        }
        if joins.centered(bond) {
            let length = a.position.distance(b.position).max(0.001);
            let nx = -(b.position.y - a.position.y) / length;
            let ny = (b.position.x - a.position.x) / length;
            for offset in [-spacing / 2., spacing / 2.] {
                if let Some(points) = joins.centered_polygon(
                    bond,
                    a.position.offset(nx * offset, ny * offset),
                    b.position.offset(nx * offset, ny * offset),
                    offset,
                ) {
                    include(points);
                }
            }
        }
    }
    radius(style).max(envelope + style.line_width())
}

pub(crate) fn label_bounds(doc: &Document, atom: &Atom) -> Option<(Point, Point)> {
    let mut bounds = crate::scene::atom_label_ink_bounds(atom, doc);
    for part in crate::scientific::document_mark_parts(atom, doc) {
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
    let radius = radius(&doc.drawing_style);
    label_shape_from(label_bounds(doc, atom), atom.position, radius)
}

pub(crate) fn label_shape_from(
    bounds: Option<(Point, Point)>,
    position: Point,
    radius: f32,
) -> (Point, Point, f32, f32) {
    if let Some((lo, hi)) = bounds {
        let ry = (hi.y - lo.y + radius) / 2.;
        let y = (lo.y + hi.y) / 2.;
        (Point::new(lo.x, y), Point::new(hi.x, y), ry / 2., ry)
    } else {
        (position, position, radius, radius)
    }
}

pub(crate) fn atom_bounds(doc: &Document, atom: &Atom) -> Option<(Point, Point)> {
    atom_color(doc, atom)?;
    if !doc.atom_visible(atom.id) {
        return None;
    }
    let (a, b, rx, ry) = label_shape(doc, atom);
    let style = atom_style(doc, atom);
    let reach = glow_reach(doc, style);
    if style == HighlightStyle::Disc {
        let r = disc_radius(a, b, rx, ry);
        let center = a.lerp(b, 0.5);
        return Some((center.offset(-r, -r), center.offset(r, r)));
    }
    Some((
        a.offset(-rx - reach, -ry - reach),
        b.offset(rx + reach, ry + reach),
    ))
}

/// How far a highlight's glow reaches past its flat shape, if it glows.
fn glow_reach(doc: &Document, style: HighlightStyle) -> f32 {
    if style == HighlightStyle::Glow {
        let r = radius(&doc.drawing_style);
        crate::scene::glow_reach(r * GLOW_SPREAD, r * GLOW_BLUR)
    } else {
        0.
    }
}

/// A disc around a label's pill, centered on its middle.
fn disc_radius(a: Point, b: Point, rx: f32, ry: f32) -> f32 {
    DISC * (rx + a.distance(b) / 2.).max(ry)
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
    let radius = bond_radius(doc, bond, joins) * CAPSULE_RADIUS_SCALE
        + glow_reach(doc, bond_style(doc, bond));
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
pub(crate) fn primitives_with_opacity(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Primitive> {
    if !any(doc) {
        return Vec::new();
    }
    // Styles form separate paint families. Matching colors within one family
    // still share their outline/glow; another family's cap cannot cover them.
    let joins = crate::bond_joins::Joins::new(doc);
    HighlightStyle::ALL
        .into_iter()
        .filter(|style| has_style(doc, *style))
        .flat_map(|style| primitives_for_style(doc, opacity, &joins, style))
        .collect()
}

fn primitives_for_style(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
    joins: &crate::bond_joins::Joins<'_>,
    style: HighlightStyle,
) -> Vec<Primitive> {
    let mut bonds: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut covered: HashMap<(u64, Color), f32> = HashMap::new();
    for bond in doc
        .bonds
        .iter()
        .filter(|bond| bond_style(doc, bond) == style)
    {
        let Some(color) = bond.highlight.filter(|_| doc.bond_visible(bond.a, bond.b)) else {
            continue;
        };
        let Some((a, b)) = doc.atom(bond.a).zip(doc.atom(bond.b)) else {
            continue;
        };
        let radius = bond_radius(doc, bond, joins);
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
            .filter(|atom| {
                crate::scene::label_trims_bonds(atom, doc, Some(opacity))
                    && atom_color(doc, atom).is_none()
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
        if !opacity.has_visibility() {
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
    for atom in doc
        .atoms
        .iter()
        .filter(|a| doc.atom_visible(a.id) && atom_style(doc, a) == style)
    {
        let Some(color) = atom_color(doc, atom) else {
            continue;
        };
        let (a, b, rx, ry) = label_shape(doc, atom);
        // A matching bond cap covers an unlabeled atom when no other bond
        // color can overpaint it (including unrelated crossing bonds). Avoid
        // drawing that same boundary twice and darkening its antialiased edge.
        if !opacity.has_visibility()
            && style != HighlightStyle::Disc
            && a == b
            && rx == ry
            && bonds.len() == 1
            && covered.get(&(atom.id, color)).is_some_and(|r| *r >= rx)
        {
            continue;
        }
        let commands = if style == HighlightStyle::Disc {
            let r = disc_radius(a, b, rx, ry);
            let center = a.lerp(b, 0.5);
            capsule(center, center, Point::new(r, 0.), Point::new(0., r))
        } else {
            capsule(a, b, Point::new(rx, 0.), Point::new(0., ry))
        };
        if !opacity.has_visibility() {
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
    let painted = |parts: Vec<Primitive>| glowing(doc, style, parts);
    if opacity.has_visibility() {
        // Batch translucent rear paint to avoid doubling identical endpoint
        // halos. Keep opaque front atoms above opaque bond halos across colors,
        // as in the legacy scene, instead of globally sorting their colors.
        let mut rear: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for groups in [&mut bonds, &mut atoms] {
            groups.retain(|key, commands| {
                if key.1 == 1_f32.to_bits() {
                    true
                } else {
                    rear.entry(*key).or_default().append(commands);
                    false
                }
            });
        }
        painted(
            opacity_paths(rear)
                .chain(opacity_paths(bonds))
                .chain(opacity_paths(atoms))
                .collect(),
        )
    } else {
        painted(opacity_paths(bonds).chain(opacity_paths(atoms)).collect())
    }
}

/// The highlight paths as soft glows when the drawing asks for them. Bonds
/// and atoms of one color glow as one shape, so where they meet the glow is
/// no brighter; faded rear paint keeps its own glow.
fn glowing(doc: &Document, style: HighlightStyle, parts: Vec<Primitive>) -> Vec<Primitive> {
    if style != HighlightStyle::Glow {
        return parts;
    }
    let r = radius(&doc.drawing_style);
    let glow = |commands, color| {
        crate::scene::glow_primitive(commands, color, r * GLOW_SPREAD, r * GLOW_BLUR, doc)
    };
    let mut out = Vec::new();
    let mut opaque: Vec<(Color, Vec<PathCommand>)> = Vec::new();
    for part in parts {
        match part {
            Primitive::Path {
                commands, style, ..
            } => {
                let color = style.fill.unwrap_or(style.stroke);
                match opaque.iter_mut().find(|(c, _)| *c == color) {
                    Some((_, all)) => all.extend(commands),
                    None => opaque.push((color, commands)),
                }
            }
            Primitive::Opacity { alpha, primitive } => match *primitive {
                Primitive::Path {
                    commands, style, ..
                } => out.push(Primitive::Opacity {
                    alpha,
                    primitive: Box::new(glow(commands, style.fill.unwrap_or(style.stroke))),
                }),
                primitive => out.push(Primitive::Opacity {
                    alpha,
                    primitive: Box::new(primitive),
                }),
            },
            part => out.push(part),
        }
    }
    out.extend(
        opaque
            .into_iter()
            .map(|(color, commands)| glow(commands, color)),
    );
    out
}

/// Whether a drawing carries any highlight paint, including collapsed members.
pub fn any(doc: &Document) -> bool {
    doc.atoms.iter().any(|a| a.display.highlight.is_some())
        || doc.bonds.iter().any(|b| b.highlight.is_some())
        || doc.abbreviations.iter().any(|g| g.highlight.is_some())
}

pub(crate) fn has_overrides(doc: &Document) -> bool {
    doc.atoms
        .iter()
        .any(|a| a.display.highlight_style.is_some())
        || doc.bonds.iter().any(|b| b.highlight_style.is_some())
        || doc
            .abbreviations
            .iter()
            .any(|g| g.highlight_style.is_some())
}

/// Paint stored with this style, including hidden abbreviation members.
pub(crate) fn has_style(doc: &Document, style: HighlightStyle) -> bool {
    doc.atoms.iter().any(|a| {
        a.display.highlight.is_some()
            && a.display.highlight_style.unwrap_or(doc.highlight_style) == style
    }) || doc
        .bonds
        .iter()
        .any(|b| b.highlight.is_some() && bond_style(doc, b) == style)
        || doc.abbreviations.iter().any(|g| {
            g.highlight.is_some() && g.highlight_style.unwrap_or(doc.highlight_style) == style
        })
}

/// Apply an explicit style to every highlight and to future paint. Returns
/// whether the drawing changed, including resetting imported overrides when
/// the chosen style is already the default.
pub fn set_style(doc: &mut Document, style: HighlightStyle) -> bool {
    let changed = doc.highlight_style != style || has_overrides(doc);
    doc.highlight_style = style;
    for atom in &mut doc.atoms {
        atom.display.highlight_style = None;
    }
    for bond in &mut doc.bonds {
        bond.highlight_style = None;
    }
    for group in &mut doc.abbreviations {
        group.highlight_style = None;
    }
    changed
}

/// Freeze only existing paint whose inherited default would change on append.
/// Unpainted imported objects use the destination default for future paint.
pub(crate) fn preserve_style(part: &mut Document, destination: HighlightStyle) {
    if part.highlight_style == destination {
        return;
    }
    for atom in &mut part.atoms {
        if atom.display.highlight.is_some() {
            atom.display
                .highlight_style
                .get_or_insert(part.highlight_style);
        }
    }
    for bond in &mut part.bonds {
        if bond.highlight.is_some() {
            bond.highlight_style.get_or_insert(part.highlight_style);
        }
    }
    for group in &mut part.abbreviations {
        if group.highlight.is_some() {
            group.highlight_style.get_or_insert(part.highlight_style);
        }
    }
}

/// A contracted label can have its own paint without recoloring its anchor
/// inside the expanded fragment.
pub(crate) fn atom_color(doc: &Document, atom: &Atom) -> Option<Color> {
    doc.abbreviation(atom.id)
        .and_then(|group| group.highlight)
        .or(atom.display.highlight)
}

/// Use the style of the same paint source as `atom_color`: a separately
/// painted contracted label takes precedence over the anchor's hidden paint.
pub(crate) fn atom_style(doc: &Document, atom: &Atom) -> HighlightStyle {
    atom_style_override(doc, atom).unwrap_or(doc.highlight_style)
}

pub(crate) fn atom_style_override(doc: &Document, atom: &Atom) -> Option<HighlightStyle> {
    doc.abbreviation(atom.id)
        .filter(|group| group.highlight.is_some())
        .map_or(atom.display.highlight_style, |group| group.highlight_style)
}

fn bond_style(doc: &Document, bond: &Bond) -> HighlightStyle {
    bond.highlight_style.unwrap_or(doc.highlight_style)
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
    let mut contracted = HashMap::new();
    for group in &mut doc.abbreviations {
        if group.members.iter().any(|id| selected.contains(id)) {
            group.highlight = color;
            if color.is_none() {
                group.highlight_style = None;
            }
            contracted.extend(
                group
                    .members
                    .iter()
                    .map(|&id| (id, (group.anchor, group.highlight_style))),
            );
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
                || !contracted.contains_key(&atom.id))
        {
            if atom.display.highlight.is_none() {
                atom.display.highlight_style =
                    contracted.get(&atom.id).and_then(|(_, style)| *style);
            }
            atom.display.highlight = color;
            if color.is_none() {
                atom.display.highlight_style = None;
            }
        }
    }
    for bond in &mut doc.bonds {
        if selected.contains(&bond.a)
            && selected.contains(&bond.b)
            && (color.is_none()
                || bond.highlight.is_none()
                || visible_bonds.contains(&(bond.a, bond.b)))
        {
            if bond.highlight.is_none() {
                bond.highlight_style = contracted.get(&bond.a).and_then(|(anchor, style)| {
                    contracted
                        .get(&bond.b)
                        .filter(|(other, _)| other == anchor)
                        .and(*style)
                });
            }
            bond.highlight = color;
            if color.is_none() {
                bond.highlight_style = None;
            }
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
        .filter_map(|atom| {
            Some((
                *mapping.get(&atom.id)?,
                atom.display.highlight?,
                atom.display.highlight_style,
            ))
        })
        .collect();
    for (id, color, style) in atoms {
        if let Some(atom) = doc.atom_mut(id)
            && atom.display.highlight.is_none()
        {
            atom.display.highlight = Some(color);
            atom.display.highlight_style = style;
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
                bond.highlight_style,
            ))
        })
        .collect();
    for (a, b, color, style) in bonds {
        if let Some(bond) = doc
            .bonds
            .iter_mut()
            .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
            && bond.highlight.is_none()
        {
            bond.highlight = Some(color);
            bond.highlight_style = style;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recoloring_an_imported_label_keeps_its_style_and_its_members_own_paint() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        let c = doc.add_atom("C", Point::new(84., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        let red = Color::Custom([255, 0, 0]);
        let blue = Color::Custom([0, 0, 255]);
        doc.atoms[0].display.highlight = Some(red);
        doc.atoms[0].display.highlight_style = Some(HighlightStyle::Glow);
        doc.bonds[0].highlight = Some(red);
        doc.bonds[0].highlight_style = Some(HighlightStyle::Flat);
        doc.abbreviations.push(crate::abbreviations::Abbreviation {
            label_style: None,
            label_color_override: false,
            highlight: Some(red),
            highlight_style: Some(HighlightStyle::Disc),
            label: "Pr".into(),
            reverse_label: String::new(),
            anchor: a,
            exit: None,
            members: vec![a, b, c],
            alignment: Default::default(),
        });
        apply(&mut doc, &[a], Some(blue));
        assert_eq!(atom_color(&doc, &doc.atoms[0]), Some(blue));
        assert_eq!(atom_style(&doc, &doc.atoms[0]), HighlightStyle::Disc);
        assert_eq!(doc.atoms[0].display.highlight, Some(red));
        assert_eq!(
            doc.atoms[0].display.highlight_style,
            Some(HighlightStyle::Glow)
        );
        assert_eq!(
            doc.atoms[1].display.highlight_style,
            Some(HighlightStyle::Disc)
        );
        assert_eq!(doc.bonds[0].highlight_style, Some(HighlightStyle::Flat));
        assert_eq!(doc.bonds[1].highlight_style, Some(HighlightStyle::Disc));
        apply(&mut doc, &[a], None);
        assert!(!any(&doc));
        assert!(!has_overrides(&doc));
    }

    #[test]
    fn mixed_styles_keep_their_geometry_and_effects_even_when_their_colors_match() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        let c = doc.add_atom("C", Point::new(84., 0.));
        let d = doc.add_atom("C", Point::new(126., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        let color = Color::Custom([10, 20, 30]);
        apply(&mut doc, &[a, b, c, d], Some(color));
        doc.atom_mut(c).unwrap().display.highlight = None;
        doc.atom_mut(a).unwrap().display.highlight_style = Some(HighlightStyle::Disc);
        doc.atom_mut(b).unwrap().display.highlight_style = Some(HighlightStyle::Glow);
        doc.bonds[1].highlight_style = Some(HighlightStyle::Glow);
        let paint = primitives_with_opacity(&doc, &crate::rear_opacity::Paint::new(&doc));
        assert_eq!(
            paint
                .iter()
                .filter(|p| matches!(p, Primitive::Glow { .. }))
                .count(),
            1
        );
        assert_eq!(
            paint
                .iter()
                .filter(|p| matches!(p, Primitive::Path { .. }))
                .count(),
            3
        );
        let Some(Primitive::Path { commands, .. }) = paint.last() else {
            panic!("the disc remains above the flat bond's matching cap");
        };
        let r = radius(&doc.drawing_style);
        let reach = commands
            .iter()
            .flat_map(PathCommand::points)
            .map(|p| p.distance(Point::default()))
            .fold(0., f32::max);
        assert!(reach > DISC * 0.99 * r);
        let (disc_lo, _) = atom_bounds(&doc, doc.atom(a).unwrap()).unwrap();
        assert!((disc_lo.x + DISC * r).abs() < 0.001);
        let (glow_lo, _) = atom_bounds(&doc, doc.atom(b).unwrap()).unwrap();
        assert!((glow_lo.x - (42. - r - glow_reach(&doc, HighlightStyle::Glow))).abs() < 0.001);
        let joins = crate::bond_joins::Joins::new(&doc);
        let (flat_lo, _) = bond_bounds(&doc, &doc.bonds[0], &joins).unwrap();
        let (glowing_lo, _) = bond_bounds(&doc, &doc.bonds[1], &joins).unwrap();
        assert!(glowing_lo.y < flat_lo.y);
        assert!(crate::graphics::tiled(&doc));
        assert_eq!(
            crate::graphics::export_notice(&doc),
            [crate::graphics::GLOW_NOTICE]
        );
        assert!(crate::scene::svg(&doc).contains("<feGaussianBlur"));

        let mut only_flat = Document {
            highlight_style: HighlightStyle::Glow,
            ..Default::default()
        };
        let a = only_flat.add_atom("C", Point::default());
        only_flat.atom_mut(a).unwrap().display.highlight = Some(color);
        only_flat.atom_mut(a).unwrap().display.highlight_style = Some(HighlightStyle::Flat);
        assert!(!crate::graphics::tiled(&only_flat));
        assert!(crate::graphics::export_notice(&only_flat).is_empty());
    }

    #[test]
    fn native_style_overrides_are_optional_and_reset_with_explicit_appearance_edits() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let color = Color::Custom([10, 20, 30]);
        apply(&mut doc, &[a, b], Some(color));
        let legacy = doc.file_json().unwrap();
        assert!(!String::from_utf8_lossy(&legacy).contains("highlight_style"));
        let reopened = Document::from_native_file(&legacy).unwrap();
        assert_eq!(reopened.file_json().unwrap(), legacy);
        assert!(!has_overrides(&reopened));

        doc.atoms[0].display.highlight_style = Some(HighlightStyle::Disc);
        doc.bonds[0].highlight_style = Some(HighlightStyle::Glow);
        apply(&mut doc, &[a, b], Some(Color::Custom([40, 50, 60])));
        assert_eq!(
            doc.atoms[0].display.highlight_style,
            Some(HighlightStyle::Disc)
        );
        assert_eq!(doc.bonds[0].highlight_style, Some(HighlightStyle::Glow));
        assert!(set_style(&mut doc, HighlightStyle::Flat));
        assert!(!has_overrides(&doc));
        assert!(!set_style(&mut doc, HighlightStyle::Flat));
        doc.atoms[0].display.highlight_style = Some(HighlightStyle::Disc);
        doc.bonds[0].highlight_style = Some(HighlightStyle::Glow);
        apply(&mut doc, &[a, b], None);
        assert!(!has_overrides(&doc));
        apply(&mut doc, &[a, b], Some(color));
        assert_eq!(atom_style(&doc, &doc.atoms[0]), HighlightStyle::Flat);
        assert_eq!(bond_style(&doc, &doc.bonds[0]), HighlightStyle::Flat);

        let mut invalid = serde_json::to_value(&doc).unwrap();
        invalid["atoms"][0]["display"]["highlight_style"] = "unsupported".into();
        assert!(serde_json::from_value::<Document>(invalid).is_err());
    }

    #[test]
    fn the_drawing_paints_every_highlight_flat_as_a_glow_or_as_a_disc() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let blue = Color::Custom([0, 0, 255]);
        doc.bonds[0].highlight = Some(blue);
        doc.atoms[0].display.highlight = Some(blue);
        let paint =
            |doc: &Document| primitives_with_opacity(doc, &crate::rear_opacity::Paint::new(doc));
        assert!(
            paint(&doc)
                .iter()
                .all(|p| matches!(p, Primitive::Path { .. }))
        );
        assert_eq!(doc.file_version(), Document::default().file_version());

        doc.highlight_style = HighlightStyle::Glow;
        let glows = paint(&doc);
        assert!(!glows.is_empty());
        for glow in &glows {
            let Primitive::Glow {
                color,
                spread,
                blur,
                ..
            } = glow
            else {
                panic!("a glow: {glow:?}");
            };
            assert_eq!(*color, [0, 0, 255]);
            assert!(*spread > 0. && *blur > 0.);
        }
        // Export leaves room for the glow's soft edge.
        let (lo, _) = crate::scene::bounds(&glows);
        assert!(lo.x < -radius(&doc.drawing_style) * 1.5, "{lo:?}");
        let svg = crate::scene::svg(&doc);
        assert!(svg.contains("<feGaussianBlur"), "{svg}");
        assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);

        doc.highlight_style = HighlightStyle::Disc;
        // Atoms paint after bonds, so the disc is the last path.
        let Some(Primitive::Path { commands, .. }) = paint(&doc).pop() else {
            panic!("a disc");
        };
        let disc = commands
            .iter()
            .flat_map(PathCommand::points)
            .map(|p| p.distance(Point::default()))
            .fold(0., f32::max);
        // The atom's disc reaches past the flat highlight's pill.
        assert!(disc > DISC * 0.99 * radius(&doc.drawing_style), "{disc}");
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["highlight_style"], "disc");
        let back: Document = serde_json::from_value(json).unwrap();
        assert_eq!(back.highlight_style, HighlightStyle::Disc);
        assert_eq!(
            crate::graphics::export_notice(&doc),
            [crate::graphics::GLOW_NOTICE]
        );
    }

    #[test]
    fn centered_double_highlight_contains_joined_rail_corners() {
        let mut doc = Document::default();
        doc.drawing_style.bond_spacing_ratio = 0.05;
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 2, "plain");
        doc.bonds[0].double_position = crate::bonds::DoublePosition::Center;
        doc.bonds[0].highlight = Some(Color::Custom([255, 220, 0]));
        for degrees in [-145_f32, 145.] {
            let angle = degrees.to_radians();
            let c = doc.add_atom("C", Point::new(angle.cos() * 42., angle.sin() * 42.));
            doc.add_bond(a, c, 1, "plain");
        }
        let bond = &doc.bonds[0];
        let joins = crate::bond_joins::Joins::new(&doc);
        let (lo, hi) = bond_bounds(&doc, bond, &joins).unwrap();
        let paint = primitives_with_opacity(&doc, &crate::rear_opacity::Paint::new(&doc));
        let Primitive::Path { commands, .. } = &paint[0] else {
            panic!("Expected a highlight capsule");
        };
        let PathCommand::Move(top) = commands[0] else {
            panic!("Expected the capsule's top edge");
        };
        let radius = -top.y;
        let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
        for offset in [-spacing / 2., spacing / 2.] {
            let points = joins
                .centered_polygon(
                    bond,
                    Point::new(0., offset),
                    Point::new(42., offset),
                    offset,
                )
                .unwrap();
            assert!(
                points
                    .iter()
                    .any(|p| p.x < -4. && p.distance(Point::default()) > 4.6)
            );
            for point in points {
                assert!(
                    crate::graphics::segment_distance(point, Point::default(), Point::new(42., 0.))
                        < radius
                );
                assert!(point.x >= lo.x && point.x <= hi.x && point.y >= lo.y && point.y <= hi.y);
            }
        }
    }
}
