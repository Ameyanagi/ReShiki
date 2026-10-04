//! A keyboard drawing hotspot, independent of pointer hover and selection.
use crate::{
    bonds::BondPreset,
    document::{Document, Point},
    templates,
};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Blank(Point),
    Atom(u64),
    Bond(u64, u64),
}

impl Target {
    pub fn point(self, doc: &Document) -> Option<Point> {
        match self {
            Self::Blank(point) => Some(point),
            Self::Atom(id) => doc.atom(id).map(|atom| atom.position),
            Self::Bond(a, b) => doc
                .bonds
                .iter()
                .any(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
                .then(|| doc.atom(a).zip(doc.atom(b)))
                .flatten()
                .map(|(a, b)| {
                    Point::new(
                        (a.position.x + b.position.x) / 2.,
                        (a.position.y + b.position.y) / 2.,
                    )
                }),
        }
    }

    pub fn atoms(self) -> Vec<u64> {
        match self {
            Self::Blank(_) => vec![],
            Self::Atom(id) => vec![id],
            Self::Bond(a, b) => vec![a, b],
        }
    }

    fn identity(self) -> (u8, u64, u64) {
        match self {
            Self::Blank(_) => (0, 0, 0),
            Self::Atom(id) => (1, id, id),
            Self::Bond(a, b) => (2, a.min(b), a.max(b)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn from_delta(x: f32, y: f32) -> Option<Self> {
        if x.abs() > y.abs() {
            Some(if x < 0. { Self::Left } else { Self::Right })
        } else if y != 0. {
            Some(if y < 0. { Self::Up } else { Self::Down })
        } else {
            None
        }
    }

    fn vector(self) -> (f32, f32) {
        match self {
            Self::Left => (-1., 0.),
            Self::Right => (1., 0.),
            Self::Up => (0., -1.),
            Self::Down => (0., 1.),
        }
    }
}

fn visible_atom(doc: &Document, id: u64) -> bool {
    doc.atom(id).is_some_and(|atom| atom.centroid.is_empty())
        && !doc
            .abbreviations
            .iter()
            .any(|group| group.anchor != id && group.members.contains(&id))
}

fn hotspots(doc: &Document) -> Vec<Target> {
    let hidden: HashSet<_> = doc
        .abbreviations
        .iter()
        .flat_map(|group| {
            group
                .members
                .iter()
                .copied()
                .filter(move |id| *id != group.anchor)
        })
        .collect();
    let visible: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|atom| atom.centroid.is_empty() && !hidden.contains(&atom.id))
        .map(|atom| atom.id)
        .collect();
    doc.atoms
        .iter()
        .filter(|atom| visible.contains(&atom.id))
        .map(|atom| Target::Atom(atom.id))
        .chain(doc.bonds.iter().filter_map(|bond| {
            (visible.contains(&bond.a) && visible.contains(&bond.b))
                .then_some(Target::Bond(bond.a.min(bond.b), bond.a.max(bond.b)))
        }))
        .collect()
}

fn position(target: Target, points: &HashMap<u64, Point>) -> Option<Point> {
    match target {
        Target::Atom(id) => points.get(&id).copied(),
        Target::Bond(a, b) => points
            .get(&a)
            .zip(points.get(&b))
            .map(|(a, b)| Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)),
        Target::Blank(point) => Some(point),
    }
}

fn connected(current: Target, candidate: Target) -> bool {
    match (current, candidate) {
        (Target::Atom(id), Target::Bond(a, b)) | (Target::Bond(a, b), Target::Atom(id)) => {
            id == a || id == b
        }
        (Target::Bond(a, b), Target::Bond(c, d)) => a == c || a == d || b == c || b == d,
        _ => false,
    }
}

/// Arrows traverse atom → bond → atom. Shift skips the intermediate kind.
/// Connected candidates win; otherwise navigation can reach another fragment.
/// IDs break geometric ties so document storage order does not affect navigation.
pub fn navigate(
    doc: &Document,
    current: Target,
    direction: Direction,
    same_kind: bool,
) -> Option<Target> {
    let origin = current.point(doc)?;
    let (axis_x, axis_y) = direction.vector();
    let points: HashMap<_, _> = doc
        .atoms
        .iter()
        .map(|atom| (atom.id, atom.position))
        .collect();
    let neighbor_atoms: HashSet<_> = match current {
        Target::Atom(id) if same_kind => doc
            .bonds
            .iter()
            .filter_map(|bond| {
                if bond.a == id {
                    Some(bond.b)
                } else if bond.b == id {
                    Some(bond.a)
                } else {
                    None
                }
            })
            .collect(),
        _ => HashSet::new(),
    };
    let mut candidates: Vec<_> = hotspots(doc)
        .into_iter()
        .filter(|candidate| {
            if candidate.identity() == current.identity() {
                return false;
            }
            match current {
                Target::Blank(_) => matches!(candidate, Target::Atom(_)),
                Target::Atom(_) => matches!(candidate, Target::Atom(_)) == same_kind,
                Target::Bond(_, _) => matches!(candidate, Target::Bond(_, _)) == same_kind,
            }
        })
        .filter_map(|candidate| {
            let point = position(candidate, &points)?;
            let dx = point.x - origin.x;
            let dy = point.y - origin.y;
            let forward = dx * axis_x + dy * axis_y;
            if forward <= 0.001 || !forward.is_finite() {
                return None;
            }
            let sideways = (dx * axis_y - dy * axis_x).abs();
            let distance = dx.hypot(dy);
            let score = distance * (1. + 2. * sideways / forward);
            let adjacent = connected(current, candidate)
                || matches!(candidate, Target::Atom(id) if neighbor_atoms.contains(&id));
            score
                .is_finite()
                .then_some((!adjacent, score, candidate.identity(), candidate))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    candidates.first().map(|candidate| candidate.3)
}

#[derive(Debug, Clone, Copy)]
struct Snapshot {
    target: Target,
    position: Point,
    marked: Option<u64>,
}

/// Session-only state. Its history mirrors document commits, including edits
/// made outside keyboard mode, so Undo never leaves a deleted active atom.
pub struct State {
    enabled: bool,
    epoch: u64,
    current: Snapshot,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            epoch: 0,
            current: Snapshot {
                target: Target::Blank(Point::default()),
                position: Point::default(),
                marked: None,
            },
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }
}

impl State {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn target(&self) -> Target {
        self.current.target
    }
    pub fn marked(&self) -> Option<u64> {
        self.current.marked
    }

    pub fn marker_point(&self, doc: &Document) -> Option<Point> {
        self.enabled.then(|| {
            self.current
                .target
                .point(doc)
                .unwrap_or(self.current.position)
        })
    }

    pub fn active_label(&self, doc: &Document) -> String {
        match self.current.target {
            Target::Blank(_) => "Empty drawing position".into(),
            Target::Atom(id) => doc
                .atom(id)
                .map(|atom| {
                    if let Some(group) = doc.abbreviation(id) {
                        format!("{} group at atom {id}", group.label)
                    } else {
                        format!("{} atom {id}", atom.element)
                    }
                })
                .unwrap_or_else(|| "Empty drawing position".into()),
            Target::Bond(a, b) => doc
                .bonds
                .iter()
                .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
                .map(|bond| {
                    crate::bonds::BondPreset::of(bond).map_or_else(
                        || format!("Bond {a}–{b}"),
                        |preset| format!("{preset} bond {a}–{b}"),
                    )
                })
                .unwrap_or_else(|| "Empty drawing position".into()),
        }
    }

    pub fn hint() -> &'static str {
        "Arrows: atom → bond → atom · Shift+arrows: skip · Digits/letters: draw · Enter: label · [: mark atom · ]: close ring · F8/Esc: leave"
    }

    fn check_epoch(&mut self, epoch: u64) {
        if self.epoch != epoch {
            *self = Self {
                epoch,
                ..Self::default()
            };
        }
    }

    pub fn enter(&mut self, doc: &Document, selected: &[u64], center: Point, epoch: u64) {
        self.check_epoch(epoch);
        let target = match selected {
            [id] if visible_atom(doc, *id) => Some(Target::Atom(*id)),
            [a, b]
                if Target::Bond(*a, *b).point(doc).is_some()
                    && visible_atom(doc, *a)
                    && visible_atom(doc, *b) =>
            {
                Some(Target::Bond((*a).min(*b), (*a).max(*b)))
            }
            _ => None,
        }
        .or_else(|| {
            let points: HashMap<_, _> = doc
                .atoms
                .iter()
                .map(|atom| (atom.id, atom.position))
                .collect();
            hotspots(doc).into_iter().min_by(|a, b| {
                let distance = |target: Target| {
                    position(target, &points).map_or(f32::INFINITY, |point| point.distance(center))
                };
                distance(*a)
                    .total_cmp(&distance(*b))
                    .then(a.identity().cmp(&b.identity()))
            })
        })
        .unwrap_or(Target::Blank(center));
        self.enabled = true;
        self.set_target(target, doc);
    }

    pub fn leave(&mut self) {
        self.enabled = false;
        self.current.marked = None;
    }

    pub fn set_target(&mut self, target: Target, doc: &Document) {
        self.current.target = target;
        if let Some(position) = target.point(doc) {
            self.current.position = position;
        }
    }

    pub fn mark(&mut self, id: u64) {
        self.current.marked = Some(id);
    }
    pub fn clear_mark(&mut self) {
        self.current.marked = None;
    }

    pub fn move_target(&mut self, doc: &Document, direction: Direction, same_kind: bool) -> bool {
        if let Some(target) = navigate(doc, self.current.target, direction, same_kind) {
            self.set_target(target, doc);
            true
        } else {
            false
        }
    }

    pub fn reconcile(&mut self, doc: &Document, epoch: u64) {
        self.check_epoch(epoch);
        let valid = match self.current.target {
            Target::Blank(_) => true,
            Target::Atom(id) => visible_atom(doc, id),
            Target::Bond(a, b) => {
                visible_atom(doc, a)
                    && visible_atom(doc, b)
                    && self.current.target.point(doc).is_some()
            }
        };
        if !valid {
            self.current.target = Target::Blank(self.current.position);
        }
        if let Some(position) = self.current.target.point(doc) {
            self.current.position = position;
        }
        self.current.marked = self.current.marked.filter(|id| visible_atom(doc, *id));
    }

    pub fn record(&mut self, before: &Document, after: &Document, epoch: u64, continuing: bool) {
        self.check_epoch(epoch);
        if before == after {
            return;
        }
        // Capture pre-edit positions while deleted or moved atoms still exist.
        if let Some(position) = self.current.target.point(before) {
            self.current.position = position;
        }
        if !continuing || self.undo.is_empty() {
            self.undo.push_back(self.current);
            if self.undo.len() > 100 {
                let _ = self.undo.pop_front();
            }
        }
        self.redo.clear();
        self.reconcile(after, epoch);
    }

    pub fn restore(&mut self, redo: bool, doc: &Document, epoch: u64) {
        self.check_epoch(epoch);
        let (source, destination) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(snapshot) = source.pop_back() {
            destination.push_back(self.current);
            self.current = snapshot;
        }
        self.reconcile(doc, epoch);
    }
}

/// Close a graph edge without moving or merging either atom. Keep chemistry
/// checks stricter than an ordinary pointer-drawn single bond.
pub fn connect_atoms(source: &Document, a: u64, b: u64) -> Result<Document, String> {
    source.validate()?;
    if a == b {
        return Err("Choose a different atom to connect".into());
    }
    if source
        .bonds
        .iter()
        .any(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
    {
        return Err("These atoms already have a bond".into());
    }
    for id in [a, b] {
        let atom = source
            .atom(id)
            .ok_or("The marked or active atom is no longer available")?;
        if !atom.centroid.is_empty()
            || source
                .abbreviations
                .iter()
                .any(|group| group.members.contains(&id))
        {
            return Err("Expand the group before connecting its atoms".into());
        }
        if atom.stereo.is_some()
            || atom.no_implicit
            || atom.explicit_h != 0
            || templates::valence(source, id).saturating_add(2) > templates::capacity(atom)
        {
            return Err(
                "This atom has no available valence; edit its hydrogens or stereochemistry first"
                    .into(),
            );
        }
    }
    let start = source.atom(a).ok_or("Missing marked atom")?.position;
    let end = source.atom(b).ok_or("Missing active atom")?.position;
    if start.distance(end) < 0.001 {
        return Err("The atoms overlap; choose another endpoint".into());
    }
    let mut candidate = source.clone();
    candidate.invalidate_chemistry(&[a, b]);
    BondPreset::Single.place(&mut candidate, a, b);
    candidate.reconcile_molecule_groups();
    crate::reactions::reconcile(&mut candidate)?;
    candidate.validate()?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> (Document, u64, u64, u64) {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        let b = doc.add_atom("N", Point::new(42., -21.));
        let c = doc.add_atom("O", Point::new(84., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        (doc, a, b, c)
    }

    #[test]
    fn arrows_traverse_hotspots_and_shift_skips_without_editing() {
        let (doc, a, b, c) = chain();
        let before = doc.clone();
        let bond = Target::Bond(a, b);
        assert_eq!(
            navigate(&doc, Target::Atom(a), Direction::Right, false),
            Some(bond)
        );
        assert_eq!(
            navigate(&doc, bond, Direction::Right, false),
            Some(Target::Atom(b))
        );
        assert_eq!(
            navigate(&doc, Target::Atom(b), Direction::Left, false),
            Some(bond)
        );
        assert_eq!(
            navigate(&doc, Target::Atom(a), Direction::Right, true),
            Some(Target::Atom(b))
        );
        assert_eq!(
            navigate(&doc, bond, Direction::Right, true),
            Some(Target::Bond(b, c))
        );
        assert_eq!(
            navigate(&doc, Target::Atom(a), Direction::Left, false),
            None
        );
        assert_eq!(doc, before);
    }

    #[test]
    fn geometry_ties_are_stable_and_connected_targets_win() {
        let (mut doc, a, b, _) = chain();
        let nearby = doc.add_atom("C", Point::new(1., 0.));
        let far = doc.add_atom("C", Point::new(2., 0.));
        doc.add_bond(nearby, far, 1, "plain");
        assert_eq!(
            navigate(&doc, Target::Atom(a), Direction::Right, false),
            Some(Target::Bond(a, b))
        );
        let expected = navigate(
            &doc,
            Target::Blank(Point::new(-1., 0.)),
            Direction::Right,
            false,
        );
        doc.atoms.reverse();
        doc.bonds.reverse();
        assert_eq!(
            navigate(
                &doc,
                Target::Blank(Point::new(-1., 0.)),
                Direction::Right,
                false
            ),
            expected
        );
    }

    #[test]
    fn cursor_history_restores_deleted_targets_and_resets_at_file_boundaries() {
        let (before, a, b, _) = chain();
        let mut state = State::default();
        state.enter(&before, &[b], Point::default(), 7);
        state.mark(a);
        let mut after = before.clone();
        after.atoms.retain(|atom| atom.id != b);
        after.bonds.retain(|bond| bond.a != b && bond.b != b);
        state.record(&before, &after, 7, false);
        assert!(matches!(state.target(), Target::Blank(_)));
        state.restore(false, &before, 7);
        assert_eq!(state.target(), Target::Atom(b));
        assert_eq!(state.marked(), Some(a));
        state.restore(true, &after, 7);
        assert!(matches!(state.target(), Target::Blank(_)));
        state.reconcile(&before, 8);
        assert!(!state.enabled());
        assert_eq!(state.marked(), None);
    }
}
