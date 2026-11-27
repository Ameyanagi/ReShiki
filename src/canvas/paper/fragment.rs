//! Redraw only the molecules a preview changes, over the cached rest of the drawing.
//!
//! A ring, template or bond preview, or a partial move, changes a few atoms of
//! a large drawing. A free equipment preview adds one piece. The changed part
//! is drawn from the preview; the rest keeps its scene and tessellation.
//! The split applies only while the two
//! parts stay apart: no ink, atom, bond or automatic indicator's reach of one
//! comes near the other's. Then no crossing gap, halo,
//! paint order or automatic label placement can differ from drawing the whole
//! preview.
use crate::canvas::{Camera, World, cache::SceneCache, markers::Markers};
use reshiki::document::Document;
use reshiki::graphics::{Graphic, PathCommand};
use reshiki::scene::{self, Primitive};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

#[cfg(test)]
thread_local! {
    /// Tests turn the split off to render the whole-preview reference.
    pub(in crate::canvas) static SPLIT: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// A retained part of a drawing and the area it can affect.
pub(in crate::canvas) struct ScenePart {
    pub scene: Rc<[Primitive]>,
    /// Empty: the preview draws its own selection markers below both parts.
    pub markers: Rc<Markers>,
    layout: Vec<(World, World)>,
}

impl ScenePart {
    pub fn new(doc: &Document) -> Self {
        let scene: Rc<[Primitive]> = scene::primitives(doc).into();
        Self {
            layout: layout(doc, &scene),
            markers: Rc::default(),
            scene,
        }
    }

    /// `doc` without the sorted `excluded` objects, or `None` when an object
    /// that stays references one of them. `scene` gives `doc`'s own scene,
    /// used when nothing is excluded.
    pub fn remainder(
        doc: &Document,
        excluded: &[u64],
        scene: impl FnOnce() -> Rc<[Primitive]>,
    ) -> Option<Self> {
        let (part, scene) = if excluded.is_empty() {
            (Cow::Borrowed(doc), scene())
        } else {
            let kept: Vec<_> = doc
                .object_ids()
                .filter(|id| excluded.binary_search(id).is_err())
                .collect();
            let part = reshiki::editing::preserved_expansion_selection(doc, &kept).ok()?;
            if part
                .object_ids()
                .any(|id| excluded.binary_search(&id).is_ok())
            {
                return None;
            }
            let scene = scene::primitives(&part).into();
            (Cow::Owned(part), scene)
        };
        Some(Self {
            layout: layout(&part, &scene),
            markers: Rc::default(),
            scene,
        })
    }
}

pub(super) struct Fragment {
    pub remainder: Rc<ScenePart>,
    pub scene: Rc<[Primitive]>,
    pub shift: Option<World>,
}

impl Fragment {
    /// Split a small molecular edit or a freely placed piece from the retained
    /// drawing. Return `None` when the parts depend on or meet each other.
    pub fn new(
        cache: &mut SceneCache,
        base: &Document,
        preview: &Document,
        camera: Camera,
        move_delta: Option<World>,
    ) -> Option<Self> {
        #[cfg(test)]
        if !SPLIT.with(std::cell::Cell::get) {
            return None;
        }
        if move_delta.is_none() && !base.models.is_empty() {
            return None;
        }
        let (part, excluded) = if let Some((piece, original)) = appended_piece(base, preview) {
            if !changed_atoms(base, preview, original)?.is_empty() {
                return None;
            }
            let part = reshiki::editing::preserved_expansion_selection(preview, &[piece]).ok()?;
            (part, Vec::new())
        } else {
            let atoms = molecules(
                base,
                preview,
                changed_atoms(base, preview, &preview.graphics)?,
            );
            // A cycle's arrows follow its parts, so moving one redraws it whole.
            let held = reshiki::cycles::part_ids(base);
            if atoms.iter().any(|id| held.contains(id)) {
                return None;
            }
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
            (part, excluded)
        };
        // Rebuilding a 2D part is cheap and exact; only a part showing a 3D
        // model reuses its retained scene and tiles while it moves.
        let rigid = move_delta
            .filter(|_| {
                part.models
                    .iter()
                    .any(|m| m.display != reshiki::models3d::Display::TwoD)
            })
            .and_then(|delta| rigid_part(base, &part, &excluded, delta));
        if !base.models.is_empty() {
            // A model must be kept whole on one side of the split.
            rigid.as_ref()?;
            for model in &base.models {
                let linked = model.linked_ids(base);
                let in_fragment = |id: &u64| excluded.binary_search(id).is_ok();
                if linked.is_empty()
                    || (linked.iter().any(in_fragment) && !linked.iter().all(in_fragment))
                {
                    return None;
                }
            }
        }
        let remainder = cache.remainder(base, &excluded)?;
        let (scene, mut fragment, shift) = if let Some((original, delta)) = rigid {
            let cached = cache.moved_part(base, &excluded, &original);
            (cached.scene.clone(), cached.layout.clone(), Some(delta))
        } else {
            let scene: Rc<[Primitive]> = scene::primitives(&part).into();
            let boxes = layout(&part, &scene);
            (scene, boxes, None)
        };
        // A pixel of antialiasing and a little more for estimated text widths,
        // plus the gap automatic hydrogens, charges and group labels keep from
        // other ink: 0.12 of their font size.
        let margin = 3. / camera.zoom + 0.12 * label_font(preview);
        let delta = shift.unwrap_or_default();
        for (lo, hi) in &mut fragment {
            *lo = lo.offset(delta.x, delta.y).offset(-margin, -margin);
            *hi = hi.offset(delta.x, delta.y).offset(margin, margin);
        }
        let all = fragment.iter().copied().reduce(|(a, b), (c, d)| {
            (
                World::new(a.x.min(c.x), a.y.min(c.y)),
                World::new(b.x.max(d.x), b.y.max(d.y)),
            )
        });
        let apart = all.is_none_or(|all| {
            remainder
                .layout
                .iter()
                .filter(|rest| overlap(rest, &all))
                .all(|rest| !fragment.iter().any(|f| overlap(rest, f)))
        });
        apart.then_some(Self {
            remainder,
            scene,
            shift,
        })
    }
}

fn rigid_part(
    base: &Document,
    part: &Document,
    excluded: &[u64],
    delta: World,
) -> Option<(Document, World)> {
    let original = reshiki::editing::preserved_expansion_selection(base, excluded).ok()?;
    let mut moved = original.clone();
    moved.translate(&original.all_ids(), delta.x, delta.y);
    (moved == *part).then_some((original, delta))
}

fn appended_piece<'a>(base: &Document, preview: &'a Document) -> Option<(u64, &'a [Graphic])> {
    // Cells and page clipping depend on the full scene. A solver that seats the
    // new piece also changes ports or paint order, checked below by changed_atoms.
    let (piece, original) = preview.graphics.split_last()?;
    (original.len() == base.graphics.len()
        && piece.piece.is_some()
        && piece.style.effects.opacity == Some(0.5)
        && base.page_layout.is_none()
        && !base.graphics.iter().any(|g| g.table.is_some()))
    .then_some((piece.id, original))
}

/// Atoms whose drawing can differ, or `None` when anything besides atoms,
/// bonds and abbreviations differs after setting aside the free preview piece.
/// Rear opacity depends on the whole drawing. Models can only be split
/// for a rigid move.
fn changed_atoms(
    base: &Document,
    preview: &Document,
    original_graphics: &[Graphic],
) -> Option<HashSet<u64>> {
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
        highlight_style,
        depth_appearance,
        version,
        atoms,
        bonds,
        annotations,
        molecule_names,
        property_captions,
        compound_numbers,
        arrows,
        arrow_labels,
        graphics: _,
        groups,
        reactions,
        models,
        recent_colors,
        piece_definitions,
        on_other_canvas: _,
        seats,
        // Calculation data is never drawn.
        science: _,
    } = preview;
    let same = depth_appearance.is_empty()
        && base.depth_appearance.is_empty()
        && *models == base.models
        && *generic == base.generic
        && *canvas_theme == base.canvas_theme
        && *color_theme == base.color_theme
        && *custom_theme == base.custom_theme
        && *drawing_style == base.drawing_style
        && *page_layout == base.page_layout
        && *atom_labels == base.atom_labels
        && *ring_fills == base.ring_fills
        && *highlight_style == base.highlight_style
        && *version == base.version
        && *annotations == base.annotations
        && *molecule_names == base.molecule_names
        && *property_captions == base.property_captions
        && *compound_numbers == base.compound_numbers
        && *piece_definitions == base.piece_definitions
        && *seats == base.seats
        && *arrows == base.arrows
        && *arrow_labels == base.arrow_labels
        && original_graphics == base.graphics
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

/// The largest label font size, in world units.
fn label_font(doc: &Document) -> f32 {
    let styles = doc
        .atoms
        .iter()
        .filter_map(|a| a.text_style.as_ref())
        .chain(
            doc.abbreviations
                .iter()
                .filter_map(|g| g.label_style.as_ref()),
        );
    let pt = styles
        .map(|style| style.size_pt)
        .fold(doc.drawing_style.font_size_pt, f32::max);
    doc.drawing_style.world(pt)
}

/// Where `doc`'s drawing `scene` paints, and what automatic placement in
/// another molecule would avoid even without ink: atoms, bonds (both may be
/// invisible) and where each automatic indicator searches.
fn layout(doc: &Document, scene: &[Primitive]) -> Vec<(World, World)> {
    let mut boxes = Vec::new();
    for p in scene {
        push_ink(p, &mut boxes);
    }
    boxes.extend(
        doc.atoms
            .iter()
            .map(|a| (a.position.offset(-3., -3.), a.position.offset(3., 3.))),
    );
    boxes.extend(doc.bonds.iter().filter_map(|b| {
        let (p, q) = (doc.atom(b.a)?.position, doc.atom(b.b)?.position);
        Some((
            World::new(p.x.min(q.x), p.y.min(q.y)),
            World::new(p.x.max(q.x), p.y.max(q.y)),
        ))
    }));
    // Indicators avoid label line boxes, which reach beyond the ink by less
    // than half the font size.
    let font = label_font(doc) / 2.;
    for indicator in reshiki::atom_labels::indicators(doc) {
        let (c, reach) = (indicator.center, indicator.reach() + font);
        boxes.push((c.offset(-reach, -reach), c.offset(reach, reach)));
    }
    boxes
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
