//! Ring interior paint follows chemical atom identities, never a detached shape.
use crate::{
    document::{Document, Point},
    graphics::{GraphicStyle, PathCommand},
    palette::{Color, Hue, Row, StoredColor},
    scene::Primitive,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "StoredRingFill")]
pub struct RingFill {
    /// Consecutive vertices of a closed molecular cycle.
    pub atoms: Vec<u64>,
    /// Palette tints follow the theme; imported and pasted colors are custom.
    pub color: Color,
    pub effect: RingEffect,
}
/// How a ring fill paints its color.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingEffect {
    #[default]
    Flat,
    /// The paper color at the middle of the ring into its color at the bonds.
    Gradient,
    /// The color at half strength, so what lies behind shows through.
    SeeThrough,
    /// A soft halo of the color around the ring.
    Glow,
    /// A round disc of the color behind the ring, reaching past its bonds.
    Disc,
}
impl RingEffect {
    pub const ALL: [Self; 5] = [
        Self::Flat,
        Self::Gradient,
        Self::SeeThrough,
        Self::Glow,
        Self::Disc,
    ];
}
impl std::fmt::Display for RingEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Flat => "Flat",
            Self::Gradient => "Gradient",
            Self::SeeThrough => "Opacity",
            Self::Glow => "Soft glow",
            Self::Disc => "Disc",
        })
    }
}
/// Earlier files stored a legacy key or, with `fixed_color`, an exact color.
#[derive(Deserialize)]
struct StoredRingFill {
    atoms: Vec<u64>,
    color: StoredColor,
    #[serde(default)]
    fixed_color: bool,
    #[serde(default)]
    effect: RingEffect,
}
impl Serialize for RingFill {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // Custom colors keep the `fixed_color` form that earlier versions read;
        // Ink and palette tints use the color's own form.
        let exact = matches!(self.color, Color::Custom(_));
        let effect = self.effect != RingEffect::Flat;
        let mut fill = serializer
            .serialize_struct("RingFill", 2 + usize::from(exact) + usize::from(effect))?;
        fill.serialize_field("atoms", &self.atoms)?;
        if exact {
            fill.serialize_field("color", &self.color.rgb())?;
            fill.serialize_field("fixed_color", &true)?;
        } else {
            fill.serialize_field("color", &self.color)?;
        }
        if effect {
            fill.serialize_field("effect", &self.effect)?;
        }
        fill.end()
    }
}
impl TryFrom<StoredRingFill> for RingFill {
    type Error = String;
    fn try_from(stored: StoredRingFill) -> Result<Self, String> {
        let color = match stored.color {
            StoredColor::Legacy(rgb) if stored.fixed_color => Color::Custom(rgb),
            StoredColor::Legacy([0, 0, 0]) => Color::Ink,
            StoredColor::Legacy(rgb) => {
                legacy_hue(rgb).map_or(Color::Custom(rgb), |hue| Color::Palette(hue, Row::Tint))
            }
            StoredColor::Current(s) => s.parse()?,
        };
        Ok(Self {
            atoms: stored.atoms,
            color,
            effect: stored.effect,
        })
    }
}

/// Keys of the five ring highlights stored by earlier versions, and the named
/// slots of theme files, with the palette hue each one became.
pub const PALETTE: [(&str, [u8; 3], Hue); 5] = [
    ("Sky", [201, 224, 248], Hue::Blue),
    ("Mint", [198, 233, 220], Hue::Teal),
    ("Rose", [249, 207, 209], Hue::Red),
    ("Lilac", [226, 211, 245], Hue::Purple),
    ("Sand", [255, 241, 174], Hue::Amber),
];
fn legacy_hue(key: [u8; 3]) -> Option<Hue> {
    PALETTE
        .iter()
        .find(|(_, rgb, _)| *rgb == key)
        .map(|(_, _, hue)| *hue)
}

pub(crate) fn key(atoms: &[u64]) -> Vec<u64> {
    let mut result = atoms.to_vec();
    if let Some((first, _)) = result.iter().enumerate().min_by_key(|(_, id)| *id) {
        result.rotate_left(first);
        if result.get(1).zip(result.last()).is_some_and(|(a, b)| a > b)
            && let Some(tail) = result.get_mut(1..)
        {
            tail.reverse();
        }
    }
    result
}

impl RingFill {
    pub fn commands(&self, doc: &Document) -> Vec<PathCommand> {
        if !self.atoms.iter().all(|id| doc.atom_visible(*id)) {
            return vec![];
        }
        let points: Option<Vec<Point>> = self
            .atoms
            .iter()
            .map(|id| doc.atom(*id).map(|a| a.position))
            .collect();
        let Some(points) = points else {
            return vec![];
        };
        let mut commands: Vec<_> = points
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    PathCommand::Move(p)
                } else {
                    PathCommand::Line(p)
                }
            })
            .collect();
        commands.push(PathCommand::Close);
        commands
    }
    pub fn primitive(&self, doc: &Document) -> Option<Primitive> {
        let commands = self.commands(doc);
        if commands.is_empty() {
            return None;
        }
        let flat = |commands| Primitive::Path {
            commands,
            style: GraphicStyle {
                stroke: self.color,
                fill: Some(self.color),
                width_pt: 0.,
                ..Default::default()
            },
            filled: true,
        };
        Some(match self.effect {
            RingEffect::Flat => flat(commands),
            RingEffect::SeeThrough => Primitive::Opacity {
                alpha: crate::graphics::Effects::SEE_THROUGH,
                primitive: Box::new(flat(commands)),
            },
            RingEffect::Glow => {
                let bond = doc.drawing_style.bond_length_world;
                crate::scene::glow_primitive(commands, self.color, bond * 0.1, bond * 0.2, doc)
            }
            RingEffect::Disc => {
                // The circle through the ring's farthest atom, a little wider.
                let points: Vec<_> = commands.iter().flat_map(PathCommand::points).collect();
                let n = points.len().max(1) as f32;
                let center = points
                    .iter()
                    .fold(Point::default(), |c, p| c.offset(p.x / n, p.y / n));
                let r = points.iter().map(|p| p.distance(center)).fold(0., f32::max) * DISC;
                flat(circle(center, r))
            }
            RingEffect::Gradient => {
                let (origin, x, y) = crate::graphics::bounding_frame(&commands)?;
                crate::scene::gradient_primitive(
                    commands,
                    [origin, x, y],
                    crate::graphics::Gradient {
                        from: self
                            .atoms
                            .first()
                            .map_or(crate::graphics::Gradient::PAPER, |id| {
                                crate::scene::drawn_paper(doc, *id)
                            }),
                        kind: crate::graphics::GradientKind::Radial,
                    },
                    self.color,
                    doc,
                )
            }
        })
    }
}

fn valid(fill: &RingFill, atoms: &HashSet<u64>, edges: &HashSet<(u64, u64)>) -> bool {
    (3..=64).contains(&fill.atoms.len())
        && fill.atoms.iter().collect::<HashSet<_>>().len() == fill.atoms.len()
        && fill.atoms.iter().all(|id| atoms.contains(id))
        && fill
            .atoms
            .iter()
            .zip(fill.atoms.iter().cycle().skip(1))
            .all(|(a, b)| edges.contains(&((*a).min(*b), (*a).max(*b))))
}
fn topology(doc: &Document) -> (HashSet<u64>, HashSet<(u64, u64)>) {
    (
        doc.atoms.iter().map(|a| a.id).collect(),
        doc.bonds
            .iter()
            .map(|b| (b.a.min(b.b), b.a.max(b.b)))
            .collect(),
    )
}
pub fn validate(doc: &Document) -> Result<(), String> {
    if doc.ring_fills.len() > 10_000 {
        return Err("Too many ring fills".into());
    }
    let (atoms, edges) = topology(doc);
    let mut seen = HashSet::new();
    for fill in &doc.ring_fills {
        if !valid(fill, &atoms, &edges) || !seen.insert(key(&fill.atoms)) {
            return Err("Ring fill needs a unique, closed cycle of 3–64 existing atoms".into());
        }
    }
    Ok(())
}
pub fn prune(doc: &mut Document) {
    let (atoms, edges) = topology(doc);
    let mut seen = HashSet::new();
    doc.ring_fills
        .retain(|fill| valid(fill, &atoms, &edges) && seen.insert(key(&fill.atoms)));
}
/// Minimal closed cycles entirely within the visible selection. Bounded BFS
/// avoids enumerating the exponentially many cycles in a dense graph.
pub fn selected_cycles(doc: &Document, selected: &[u64]) -> Vec<Vec<u64>> {
    let selected: HashSet<_> = selected.iter().copied().collect();
    let mut adjacent: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for b in &doc.bonds {
        if (1..=4).contains(&b.order)
            && selected.contains(&b.a)
            && selected.contains(&b.b)
            && doc.bond_visible(b.a, b.b)
        {
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
        }
    }
    for next in adjacent.values_mut() {
        next.sort_unstable();
    }
    let mut cycles = BTreeSet::new();
    let mut budget = 200_000_usize;
    for (&a, next) in &adjacent {
        for &b in next.iter().filter(|b| **b > a) {
            let mut previous = BTreeMap::from([(a, a)]);
            let mut queue = VecDeque::from([(a, 0)]);
            while let Some((current, depth)) = queue.pop_front() {
                if budget == 0 {
                    return cycles.into_iter().collect();
                }
                budget -= 1;
                if depth >= 63 {
                    continue;
                }
                for &neighbor in adjacent.get(&current).into_iter().flatten() {
                    if (current == a && neighbor == b) || previous.contains_key(&neighbor) {
                        continue;
                    }
                    previous.insert(neighbor, current);
                    queue.push_back((neighbor, depth + 1));
                    if neighbor == b {
                        break;
                    }
                }
                if previous.contains_key(&b) {
                    break;
                }
            }
            if !previous.contains_key(&b) {
                continue;
            }
            let mut ids = vec![b];
            let mut current = b;
            while current != a {
                let Some(parent) = previous.get(&current).copied() else {
                    break;
                };
                ids.push(parent);
                current = parent;
            }
            if current == a && ids.len() >= 3 {
                cycles.insert(key(&ids));
            }
        }
    }
    cycles.into_iter().collect()
}
/// A disc's radius over the ring's own.
const DISC: f32 = 1.15;

/// A circle of four cubic quarters.
fn circle(c: Point, r: f32) -> Vec<PathCommand> {
    const K: f32 = 0.552_284_8;
    let p = |x: f32, y: f32| c.offset(x * r, y * r);
    vec![
        PathCommand::Move(p(1., 0.)),
        PathCommand::Cubic(p(1., K), p(K, 1.), p(0., 1.)),
        PathCommand::Cubic(p(-K, 1.), p(-1., K), p(-1., 0.)),
        PathCommand::Cubic(p(-1., -K), p(-K, -1.), p(0., -1.)),
        PathCommand::Cubic(p(K, -1.), p(1., -K), p(1., 0.)),
        PathCommand::Close,
    ]
}

/// Paint the selected rings' fills with `effect`; returns how many changed.
pub fn set_effect(doc: &mut Document, selected: &[u64], effect: RingEffect) -> usize {
    let cycles: HashSet<_> = selected_cycles(doc, selected).into_iter().collect();
    let mut changed = 0;
    for fill in &mut doc.ring_fills {
        if fill.effect != effect && cycles.contains(&key(&fill.atoms)) {
            fill.effect = effect;
            changed += 1;
        }
    }
    changed
}
/// Return the number of rings changed. None removes paint, not bonds.
pub fn apply(doc: &mut Document, selected: &[u64], color: Option<Color>) -> usize {
    let cycles = selected_cycles(doc, selected);
    let mut changed = 0;
    for atoms in cycles {
        let index = doc
            .ring_fills
            .iter()
            .position(|fill| key(&fill.atoms) == atoms);
        match (index, color) {
            (Some(i), Some(color)) => {
                if let Some(fill) = doc.ring_fills.get_mut(i)
                    && fill.color != color
                {
                    fill.color = color;
                    changed += 1;
                }
            }
            (Some(i), None) => {
                doc.ring_fills.remove(i);
                changed += 1;
            }
            (None, Some(color)) => {
                doc.ring_fills.push(RingFill {
                    atoms,
                    color,
                    effect: RingEffect::Flat,
                });
                changed += 1;
            }
            _ => (),
        }
    }
    changed
}

#[cfg(test)]
mod tests;
