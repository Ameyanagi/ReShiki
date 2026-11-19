//! Two versions of a drawing compared as chemistry and as layout, for
//! History's Compare and the changed-on-GitHub dialog. Molecules match by
//! their ids, checked against the bonds they keep, then by canonical SMILES,
//! duplicates by where they sit, then, for drawings whose ids changed, by
//! SMILES without stereo and by structure (see [`structure`]). Each matched
//! pair is compared as chemistry (see [`chemistry`]) and fitted onto its old
//! self with a rigid move, or failing that one that also reflects or scales:
//! what the fit removes is a move, a mirror or a scale, what it leaves is a
//! redraw. A move most of the scheme shares is one change. Style is compared
//! once the right side is lined up, text on its own. One object can have
//! several kinds of change.
mod chemistry;
pub mod fit;
mod structure;
#[cfg(test)]
mod tests;

use crate::document::{Document, Point};
use crate::reactions::Role;
use chemistry::{Signature, key};
use fit::{Fit, Xy};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

/// The two filters a change belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Chemistry,
    Layout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The molecule is different chemistry.
    Chemical,
    /// Its role or coefficient in a reaction, or an arrow's kind.
    Reaction,
    Added,
    Removed,
    /// Chemistry that cannot be compared, with the reason in [`Detail`].
    Unverified,
    /// Most of the drawing moved together.
    SchemeMoved,
    Moved,
    Rotated,
    /// Reflected, as by the Flip tool.
    Mirrored,
    /// Drawn larger or smaller.
    Scaled,
    /// Same chemistry or the same object, drawn in another shape.
    Redrawn,
    Style,
    Text,
}

/// What a change is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Molecule,
    Arrow,
    Text,
    Graphic,
    Scheme,
    /// The drawing's style, themes and other settings.
    Settings,
}

/// Where a change is, on one side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    /// The objects concerned: a molecule's atoms, or a text, arrow or graphic.
    pub objects: Vec<u64>,
    /// Atoms and bonds that changed as chemistry, to paint.
    pub atoms: Vec<u64>,
    pub bonds: Vec<(u64, u64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Detail {
    None,
    /// Heavy atoms added and removed, as formulas such as `NO₂`, whether
    /// only the stereo changed, and the edit in parts to word it.
    Edit {
        added: String,
        removed: String,
        changed: usize,
        stereo: bool,
        /// Every stereocenter inverted and nothing else: the mirror image.
        mirror_image: bool,
        parts: Vec<Part>,
        /// Matched by structure, its ids having changed.
        by_structure: bool,
    },
    /// How far, in drawing units, and how much it turned, clockwise; how
    /// much larger it is drawn, and the mirror line, in degrees from the
    /// x axis, if it was reflected.
    Move {
        dx: f64,
        dy: f64,
        degrees: f64,
        scale: f64,
        mirror: Option<f64>,
    },
    Reason(String),
}

/// One piece of a chemical edit, worded with the drawing's atom numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub kind: PartKind,
    /// A formula such as `OH`, `Cl → Br`, `N → N⁺` or `single → double`.
    pub what: String,
    /// Where: atom numbers such as `C4`, as the drawings number them, empty
    /// when they are not numbered.
    pub at: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Added,
    Removed,
    /// A group removed and another added in its place.
    Replaced,
    /// An atom's charge, isotope, hydrogens or unpaired electrons.
    Atom,
    /// A bond's order, or a double bond's geometry.
    Bond,
    /// A bond made or broken between atoms both sides have.
    BondAdded,
    BondRemoved,
    /// A stereocenter inverted.
    Inverted,
    /// A stereocenter's stereo set, cleared or otherwise changed.
    Stereo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub kind: Kind,
    pub subject: Subject,
    pub left: Marks,
    pub right: Marks,
    /// SMILES, text or a role before and after, where one applies.
    pub before: Option<String>,
    pub after: Option<String>,
    pub detail: Detail,
}

impl Change {
    fn new(kind: Kind, subject: Subject) -> Self {
        Self {
            kind,
            subject,
            left: Marks::default(),
            right: Marks::default(),
            before: None,
            after: None,
            detail: Detail::None,
        }
    }

    pub fn category(&self) -> Category {
        match self.kind {
            Kind::Chemical | Kind::Reaction | Kind::Unverified => Category::Chemistry,
            Kind::Added | Kind::Removed if self.subject == Subject::Molecule => Category::Chemistry,
            _ => Category::Layout,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchedBy {
    Id,
    Smiles,
    /// The same atoms and bonds; stereo or isotopes changed.
    Constitution,
    /// Most bonds in common, once ids changed.
    Structure,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Chemistry {
    Same,
    Changed,
    Unverified(String),
}

/// A molecule found in both versions.
#[derive(Clone, Debug, PartialEq)]
pub struct Pair {
    pub left: Vec<u64>,
    pub right: Vec<u64>,
    pub by: MatchedBy,
    pub chemistry: Chemistry,
    /// Canonical SMILES of each side, where it could be read.
    pub smiles: [Option<String>; 2],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub molecules: Vec<Pair>,
    pub changes: Vec<Change>,
    /// The move most of the drawing shares, right onto left, if it moved.
    pub scheme: Option<Fit>,
    /// Right-side objects and the fit that lines each up with its left
    /// counterpart: the right side in the left side's frame.
    pub alignment: Vec<(Vec<u64>, Fit)>,
    /// The bond length the tolerances are measured in.
    pub unit: f64,
}

impl Report {
    /// Changes in `category`, Unverified left out.
    pub fn count(&self, category: Category) -> usize {
        self.changes
            .iter()
            .filter(|change| change.category() == category && change.kind != Kind::Unverified)
            .count()
    }

    pub fn unverified(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.kind == Kind::Unverified)
            .count()
    }
}

/// Positions must agree to this fraction of a bond length: in mean square
/// and at any one point.
const RMS: f64 = 0.01;
const MAX: f64 = 0.03;

/// The median bond length, of drawn bonds of nonzero order.
fn bond_length(doc: &Document) -> Option<f64> {
    let mut lengths: Vec<f64> = doc
        .bonds
        .iter()
        .filter(|bond| bond.order != 0)
        .filter_map(|bond| {
            let (a, b) = (doc.atom(bond.a)?.position, doc.atom(bond.b)?.position);
            Some(f64::from(a.x - b.x).hypot(f64::from(a.y - b.y)))
        })
        .filter(|length| *length > 1e-6)
        .collect();
    lengths.sort_by(f64::total_cmp);
    lengths.get(lengths.len() / 2).copied()
}

fn xy(p: Point) -> Xy {
    (f64::from(p.x), f64::from(p.y))
}

fn json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or_default()
}

/// Numbers to a hundredth of a drawing unit, angles within a turn.
fn rounded(value: Value) -> Value {
    match value {
        Value::Number(n) if !n.is_i64() && !n.is_u64() => {
            json(&n.as_f64().map(|x| (x * 100.).round() / 100.))
        }
        Value::Array(items) => Value::Array(items.into_iter().map(rounded).collect()),
        // An angle of -90° is one of 270°.
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| match value.as_f64() {
                    Some(degrees) if key == "angle" => (
                        key,
                        json(&((degrees * 100.).round() / 100.).rem_euclid(360.)),
                    ),
                    _ => (key, value),
                })
                .map(|(key, value)| (key, rounded(value)))
                .collect(),
        ),
        other => other,
    }
}

fn without(mut value: Value, keys: &[&str]) -> Value {
    if let Value::Object(fields) = &mut value {
        for key in keys {
            fields.remove(*key);
        }
    }
    value
}

/// One version's molecules and what is known of them.
struct Side<'a> {
    doc: &'a Document,
    molecules: Vec<Vec<u64>>,
    owner: HashMap<u64, usize>,
    signatures: Vec<Result<Signature, String>>,
    bonds: HashSet<(u64, u64)>,
}

impl<'a> Side<'a> {
    fn new(doc: &'a Document) -> Self {
        let ids: Vec<u64> = doc.atoms.iter().map(|atom| atom.id).collect();
        let molecules = crate::reactions::molecules(doc, &ids);
        let owner = molecules
            .iter()
            .enumerate()
            .flat_map(|(i, atoms)| atoms.iter().map(move |id| (*id, i)))
            .collect();
        let signatures = molecules
            .iter()
            .map(|atoms| chemistry::signature(doc, atoms))
            .collect();
        let bonds = doc
            .bonds
            .iter()
            .filter(|bond| bond.order != 0)
            .map(|bond| key(bond.a, bond.b))
            .collect();
        Self {
            doc,
            molecules,
            owner,
            signatures,
            bonds,
        }
    }

    fn atoms(&self, i: usize) -> &[u64] {
        self.molecules.get(i).map_or(&[], Vec::as_slice)
    }

    fn signature(&self, i: usize) -> Option<&Result<Signature, String>> {
        self.signatures.get(i)
    }

    fn position(&self, id: u64) -> Option<Xy> {
        self.doc.atom(id).map(|atom| xy(atom.position))
    }

    fn centroid(&self, i: usize) -> Xy {
        centroid(self.atoms(i).iter().filter_map(|id| self.position(*id)))
    }
}

fn centroid(points: impl Iterator<Item = Xy>) -> Xy {
    let (mut sum, mut n) = ((0., 0.), 0.);
    for (x, y) in points {
        sum = (sum.0 + x, sum.1 + y);
        n += 1.;
    }
    if n == 0. {
        (0., 0.)
    } else {
        (sum.0 / n, sum.1 / n)
    }
}

/// Each right atom with the nearest left atom of the same element once the
/// centroids meet, as (right, left); empty unless that pairs all one to one.
fn nearest(left: &Side, mine: &[u64], right: &Side, theirs: &[u64]) -> Vec<(u64, u64)> {
    if mine.len() != theirs.len() {
        return Vec::new();
    }
    let center =
        |side: &Side, ids: &[u64]| centroid(ids.iter().filter_map(|id| side.position(*id)));
    let (from, to) = (center(right, theirs), center(left, mine));
    let element = |side: &Side, id: u64| side.doc.atom(id).map(|atom| atom.element.clone());
    let mut taken = HashSet::new();
    let mut pairs = Vec::new();
    for b in theirs {
        let Some((x, y)) = right.position(*b) else {
            return Vec::new();
        };
        let at = (x - from.0 + to.0, y - from.1 + to.1);
        let best = mine
            .iter()
            .filter(|a| !taken.contains(*a) && element(left, **a) == element(right, *b))
            .filter_map(|a| {
                let (ax, ay) = left.position(*a)?;
                Some(((ax - at.0).hypot(ay - at.1), *a))
            })
            .min_by(|p, q| p.0.total_cmp(&q.0));
        let Some((_, a)) = best else {
            return Vec::new();
        };
        taken.insert(a);
        pairs.push((*b, a));
    }
    // Only a pairing that keeps every bond is the same drawing.
    let map: HashMap<u64, u64> = pairs.iter().copied().collect();
    let kept = right
        .bonds
        .iter()
        .all(|(a, b)| match (map.get(a), map.get(b)) {
            (Some(x), Some(y)) => left.bonds.contains(&key(*x, *y)),
            _ => true,
        });
    if kept { pairs } else { Vec::new() }
}

/// Of atoms paired (left, right), those bonded to the same paired atoms on
/// both sides: what rewiring nearby did not redraw. A group added or
/// removed does not count.
fn unchanged(left: &Side, right: &Side, atoms: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let map: HashMap<u64, u64> = atoms.iter().copied().collect();
    let paired: HashSet<u64> = map.values().copied().collect();
    let neighbors = |side: &Side, id: u64| -> Vec<u64> {
        side.bonds
            .iter()
            .filter_map(|(a, b)| match (*a == id, *b == id) {
                (true, _) => Some(*b),
                (_, true) => Some(*a),
                _ => None,
            })
            .collect()
    };
    atoms
        .iter()
        .filter(|(a, b)| {
            let mine: BTreeSet<u64> = (neighbors(left, *a).iter())
                .filter_map(|n| map.get(n).copied())
                .collect();
            let theirs: BTreeSet<u64> = (neighbors(right, *b).into_iter())
                .filter(|n| paired.contains(n))
                .collect();
            mine == theirs
        })
        .copied()
        .collect()
}

/// Molecule pairs, (left, right), by index.
type Molecules = Vec<(usize, usize)>;

/// Pairs molecules that share ids, as (strong, weak). Shared ids are
/// evidence, not proof: a pair must keep, between atoms of the same
/// elements, at least half the bonds of the smaller, or for single atoms the
/// same element. Exactly half is weak, as when a re-import numbered atoms
/// afresh and ethanol took ids of an ester, keeping one of its two bonds:
/// equal content and structure come first.
fn by_id(left: &Side, right: &Side) -> (Molecules, Molecules) {
    let sizes = |side: &Side| {
        let mut bonds = vec![0_usize; side.molecules.len()];
        for bond in side.doc.bonds.iter().filter(|bond| bond.order != 0) {
            if let Some(n) = side.owner.get(&bond.a).and_then(|i| bonds.get_mut(*i)) {
                *n += 1;
            }
        }
        bonds
    };
    let (sizes_l, sizes_r) = (sizes(left), sizes(right));
    let same = |id: u64| {
        left.doc.atom(id).map(|atom| &atom.element) == right.doc.atom(id).map(|atom| &atom.element)
    };
    // (bonds kept, atoms shared) for each candidate pair.
    let mut score = HashMap::<(usize, usize), (usize, usize)>::new();
    for (i, atoms) in left.molecules.iter().enumerate() {
        for id in atoms {
            if let Some(&j) = right.owner.get(id) {
                score.entry((i, j)).or_default().1 += 1;
            }
        }
    }
    for bond in left.doc.bonds.iter().filter(|bond| bond.order != 0) {
        let (Some(&i), Some(&j), Some(&k)) = (
            left.owner.get(&bond.a),
            right.owner.get(&bond.a),
            right.owner.get(&bond.b),
        ) else {
            continue;
        };
        if j == k && right.bonds.contains(&key(bond.a, bond.b)) && same(bond.a) && same(bond.b) {
            score.entry((i, j)).or_default().0 += 1;
        }
    }
    let mut candidates: Vec<_> = score
        .into_iter()
        .filter(|((i, j), (kept, _))| {
            let smaller = sizes_l.get(*i).min(sizes_r.get(*j)).copied().unwrap_or(0);
            (*kept > 0 && 2 * kept >= smaller)
                || match (left.atoms(*i), right.atoms(*j)) {
                    ([a], [b]) => {
                        a == b
                            && left.doc.atom(*a).map(|atom| &atom.element)
                                == right.doc.atom(*b).map(|atom| &atom.element)
                    }
                    _ => false,
                }
        })
        .collect();
    candidates.sort_by(|(pair_a, score_a), (pair_b, score_b)| {
        score_b.cmp(score_a).then(pair_a.cmp(pair_b))
    });
    let weak: HashSet<(usize, usize)> = (candidates.iter())
        .filter(|((i, j), (kept, _))| {
            *kept > 0 && 2 * kept == sizes_l.get(*i).min(sizes_r.get(*j)).copied().unwrap_or(0)
        })
        .map(|(pair, _)| *pair)
        .collect();
    greedy(candidates.into_iter().map(|(pair, _)| pair))
        .into_iter()
        .partition(|pair| !weak.contains(pair))
}

/// Takes pairs in order, each side at most once.
fn greedy(ranked: impl Iterator<Item = (usize, usize)>) -> Vec<(usize, usize)> {
    let (mut used_left, mut used_right) = (HashSet::new(), HashSet::new());
    let mut taken = Vec::new();
    for (i, j) in ranked {
        if !used_left.contains(&i) && !used_right.contains(&j) {
            used_left.insert(i);
            used_right.insert(j);
            taken.push((i, j));
        }
    }
    taken
}

/// Pairs what is left by `same` content, nearest first once `guess` carries
/// the right side onto the left.
fn by_content<K: PartialEq>(
    left: &[(usize, K, Xy)],
    right: &[(usize, K, Xy)],
    guess: &Fit,
) -> Vec<(usize, usize)> {
    let mut candidates = Vec::new();
    for (i, a, at) in left {
        for (j, b, bt) in right {
            if a == b {
                let moved = guess.apply(*bt);
                candidates.push(((moved.0 - at.0).hypot(moved.1 - at.1), *i, *j));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then((a.1, a.2).cmp(&(b.1, b.2))));
    greedy(candidates.into_iter().map(|(_, i, j)| (i, j)))
}

/// Something matched on both sides that has a place: a molecule, text, arrow
/// or graphic, with its points paired right → left.
struct Entity {
    subject: Subject,
    left: Vec<u64>,
    right: Vec<u64>,
    pairs: Vec<(Xy, Xy)>,
    fit: Fit,
    /// Only a translation could be fitted, so its shape is unknown.
    loose: bool,
}

impl Entity {
    fn new(subject: Subject, left: Vec<u64>, right: Vec<u64>, pairs: Vec<(Xy, Xy)>) -> Self {
        let fit = Fit::rigid(&pairs).unwrap_or(Fit::IDENTITY);
        Self {
            subject,
            left,
            right,
            pairs,
            fit,
            loose: false,
        }
    }

    /// A molecule or graphic that no rigid move fits, with three points off
    /// one line, may fit reflected, scaled, or both, in that order, at the
    /// same tolerances. Fewer points always fit a scale, so an arrow drawn
    /// longer stays redrawn.
    fn reflect_or_scale(&mut self, unit: f64) {
        if !self.reshaped(unit)
            || !matches!(self.subject, Subject::Molecule | Subject::Graphic)
            || fit::collinear(&self.pairs)
            || fit::collinear(&self.pairs.iter().map(|(a, b)| (*b, *a)).collect::<Vec<_>>())
        {
            return;
        }
        for (mirror, scale) in [(true, false), (false, true), (true, true)] {
            let Some(fit) = Fit::similar(&self.pairs, mirror, scale) else {
                continue;
            };
            let scaled = (fit.scale - 1.).abs() >= 0.01;
            if (scaled || !scale) && fit.rms <= RMS * unit && fit.max <= MAX * unit {
                self.fit = fit;
                return;
            }
        }
    }

    /// Its shape changed; with no point-for-point match, as an arrow that
    /// gained a bend, that is so too.
    fn reshaped(&self, unit: f64) -> bool {
        self.loose || self.fit.rms > RMS * unit || self.fit.max > MAX * unit
    }

    /// Whether `fit` carries it within the tolerances, even one its own fit
    /// leaves reshaped, such as an arrow scaled with the scheme. One with no
    /// point-for-point match never is.
    fn agrees(&self, fit: &Fit, unit: f64) -> bool {
        let (rms, max) = fit.residuals(&self.pairs);
        !self.loose && rms <= RMS * unit && max <= MAX * unit
    }

    fn displaced(&self, unit: f64) -> bool {
        Fit::IDENTITY.residuals(&self.pairs).1 > MAX * unit
    }

    /// Mirrored, scaled, rotated or moved, by its own fit.
    fn pose(&self, unit: f64) -> Kind {
        if self.fit.mirror {
            return Kind::Mirrored;
        }
        if self.fit.scale != 1. {
            return Kind::Scaled;
        }
        let center = centroid(self.pairs.iter().map(|(from, _)| *from));
        let turn = Fit {
            dx: 0.,
            dy: 0.,
            ..self.fit
        };
        let reach = turn.reach(
            self.pairs
                .iter()
                .map(|(from, _)| (from.0 - center.0, from.1 - center.1)),
        );
        if reach > MAX * unit {
            Kind::Rotated
        } else {
            Kind::Moved
        }
    }
}

/// The move most entities share, refitted with its reflection and scale on
/// all that share it, and which entities share it. Each entity's own fit is
/// a candidate; every entity it carries counts. `None` unless at least two
/// and three in four agree.
fn common(entities: &[Entity], unit: f64) -> Option<(Fit, Vec<bool>)> {
    let enough = |n: usize| n >= 2 && n * 4 >= entities.len() * 3;
    let mut best: Option<(usize, usize, Fit)> = None;
    for candidate in entities.iter().filter(|e| !e.reshaped(unit)) {
        let fit = candidate.fit;
        let agree: Vec<&Entity> = entities.iter().filter(|e| e.agrees(&fit, unit)).collect();
        let points = agree.iter().map(|e| e.pairs.len()).sum::<usize>();
        if best.is_none_or(|(n, p, _)| (agree.len(), points) > (n, p)) {
            best = Some((agree.len(), points, fit));
        }
    }
    let (support, _, fit) = best?;
    if !enough(support) {
        return None;
    }
    let pairs: Vec<(Xy, Xy)> = entities
        .iter()
        .filter(|e| e.agrees(&fit, unit))
        .flat_map(|e| e.pairs.iter().copied())
        .collect();
    let shares = |fit: Fit| {
        let shared: Vec<bool> = entities.iter().map(|e| e.agrees(&fit, unit)).collect();
        enough(shared.iter().filter(|s| **s).count()).then_some((fit, shared))
    };
    // The refit, unless it is degenerate or loses the quorum the candidate had.
    Fit::similar(&pairs, fit.mirror, fit.scale != 1.)
        .filter(|refit| refit.scale.is_finite() && refit.scale > 1e-6)
        .and_then(shares)
        .or_else(|| shares(fit))
}

/// An atom's number as the drawings show it, such as `C4` for a bare `4`,
/// `4a` or `1′`, other text as written: the current side's, qualified where
/// only one side numbers the atom or the two differ. `None` where neither
/// does, or the atom is hidden in a collapsed label.
fn locant(left: &Document, right: &Document, l: Option<u64>, r: Option<u64>) -> Option<String> {
    let number = |doc: &Document, id: Option<u64>| {
        let atom = doc.atom(id?)?;
        let text = &atom.display.number.as_ref()?.text;
        if text.is_empty() || !doc.atom_visible(atom.id) {
            return None;
        }
        let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
        let digits = rest.len() < text.len();
        let rest = rest
            .strip_prefix(|c: char| c.is_ascii_lowercase())
            .unwrap_or(rest);
        let bare = digits && rest.chars().all(|c| matches!(c, '\'' | '′' | '″'));
        Some(if bare {
            format!("{}{text}", atom.element)
        } else {
            text.clone()
        })
    };
    match (number(left, l), number(right, r)) {
        (Some(a), Some(b)) if a == b => Some(b),
        (Some(a), Some(b)) => Some(format!("{a} → {b}")),
        (None, Some(b)) => Some(format!("{b} (current)")),
        (Some(a), None) => Some(format!("{a} (compared)")),
        (None, None) => None,
    }
}

/// How the right side moved from where the left side has it, given `fit`
/// that lines it up and its points (right, left): how far the middle of its
/// extent went, so a flip or scale in place is no move, and the turn, scale
/// and mirror undone.
fn movement(fit: &Fit, pairs: &[(Xy, Xy)]) -> Detail {
    let middle = |points: &mut dyn Iterator<Item = Xy>| {
        let (lo, hi) = points.fold(
            (
                (f64::INFINITY, f64::INFINITY),
                (f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(lo, hi), (x, y)| ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y))),
        );
        ((lo.0 + hi.0) / 2., (lo.1 + hi.1) / 2.)
    };
    let now = middle(&mut pairs.iter().map(|(right, _)| *right));
    let was = middle(&mut pairs.iter().map(|(_, left)| *left));
    Detail::Move {
        dx: now.0 - was.0,
        dy: now.1 - was.1,
        degrees: -fit.degrees(),
        scale: 1. / fit.scale,
        mirror: fit.mirror.then(|| fit.axis_degrees()),
    }
}

/// Points of a text, arrow or graphic that show where it is and its shape.
fn arrow_points(arrow: &crate::document::Arrow) -> Vec<Point> {
    let mut points = vec![arrow.start, arrow.end];
    points.extend(arrow.control);
    points.extend(arrow.cubic.iter().flatten());
    for node in &arrow.nodes {
        points.push(node.point);
        points.extend(node.controls);
    }
    points
}

fn graphic_points(graphic: &crate::graphics::Graphic) -> Vec<Point> {
    let o = graphic.origin;
    vec![
        o,
        o.offset(graphic.axis_x.x, graphic.axis_x.y),
        o.offset(graphic.axis_y.x, graphic.axis_y.y),
    ]
}

fn zip_points(right: &[Point], left: &[Point]) -> Vec<(Xy, Xy)> {
    if right.len() != left.len() {
        // Another shape: the first points, start and end or the origin.
        return right
            .iter()
            .zip(left)
            .take(1)
            .map(|(r, l)| (xy(*r), xy(*l)))
            .collect();
    }
    right
        .iter()
        .zip(left)
        .map(|(r, l)| (xy(*r), xy(*l)))
        .collect()
}

/// A molecule's roles: (arrow, role, coefficient), with the arrow as the
/// right side names it.
fn roles(
    doc: &Document,
    atoms: &[u64],
    arrow: impl Fn(u64) -> Option<u64>,
) -> BTreeSet<(Option<u64>, &'static str, u16)> {
    let atoms: HashSet<u64> = atoms.iter().copied().collect();
    let mut found = BTreeSet::new();
    for reaction in &doc.reactions {
        for role in Role::ALL {
            for participant in reaction.participants(role) {
                if participant.atoms.iter().any(|id| atoms.contains(id)) {
                    let name = match role {
                        Role::Reactant => "Reactant",
                        Role::Product => "Product",
                        Role::Agent => "Reagent",
                    };
                    found.insert((arrow(reaction.arrow), name, participant.coefficient));
                }
            }
        }
    }
    found
}

fn describe_roles(roles: &BTreeSet<(Option<u64>, &'static str, u16)>) -> String {
    if roles.is_empty() {
        return "Not in a reaction".into();
    }
    roles
        .iter()
        .map(|(_, role, n)| {
            if *n > 1 {
                format!("{role} ×{n}")
            } else {
                (*role).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Atom and bond fields that are chemistry: compared as chemistry, so left
/// out of the style comparison of a molecule whose chemistry changed.
const ATOM_CHEMISTRY: &[&str] = &[
    "element",
    "charge",
    "isotope",
    "explicit_h",
    "no_implicit",
    "aromatic",
    "stereo",
    "stereo_group",
    "radical_electrons",
    "singlet",
];
const BOND_CHEMISTRY: &[&str] = &[
    "order",
    "display",
    "stereo",
    "stereo_atoms",
    "stereo_authoritative",
    "a",
    "b",
];
/// Where it is, and labels computed from the chemistry.
const ATOM_PLACE: &[&str] = &[
    "id",
    "position",
    "depth",
    "cip_label",
    "steroid_label",
    "label_h",
];

/// Atoms whose style differs between the two sides, among those both have:
/// colors, label display, highlights. With `chemistry` the drawing's own
/// chemistry fields count too, such as aromatic or Kekulé bonds.
/// `mirrored`: lined up by a reflection, where a bold wedge comes back hollow,
/// as the Flip tool turns both to hashed.
fn restyled(
    left: &Document,
    right: &Document,
    atoms: &[u64],
    chemistry: bool,
    mirrored: bool,
) -> Vec<u64> {
    let atom_keys: Vec<&str> = ATOM_PLACE
        .iter()
        .chain(if chemistry { &[][..] } else { ATOM_CHEMISTRY })
        .copied()
        .collect();
    let bond_keys: &[&str] = if chemistry {
        &["cip_label"]
    } else {
        BOND_CHEMISTRY
    };
    let set: HashSet<u64> = atoms.iter().copied().collect();
    let mut found = BTreeSet::new();
    // Offsets lined up by a turn differ in their last digits.
    let style = |value: Value, keys: &[&str]| rounded(without(value, keys));
    for id in atoms {
        if let (Some(a), Some(b)) = (left.atom(*id), right.atom(*id))
            && style(json(a), &atom_keys) != style(json(b), &atom_keys)
        {
            found.insert(*id);
        }
    }
    let bonds = |doc: &Document| -> HashMap<(u64, u64), Value> {
        doc.bonds
            .iter()
            .filter(|bond| set.contains(&bond.a) && set.contains(&bond.b))
            .map(|bond| {
                let mut value = style(json(bond), bond_keys);
                if let Some(display) = value.get_mut("display")
                    && mirrored
                    && display == "bold"
                {
                    *display = "hollow_wedge".into();
                }
                (key(bond.a, bond.b), value)
            })
            .collect()
    };
    let theirs = bonds(right);
    for (pair, value) in bonds(left) {
        if theirs.get(&pair).is_some_and(|other| *other != value) {
            found.extend([pair.0, pair.1]);
        }
    }
    // A label collapsed or expanded, or restyled.
    let groups = |doc: &Document| -> HashMap<u64, Value> {
        doc.abbreviations
            .iter()
            .filter(|group| group.members.iter().any(|id| set.contains(id)))
            .map(|group| (group.anchor, json(group)))
            .collect()
    };
    let (mine, theirs) = (groups(left), groups(right));
    for (anchor, value) in &mine {
        if theirs.get(anchor) != Some(value) {
            found.insert(*anchor);
        }
    }
    found.extend(theirs.keys().filter(|anchor| !mine.contains_key(anchor)));
    found.into_iter().collect()
}

/// Everything that is not an object or a link: style, themes, page.
fn settings(doc: &Document) -> Value {
    without(
        json(doc),
        &[
            "atoms",
            "bonds",
            "annotations",
            "arrows",
            "graphics",
            "arrow_labels",
            "molecule_names",
            "property_captions",
            "reactions",
            "abbreviations",
            "generic",
            // Neither shows: the file format and the color picker's recents.
            "version",
            "recent_colors",
        ],
    )
}

fn marks(objects: &[u64]) -> Marks {
    Marks {
        objects: objects.to_vec(),
        ..Marks::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Object {
    Text,
    Arrow,
    Graphic,
}

/// A text, arrow or graphic of one side: its id, what it says or is, and
/// its points.
struct Thing {
    id: u64,
    content: Value,
    points: Vec<Point>,
}

fn things(doc: &Document, object: Object) -> Vec<Thing> {
    match object {
        Object::Text => doc
            .annotations
            .iter()
            .map(|note| Thing {
                id: note.id,
                content: Value::String(note.text.clone()),
                points: vec![note.position],
            })
            .collect(),
        Object::Arrow => doc
            .arrows
            .iter()
            .map(|arrow| Thing {
                id: arrow.id,
                content: Value::String(arrow.kind.clone()),
                points: arrow_points(arrow),
            })
            .collect(),
        Object::Graphic => doc
            .graphics
            .iter()
            .map(|graphic| Thing {
                id: graphic.id,
                content: json(&graphic.kind),
                points: graphic_points(graphic),
            })
            .collect(),
    }
}

/// What a text, arrow or graphic looks like apart from where it is: the
/// part compared for Style, and the rest of it.
fn look(doc: &Document, object: Object, id: u64) -> (Value, Value) {
    match object {
        Object::Text => {
            let style = doc
                .annotations
                .iter()
                .find(|note| note.id == id)
                .map(|note| json(&note.format))
                .unwrap_or_default();
            // The links that attach it: to an arrow, or a property caption.
            let label = doc.arrow_labels.iter().find(|label| label.annotation == id);
            let caption = doc
                .property_captions
                .iter()
                .find(|caption| caption.annotation == id)
                .map(|caption| json(&caption.properties));
            (
                style,
                json(&(label.map(|l| (l.arrow, json(&l.side))), caption)),
            )
        }
        Object::Arrow => {
            let arrow = doc.arrows.iter().find(|arrow| arrow.id == id);
            (
                arrow.map(|arrow| json(&arrow.style)).unwrap_or_default(),
                // What its ends hold on to.
                // Its targets only: offsets turn with the drawing.
                arrow
                    .map(|arrow| {
                        let target = |anchor: &Option<crate::arrow_anchors::Anchor>| {
                            anchor.as_ref().map(|anchor| json(&anchor.target))
                        };
                        json(&(target(&arrow.start_anchor), target(&arrow.end_anchor)))
                    })
                    .unwrap_or_default(),
            )
        }
        Object::Graphic => {
            let graphic = doc.graphics.iter().find(|graphic| graphic.id == id);
            let all = graphic.map(json).unwrap_or_default();
            let style = graphic
                .map(|g| json(&(json(&g.style), g.layer, json(&g.phase), g.phase_flipped)))
                .unwrap_or_default();
            let shape = without(
                all,
                &[
                    "id",
                    "origin",
                    "axis_x",
                    "axis_y",
                    "depth",
                    "style",
                    "layer",
                    "phase",
                    "phase_flipped",
                ],
            );
            (style, shape)
        }
    }
}

/// Compares `left` with `right`. Pure and deterministic; it may take a
/// while for a large drawing, so the app runs it off the UI thread.
pub fn compare(left: &Document, right: &Document) -> Report {
    let unit = bond_length(left)
        .or_else(|| bond_length(right))
        .unwrap_or_else(|| f64::from(left.drawing_style.bond_length_world))
        .max(1e-3);
    let (l, r) = (Side::new(left), Side::new(right));
    let mut report = Report {
        unit,
        ..Report::default()
    };

    // Molecules by id, then texts, arrows and graphics by id.
    let (strong, weak) = by_id(&l, &r);
    let mut pairs: Vec<(usize, usize, MatchedBy)> = strong
        .into_iter()
        .map(|(i, j)| (i, j, MatchedBy::Id))
        .collect();
    let objects = [Object::Text, Object::Arrow, Object::Graphic];
    let mut thing_pairs: HashMap<Object, Vec<(usize, usize)>> = HashMap::new();
    let all_things: HashMap<Object, (Vec<Thing>, Vec<Thing>)> = objects
        .iter()
        .map(|object| (*object, (things(left, *object), things(right, *object))))
        .collect();
    for (object, (mine, theirs)) in &all_things {
        let index: HashMap<u64, usize> = theirs
            .iter()
            .enumerate()
            .map(|(j, thing)| (thing.id, j))
            .collect();
        let found = mine
            .iter()
            .enumerate()
            .filter_map(|(i, thing)| Some((i, *index.get(&thing.id)?)))
            .collect();
        thing_pairs.insert(*object, found);
    }

    // Atoms paired by constitution or structure, (left, right), by pair.
    let mut atom_pairs: HashMap<(usize, usize), Vec<(u64, u64)>> = HashMap::new();
    let molecule_entity = |i: usize, j: usize, by: MatchedBy, atoms: &[(u64, u64)]| -> Entity {
        let (atoms_l, atoms_r) = (l.atoms(i).to_vec(), r.atoms(j).to_vec());
        let ordered: Option<Vec<(u64, u64)>> = match (l.signature(i), r.signature(j)) {
            (Some(Ok(a)), Some(Ok(b))) if by == MatchedBy::Smiles && a.drawn == b.drawn => Some(
                a.order
                    .iter()
                    .zip(&b.order)
                    .map(|(a, b)| (*b, *a))
                    .collect(),
            ),
            _ => None,
        };
        let points = |correspondence: &[(u64, u64)]| -> Vec<(Xy, Xy)> {
            correspondence
                .iter()
                .filter_map(|(b, a)| Some((r.position(*b)?, l.position(*a)?)))
                .collect()
        };
        let points: Vec<(Xy, Xy)> = match by {
            MatchedBy::Id => {
                let mine: HashSet<u64> = atoms_l.iter().copied().collect();
                let shared: Vec<(u64, u64)> = atoms_r
                    .iter()
                    .filter(|id| mine.contains(id))
                    .map(|id| (*id, *id))
                    .collect();
                points(&shared)
            }
            // Canonical order pairs symmetric atoms either way round; the
            // nearest same-element atoms may pair them as drawn. The better
            // fit wins.
            MatchedBy::Smiles => {
                let canonical = points(&ordered.unwrap_or_default());
                let nearest = points(&nearest(&l, &atoms_l, &r, &atoms_r));
                // The first model that fits, then how well: a rigid fit wins
                // over a mirror, and a mirror drawn anew pairs as mirrored.
                let score = |pairs: &[(Xy, Xy)]| {
                    let models = [(false, false), (true, false), (false, true), (true, true)];
                    (models.iter().enumerate())
                        .filter_map(|(n, (mirror, scale))| {
                            Some((n, Fit::similar(pairs, *mirror, *scale)?))
                        })
                        .find(|(_, fit)| fit.rms <= RMS * unit && fit.max <= MAX * unit)
                        .map_or_else(
                            || {
                                (
                                    models.len(),
                                    Fit::rigid(pairs).map_or(f64::INFINITY, |f| f.rms),
                                )
                            },
                            |(n, fit)| (n, fit.rms),
                        )
                };
                let (a, b) = (score(&nearest), score(&canonical));
                if nearest.len() == canonical.len() && (a.0, a.1) <= (b.0, b.1 + 1e-6) {
                    nearest
                } else {
                    canonical
                }
            }
            // Unpaired when too large or out of budget: as drawn, if that
            // keeps every bond.
            MatchedBy::Constitution if atoms.is_empty() => {
                points(&nearest(&l, &atoms_l, &r, &atoms_r))
            }
            MatchedBy::Constitution => {
                points(&atoms.iter().map(|(a, b)| (*b, *a)).collect::<Vec<_>>())
            }
            // Atoms an edit nearby may have moved are left out.
            MatchedBy::Structure => points(
                &unchanged(&l, &r, atoms)
                    .into_iter()
                    .map(|(a, b)| (b, a))
                    .collect::<Vec<_>>(),
            ),
        };
        if points.is_empty() {
            let pairs = vec![(r.centroid(j), l.centroid(i))];
            let mut entity = Entity::new(Subject::Molecule, atoms_l, atoms_r, pairs);
            entity.fit = Fit::translation(&entity.pairs).unwrap_or(Fit::IDENTITY);
            entity.loose = true;
            return entity;
        }
        let mut entity = Entity::new(Subject::Molecule, atoms_l, atoms_r, points);
        entity.reflect_or_scale(unit);
        entity
    };
    let thing_entity = |object: Object, i: usize, j: usize| -> Option<Entity> {
        let (mine, theirs) = all_things.get(&object)?;
        let (a, b) = (mine.get(i)?, theirs.get(j)?);
        let subject = match object {
            Object::Text => Subject::Text,
            Object::Arrow => Subject::Arrow,
            Object::Graphic => Subject::Graphic,
        };
        let mut entity = Entity::new(
            subject,
            vec![a.id],
            vec![b.id],
            zip_points(&b.points, &a.points),
        );
        entity.loose = a.points.len() != b.points.len();
        entity.reflect_or_scale(unit);
        Some(entity)
    };
    let entities = |pairs: &[(usize, usize, MatchedBy)],
                    thing_pairs: &HashMap<Object, Vec<(usize, usize)>>,
                    atom_pairs: &HashMap<(usize, usize), Vec<(u64, u64)>>| {
        let mut all: Vec<Entity> = pairs
            .iter()
            .map(|(i, j, by)| {
                let atoms = atom_pairs.get(&(*i, *j)).map_or(&[][..], Vec::as_slice);
                molecule_entity(*i, *j, *by, atoms)
            })
            .collect();
        for object in objects {
            for (i, j) in thing_pairs.get(&object).into_iter().flatten() {
                all.extend(thing_entity(object, *i, *j));
            }
        }
        all
    };

    // What is left, by content: molecules by SMILES, nearest first after the
    // move the id matches share; texts by their words, arrows and graphics by
    // kind.
    let guess = common(&entities(&pairs, &thing_pairs, &atom_pairs), unit)
        .map_or(Fit::IDENTITY, |(f, _)| f);
    let paired_l: HashSet<usize> = pairs.iter().map(|(i, ..)| *i).collect();
    let paired_r: HashSet<usize> = pairs.iter().map(|(_, j, _)| *j).collect();
    let open = |side: &Side, paired: &HashSet<usize>| -> Vec<(usize, (String, String), Xy)> {
        (0..side.molecules.len())
            .filter(|i| !paired.contains(i))
            .filter_map(|i| match side.signature(i) {
                Some(Ok(sig)) => {
                    Some((i, (sig.smiles.clone(), sig.extra.clone()), side.centroid(i)))
                }
                _ => None,
            })
            .collect()
    };
    let found = by_content(&open(&l, &paired_l), &open(&r, &paired_r), &guess);
    pairs.extend(found.into_iter().map(|(i, j)| (i, j, MatchedBy::Smiles)));
    for (object, (mine, theirs)) in &all_things {
        let taken = thing_pairs.entry(*object).or_default();
        let (used_l, used_r): (HashSet<usize>, HashSet<usize>) = taken.iter().copied().unzip();
        let open = |list: &[Thing], used: &HashSet<usize>| -> Vec<(usize, Value, Xy)> {
            list.iter()
                .enumerate()
                .filter(|(i, _)| !used.contains(i))
                .filter_map(|(i, thing)| {
                    Some((i, thing.content.clone(), xy(*thing.points.first()?)))
                })
                .collect()
        };
        let found = by_content(&open(mine, &used_l), &open(theirs, &used_r), &guess);
        taken.extend(found);
    }

    // Drawn again with new ids, as a re-import does: by SMILES without
    // stereo, then by structure, nearest first after the move all matches
    // so far share.
    let guess = common(&entities(&pairs, &thing_pairs, &atom_pairs), unit)
        .map_or(Fit::IDENTITY, |(f, _)| f);
    let mut work = structure::TOTAL;
    let unpaired = |side: &Side, pairs: &[(usize, usize, MatchedBy)], mine: bool| -> Vec<usize> {
        let paired: HashSet<usize> = pairs
            .iter()
            .map(|(i, j, _)| if mine { *i } else { *j })
            .collect();
        (0..side.molecules.len())
            .filter(|i| !paired.contains(i) && matches!(side.signature(*i), Some(Ok(_))))
            .collect()
    };
    let constitution = |side: &Side, open: &[usize]| -> Vec<(usize, String, Xy)> {
        open.iter()
            .filter_map(|i| match side.signature(*i) {
                Some(Ok(sig)) => Some((*i, sig.constitution.clone(), side.centroid(*i))),
                _ => None,
            })
            .collect()
    };
    let found = by_content(
        &constitution(&l, &unpaired(&l, &pairs, true)),
        &constitution(&r, &unpaired(&r, &pairs, false)),
        &guess,
    );
    for (i, j) in found {
        let atoms = structure::constitution((&l, i), (&r, j), &guess, unit, &mut work);
        atom_pairs.insert((i, j), atoms);
        pairs.push((i, j, MatchedBy::Constitution));
    }
    let guess = common(&entities(&pairs, &thing_pairs, &atom_pairs), unit)
        .map_or(Fit::IDENTITY, |(f, _)| f);
    let found = structure::matches(
        (&l, &unpaired(&l, &pairs, true)),
        (&r, &unpaired(&r, &pairs, false)),
        &guess,
        unit,
        &mut work,
    );
    for (i, j, atoms) in found {
        atom_pairs.insert((i, j), atoms);
        pairs.push((i, j, MatchedBy::Structure));
    }
    let (taken_l, taken_r): (HashSet<usize>, HashSet<usize>) =
        pairs.iter().map(|(i, j, _)| (*i, *j)).unzip();
    pairs.extend(
        weak.into_iter()
            .filter(|(i, j)| !taken_l.contains(i) && !taken_r.contains(j))
            .map(|(i, j)| (i, j, MatchedBy::Id)),
    );
    pairs.sort_unstable_by_key(|(i, j, _)| (*i, *j));

    let entities = entities(&pairs, &thing_pairs, &atom_pairs);
    let common = common(&entities, unit);
    let shared = |n: usize| {
        common
            .as_ref()
            .is_some_and(|(_, shared)| shared.get(n) == Some(&true))
    };
    let scheme = common.as_ref().map(|(fit, _)| *fit).filter(|fit| {
        let points = entities
            .iter()
            .flat_map(|e| e.pairs.iter().map(|(from, _)| *from));
        fit.reach(points) > MAX * unit
    });
    report.scheme = scheme;
    if let Some(fit) = scheme {
        let (mut change, mut points) = (Change::new(Kind::SchemeMoved, Subject::Scheme), vec![]);
        for entity in entities
            .iter()
            .enumerate()
            .filter(|(n, _)| shared(*n))
            .map(|(_, e)| e)
        {
            change.left.objects.extend(&entity.left);
            change.right.objects.extend(&entity.right);
            points.extend(entity.pairs.iter().copied());
        }
        change.detail = movement(&fit, &points);
        report.changes.push(change);
    }

    // Each entity's own change of place or shape, and how to line it up.
    // One the scheme's move explains has none of its own, and lines up by
    // it, so a flip of the whole scheme is undone on every molecule.
    for (n, entity) in entities.iter().enumerate() {
        let explained = scheme.filter(|_| shared(n));
        report
            .alignment
            .push((entity.right.clone(), explained.unwrap_or(entity.fit)));
        let kind = if explained.is_some() {
            None
        } else if entity.reshaped(unit) {
            Some(Kind::Redrawn)
        } else if entity.displaced(unit) || entity.fit.mirror || entity.fit.scale != 1. {
            Some(entity.pose(unit))
        } else {
            None
        };
        if let Some(kind) = kind {
            let mut change = Change::new(kind, entity.subject);
            change.left = marks(&entity.left);
            change.right = marks(&entity.right);
            change.detail = movement(&entity.fit, &entity.pairs);
            report.changes.push(change);
        }
    }
    let arrow_map: HashMap<u64, u64> = thing_pairs
        .get(&Object::Arrow)
        .into_iter()
        .flatten()
        .filter_map(|(i, j)| {
            let (mine, theirs) = all_things.get(&Object::Arrow)?;
            Some((mine.get(*i)?.id, theirs.get(*j)?.id))
        })
        .collect();

    // Matched molecules as chemistry, style and reaction roles. Style is
    // compared lined up, so what a turn or a flip does to wedges, hydrogens
    // and number offsets is not a change of style.
    let lined = aligned(right, &report);
    for (i, j, by) in &pairs {
        let (atoms_l, atoms_r) = (l.atoms(*i), r.atoms(*j));
        let (sig_l, sig_r) = (l.signature(*i), r.signature(*j));
        let smiles =
            [sig_l, sig_r].map(|sig| sig.and_then(|s| s.as_ref().ok()).map(|s| s.smiles.clone()));
        let both = match (sig_l, sig_r) {
            (Some(Ok(a)), Some(Ok(b))) => Some((a, b)),
            _ => None,
        };
        let chemistry = match (both, sig_l, sig_r) {
            (Some((a, b)), ..) if a.same(b) => Chemistry::Same,
            (Some(_), ..) => Chemistry::Changed,
            (None, Some(Err(reason)), _) | (None, _, Some(Err(reason))) => {
                Chemistry::Unverified(reason.clone())
            }
            (None, ..) => Chemistry::Unverified("Not read".into()),
        };
        if let Chemistry::Unverified(reason) = &chemistry {
            let mut change = Change::new(Kind::Unverified, Subject::Molecule);
            change.left = marks(atoms_l);
            change.right = marks(atoms_r);
            change.before.clone_from(&smiles[0]);
            change.after.clone_from(&smiles[1]);
            change.detail = Detail::Reason(reason.clone());
            report.changes.push(change);
        }
        if let (Chemistry::Changed, Some((a, b))) = (&chemistry, both) {
            let mut change = Change::new(Kind::Chemical, Subject::Molecule);
            change.left = marks(atoms_l);
            change.right = marks(atoms_r);
            // Atoms by their ids, or as constitution or structure paired them.
            let map: HashMap<u64, u64> = if *by == MatchedBy::Id {
                (a.atoms.keys())
                    .filter(|id| b.atoms.contains_key(id))
                    .map(|id| (*id, *id))
                    .collect()
            } else {
                atom_pairs
                    .get(&(*i, *j))
                    .into_iter()
                    .flatten()
                    .copied()
                    .collect()
            };
            if !map.is_empty() {
                let edit = chemistry::edit_mapped(a, b, &map);
                change.left.atoms = edit.removed.clone();
                change.right.atoms = edit.added.clone();
                change.left.bonds = edit.removed_bonds.clone();
                change.right.bonds = edit.added_bonds.clone();
                for (l, r) in &edit.changed {
                    change.left.atoms.push(*l);
                    change.right.atoms.push(*r);
                }
                for (l, r) in &edit.changed_bonds {
                    change.left.bonds.push(*l);
                    change.right.bonds.push(*r);
                }
                let parts = chemistry::parts(a, b, &edit, &map, |l, r| locant(left, right, l, r));
                change.detail = Detail::Edit {
                    added: chemistry::formula(b, &edit.added),
                    removed: chemistry::formula(a, &edit.removed),
                    changed: edit.changed.len(),
                    stereo: edit.stereo,
                    mirror_image: edit.mirror_image,
                    parts,
                    by_structure: *by == MatchedBy::Structure,
                };
            } else {
                change.left.atoms = atoms_l.to_vec();
                change.right.atoms = atoms_r.to_vec();
            }
            change.before.clone_from(&smiles[0]);
            change.after.clone_from(&smiles[1]);
            report.changes.push(change);
        }
        if *by == MatchedBy::Id {
            let common: Vec<u64> = atoms_l
                .iter()
                .filter(|id| right.atom(**id).is_some())
                .copied()
                .collect();
            let mirrored = (report.alignment.iter())
                .any(|(ids, fit)| fit.mirror && ids.first() == atoms_r.first());
            let restyled = restyled(
                left,
                &lined,
                &common,
                chemistry == Chemistry::Same,
                mirrored,
            );
            if !restyled.is_empty() {
                let mut change = Change::new(Kind::Style, Subject::Molecule);
                change.left = marks(atoms_l);
                change.right = marks(atoms_r);
                change.left.atoms.clone_from(&restyled);
                change.right.atoms = restyled;
                report.changes.push(change);
            }
        }
        let was = roles(left, atoms_l, |arrow| arrow_map.get(&arrow).copied());
        let now = roles(right, atoms_r, Some);
        if was != now {
            let mut change = Change::new(Kind::Reaction, Subject::Molecule);
            change.left = marks(atoms_l);
            change.right = marks(atoms_r);
            change.before = Some(describe_roles(&was));
            change.after = Some(describe_roles(&now));
            report.changes.push(change);
        }
        report.molecules.push(Pair {
            left: atoms_l.to_vec(),
            right: atoms_r.to_vec(),
            by: *by,
            chemistry,
            smiles,
        });
    }

    // Texts, arrows and graphics: words, kind and style.
    for object in objects {
        let Some((mine, theirs)) = all_things.get(&object) else {
            continue;
        };
        let subject = match object {
            Object::Text => Subject::Text,
            Object::Arrow => Subject::Arrow,
            Object::Graphic => Subject::Graphic,
        };
        for (i, j) in thing_pairs.get(&object).into_iter().flatten() {
            let (Some(a), Some(b)) = (mine.get(*i), theirs.get(*j)) else {
                continue;
            };
            let (style_a, rest_a) = look(left, object, a.id);
            let (style_b, rest_b) = look(right, object, b.id);
            let one = |kind: Kind| {
                let mut change = Change::new(kind, subject);
                change.left = marks(&[a.id]);
                change.right = marks(&[b.id]);
                change
            };
            if a.content != b.content {
                let kind = match object {
                    Object::Text => Kind::Text,
                    Object::Arrow => Kind::Reaction,
                    Object::Graphic => Kind::Redrawn,
                };
                let mut change = one(kind);
                let words = |value: &Value| value.as_str().map(str::to_owned);
                change.before = words(&a.content);
                change.after = words(&b.content);
                report.changes.push(change);
            } else if rest_a != rest_b {
                let kind = match object {
                    Object::Text => Kind::Text,
                    Object::Arrow | Object::Graphic => Kind::Redrawn,
                };
                let mut change = one(kind);
                if object != Object::Graphic {
                    change.detail = Detail::Reason("Attached differently".into());
                }
                report.changes.push(change);
            }
            if style_a != style_b {
                report.changes.push(one(Kind::Style));
            }
        }
        let (used_l, used_r): (HashSet<usize>, HashSet<usize>) = thing_pairs
            .get(&object)
            .into_iter()
            .flatten()
            .copied()
            .unzip();
        for thing in mine
            .iter()
            .enumerate()
            .filter(|(i, _)| !used_l.contains(i))
            .map(|(_, thing)| thing)
        {
            let mut change = Change::new(Kind::Removed, subject);
            change.left = marks(&[thing.id]);
            change.before = thing.content.as_str().map(str::to_owned);
            report.changes.push(change);
        }
        for thing in theirs
            .iter()
            .enumerate()
            .filter(|(j, _)| !used_r.contains(j))
            .map(|(_, thing)| thing)
        {
            let mut change = Change::new(Kind::Added, subject);
            change.right = marks(&[thing.id]);
            change.after = thing.content.as_str().map(str::to_owned);
            report.changes.push(change);
            if let Some(fit) = scheme {
                report.alignment.push((vec![thing.id], fit));
            }
        }
    }

    // Molecules on one side only.
    let paired_l: HashSet<usize> = pairs.iter().map(|(i, ..)| *i).collect();
    let paired_r: HashSet<usize> = pairs.iter().map(|(_, j, _)| *j).collect();
    let smiles = |side: &Side, i: usize| {
        side.signature(i)
            .and_then(|sig| sig.as_ref().ok())
            .map(|sig| sig.smiles.clone())
    };
    for i in (0..l.molecules.len()).filter(|i| !paired_l.contains(i)) {
        let mut change = Change::new(Kind::Removed, Subject::Molecule);
        change.left = marks(l.atoms(i));
        change.left.atoms = l.atoms(i).to_vec();
        change.before = smiles(&l, i);
        report.changes.push(change);
    }
    for j in (0..r.molecules.len()).filter(|j| !paired_r.contains(j)) {
        let mut change = Change::new(Kind::Added, Subject::Molecule);
        change.right = marks(r.atoms(j));
        change.right.atoms = r.atoms(j).to_vec();
        change.after = smiles(&r, j);
        report.changes.push(change);
        if let Some(fit) = scheme {
            report.alignment.push((r.atoms(j).to_vec(), fit));
        }
    }

    // Hydrogen bonds and other order-0 links: whether they are there. How
    // they look is style.
    let links = |doc: &Document| -> HashSet<(u64, u64)> {
        doc.bonds
            .iter()
            .filter(|bond| bond.order == 0)
            .map(|bond| key(bond.a, bond.b))
            .collect()
    };
    // Those within one molecule belong to its chemistry and style.
    let between = |side: &Side, (a, b): &(u64, u64)| side.owner.get(a) != side.owner.get(b);
    let (mine, theirs): (HashSet<_>, HashSet<_>) = (
        links(left)
            .into_iter()
            .filter(|pair| between(&l, pair))
            .collect(),
        links(right)
            .into_iter()
            .filter(|pair| between(&r, pair))
            .collect(),
    );
    let mut changed: Vec<&(u64, u64)> = mine.symmetric_difference(&theirs).collect();
    changed.sort_unstable();
    for (a, b) in changed {
        let mut change = Change::new(Kind::Chemical, Subject::Molecule);
        for (side, links, doc) in [
            (&mut change.left, &mine, left),
            (&mut change.right, &theirs, right),
        ] {
            if links.contains(&(*a, *b)) {
                side.objects = [*a, *b]
                    .into_iter()
                    .filter(|id| doc.atom(*id).is_some())
                    .collect();
                side.atoms.clone_from(&side.objects);
                side.bonds = vec![(*a, *b)];
            }
        }
        change.detail = Detail::Reason("Hydrogen bond or other link changed".into());
        report.changes.push(change);
    }
    let look = |doc: &Document, pair: &(u64, u64)| {
        doc.bonds
            .iter()
            .find(|bond| bond.order == 0 && key(bond.a, bond.b) == *pair)
            .map(|bond| without(json(bond), &["a", "b"]))
    };
    let mut kept: Vec<&(u64, u64)> = mine.intersection(&theirs).collect();
    kept.sort_unstable();
    for pair in kept {
        if look(left, pair) != look(right, pair) {
            let mut change = Change::new(Kind::Style, Subject::Molecule);
            for side in [&mut change.left, &mut change.right] {
                side.objects = vec![pair.0, pair.1];
            }
            report.changes.push(change);
        }
    }

    if settings(left) != settings(right) {
        report
            .changes
            .push(Change::new(Kind::Style, Subject::Settings));
    }
    report
}

/// A copy of `right` with each object lined up with its counterpart on the
/// left, by the report's alignment: the right side in the left side's frame.
pub fn aligned(right: &Document, report: &Report) -> Document {
    let mut doc = right.clone();
    let mut moves: HashMap<u64, Fit> = HashMap::new();
    for (ids, fit) in &report.alignment {
        for id in ids {
            moves.insert(*id, *fit);
        }
        // Hydrogens, offsets and wedges turn, scale and reflect with the
        // atoms, so a mirror undone keeps the configuration shown.
        let atoms: Vec<u64> = ids
            .iter()
            .filter(|id| doc.atom(**id).is_some())
            .copied()
            .collect();
        if !atoms.is_empty() {
            crate::editing::transform_display(&mut doc, &atoms, |p| {
                let (x, y) = fit.rotate(xy(p));
                Point::new(x as f32, y as f32)
            });
        }
    }
    let point = |fit: &Fit, p: Point| {
        let (x, y) = fit.apply(xy(p));
        Point::new(x as f32, y as f32)
    };
    let vector = |fit: &Fit, p: Point| {
        let (x, y) = fit.rotate(xy(p));
        Point::new(x as f32, y as f32)
    };
    for atom in &mut doc.atoms {
        if let Some(fit) = moves.get(&atom.id) {
            atom.position = point(fit, atom.position);
        }
    }
    for note in &mut doc.annotations {
        if let Some(fit) = moves.get(&note.id) {
            note.position = point(fit, note.position);
        }
    }
    for arrow in &mut doc.arrows {
        if let Some(fit) = moves.get(&arrow.id) {
            arrow.map_points(|p| point(fit, p));
        }
    }
    for graphic in &mut doc.graphics {
        if let Some(fit) = moves.get(&graphic.id) {
            graphic.origin = point(fit, graphic.origin);
            graphic.axis_x = vector(fit, graphic.axis_x);
            graphic.axis_y = vector(fit, graphic.axis_y);
        }
    }
    doc
}
