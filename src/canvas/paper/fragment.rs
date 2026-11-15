//! Redraw only the molecules a preview changes, over the cached rest of the drawing.
//!
//! A ring, template or bond preview, or a partial move, changes a few atoms of
//! a large drawing. Their connected molecules are drawn from the preview; the
//! rest keeps its scene and tessellation. The split applies only while the two
//! parts' ink stays apart, so no crossing gap, halo or paint order between
//! them can differ from drawing the whole preview.
use crate::canvas::{Camera, World, cache::SceneCache, markers::Markers};
use reshiki::document::Document;
use reshiki::graphics::PathCommand;
use reshiki::scene::{self, Primitive};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

#[cfg(test)]
thread_local! {
    /// Tests turn the split off to render the whole-preview reference.
    pub(in crate::canvas) static SPLIT: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// The drawing without the molecules a preview redraws.
pub(in crate::canvas) struct Remainder {
    pub scene: Rc<[Primitive]>,
    /// Empty: the preview draws its own selection markers below both parts.
    pub markers: Rc<Markers>,
    ink: Vec<(World, World)>,
}

impl Remainder {
    pub fn new(scene: Rc<[Primitive]>) -> Self {
        let mut ink = Vec::new();
        for p in scene.iter() {
            push_ink(p, &mut ink);
        }
        Self {
            ink,
            markers: Rc::default(),
            scene,
        }
    }

    /// The scene of `doc` without the sorted `excluded` objects, or `None`
    /// when an object that stays references one of them.
    pub fn scene_without(doc: &Document, excluded: &[u64]) -> Option<Rc<[Primitive]>> {
        let kept: Vec<_> = doc
            .object_ids()
            .filter(|id| excluded.binary_search(id).is_err())
            .collect();
        let part = reshiki::editing::preserved_expansion_selection(doc, &kept).ok()?;
        part.object_ids()
            .all(|id| excluded.binary_search(&id).is_err())
            .then(|| scene::primitives(&part).into())
    }
}

pub(super) struct Fragment {
    pub remainder: Rc<Remainder>,
    pub scene: Vec<Primitive>,
}

impl Fragment {
    /// Split `preview` against the cached `base`, or `None` when it changes
    /// more than atoms, bonds and abbreviations, or the parts' ink would meet.
    pub fn new(
        cache: &mut SceneCache,
        base: &Document,
        preview: &Document,
        camera: Camera,
    ) -> Option<Self> {
        #[cfg(test)]
        if !SPLIT.with(std::cell::Cell::get) {
            return None;
        }
        let atoms = molecules(base, preview, changed_atoms(base, preview)?);
        let ids: Vec<_> = preview
            .atoms
            .iter()
            .map(|a| a.id)
            .filter(|id| atoms.contains(id))
            .collect();
        let part = reshiki::editing::preserved_expansion_selection(preview, &ids).ok()?;
        let drawn: HashSet<_> = part.object_ids().chain(atoms).collect();
        let mut excluded: Vec<_> = base.object_ids().filter(|id| drawn.contains(id)).collect();
        excluded.sort_unstable();
        let remainder = cache.remainder(base, &excluded)?;
        let scene = scene::primitives(&part);
        let mut fragment = Vec::new();
        for p in &scene {
            push_ink(p, &mut fragment);
        }
        // A pixel of antialiasing and a little more for estimated text widths.
        let margin = 3. / camera.zoom;
        for (lo, hi) in &mut fragment {
            *lo = lo.offset(-margin, -margin);
            *hi = hi.offset(margin, margin);
        }
        let all = fragment.iter().copied().reduce(|(a, b), (c, d)| {
            (
                World::new(a.x.min(c.x), a.y.min(c.y)),
                World::new(b.x.max(d.x), b.y.max(d.y)),
            )
        });
        let apart = all.is_none_or(|all| {
            remainder
                .ink
                .iter()
                .filter(|rest| overlap(rest, &all))
                .all(|rest| !fragment.iter().any(|f| overlap(rest, f)))
        });
        apart.then_some(Self { remainder, scene })
    }
}

/// Atoms whose drawing can differ, or `None` when anything besides atoms,
/// bonds and abbreviations differs. Rear opacity and 3D models depend on the
/// whole drawing, so they always take the full path.
fn changed_atoms(base: &Document, preview: &Document) -> Option<HashSet<u64>> {
    let Document {
        generic,
        canvas_theme,
        color_theme,
        custom_theme,
        drawing_style,
        page_layout,
        abbreviations,
        atom_labels,
        ring_fills,
        depth_appearance,
        version,
        atoms,
        bonds,
        annotations,
        molecule_names,
        arrows,
        arrow_labels,
        graphics,
        groups,
        reactions,
        models,
        recent_colors,
    } = preview;
    let same = depth_appearance.is_empty()
        && base.depth_appearance.is_empty()
        && models.is_empty()
        && base.models.is_empty()
        && *generic == base.generic
        && *canvas_theme == base.canvas_theme
        && *color_theme == base.color_theme
        && *custom_theme == base.custom_theme
        && *drawing_style == base.drawing_style
        && *page_layout == base.page_layout
        && *atom_labels == base.atom_labels
        && *ring_fills == base.ring_fills
        && *version == base.version
        && *annotations == base.annotations
        && *molecule_names == base.molecule_names
        && *arrows == base.arrows
        && *arrow_labels == base.arrow_labels
        && *graphics == base.graphics
        && *groups == base.groups
        && *reactions == base.reactions
        && *recent_colors == base.recent_colors;
    if !same {
        return None;
    }
    let mut changed: HashSet<_> = differing(&base.atoms, atoms, |a| a.id)
        .map(|a| a.id)
        .collect();
    changed.extend(differing(&base.bonds, bonds, |b| (b.a, b.b)).flat_map(|b| [b.a, b.b]));
    changed.extend(
        differing(&base.abbreviations, abbreviations, |g| g.anchor)
            .flat_map(|g| g.members.iter().copied().chain([g.anchor])),
    );
    Some(changed)
}

/// Entries of `a` or `b` without an equal entry under the same key in the
/// other. Previews append, insert or edit in one place, so only the run
/// between the equal start and end is matched by key.
fn differing<'a, T: PartialEq, K: Hash + Eq>(
    a: &'a [T],
    b: &'a [T],
    key: impl Fn(&T) -> K,
) -> impl Iterator<Item = &'a T> {
    let start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (a, b) = (a.split_at(start).1, b.split_at(start).1);
    let end = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a, b) = (a.split_at(a.len() - end).0, b.split_at(b.len() - end).0);
    let before: HashMap<_, _> = a.iter().map(|x| (key(x), x)).collect();
    let kept: HashSet<_> = b
        .iter()
        .filter(|y| before.get(&key(y)) == Some(y))
        .map(&key)
        .collect();
    a.iter().chain(b).filter(move |x| !kept.contains(&key(x)))
}

/// Every atom bonded, in either drawing, to a changed one.
fn molecules(base: &Document, preview: &Document, mut atoms: HashSet<u64>) -> HashSet<u64> {
    let mut neighbors: HashMap<u64, Vec<u64>> = HashMap::new();
    for b in base.bonds.iter().chain(&preview.bonds) {
        neighbors.entry(b.a).or_default().push(b.b);
        neighbors.entry(b.b).or_default().push(b.a);
    }
    let mut stack: Vec<_> = atoms.iter().copied().collect();
    while let Some(id) = stack.pop() {
        for &next in neighbors.get(&id).into_iter().flatten() {
            if atoms.insert(next) {
                stack.push(next);
            }
        }
    }
    atoms
}

/// Where a primitive can paint. A path holds every bond of one color, so
/// each of its subpaths counts on its own; a halo adds the paper it spreads.
fn push_ink(p: &Primitive, ink: &mut Vec<(World, World)>) {
    match p {
        Primitive::Path {
            commands,
            style,
            filled,
        } => {
            for subpath in commands.chunk_by(|_, c| !matches!(c, PathCommand::Move(_))) {
                ink.push(scene::bounds(&[Primitive::Path {
                    commands: subpath.to_vec(),
                    style: style.clone(),
                    filled: *filled,
                }]));
            }
        }
        Primitive::Opacity { primitive, .. } => push_ink(primitive, ink),
        Primitive::Halo { radius, primitives } => {
            let (lo, hi) = scene::bounds(primitives);
            ink.push((lo.offset(-radius, -radius), hi.offset(*radius, *radius)));
        }
        p => ink.push(scene::bounds(std::slice::from_ref(p))),
    }
}

fn overlap(a: &(World, World), b: &(World, World)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}
