//! Molecules matched by structure: what ids, SMILES and constitution left
//! unmatched on both sides, paired when most of their bonds correspond.
//! Bonds count only kept with the same order and ring membership, so a
//! chain is never a ring. A pair must be each molecule's clear best: a
//! rival as good, or one the budget left unknown that could be, leaves both
//! unmatched. Distance only breaks a tie. The searches are bounded by work,
//! not time, so the same drawings always match the same way.
use super::{Side, fit::Fit};
use crate::reaction_mapping::{Budget, Graph, correspond};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// At most this many unmatched molecules a side; beyond, none match.
const MOLECULES: usize = 32;
/// The mapper's limit on one side.
const ATOMS: usize = 128;
/// One pass through two molecules of 128 atoms costs about 7,000,000.
const PAIR: Budget = Budget {
    states: 20_000,
    work: 10_000_000,
};
/// Shared by every search of one comparison.
pub(super) const TOTAL: u64 = 40_000_000;
/// Heavy-atom bonds two molecules must share.
const BONDS: u32 = 3;

/// One unmatched molecule as the mapper reads it.
struct Molecule {
    index: usize,
    graph: Graph,
    /// Heavy-atom bonds by (element, element, order, in a ring).
    classes: BTreeMap<(String, String, u8, bool), u32>,
    bonds: u32,
    /// Each ring atom's ring system, by atom index, and how many there are.
    system: Vec<Option<usize>>,
    count: usize,
    /// Atom indices by id.
    at: HashMap<u64, usize>,
}

impl Molecule {
    fn new(side: &Side, index: usize) -> Option<Self> {
        let graph = Graph::compared(side.doc, side.atoms(index)).ok()?;
        let element = |i: usize| graph.atoms.get(i).map(|atom| atom.element.as_str());
        let mut classes = BTreeMap::new();
        for (&(a, b), class) in &graph.edges {
            let (Some(x), Some(y)) = (element(a), element(b)) else {
                continue;
            };
            if x != "H" && y != "H" {
                let (x, y) = (x.min(y).to_owned(), x.max(y).to_owned());
                *classes.entry((x, y, class.order, class.ring)).or_default() += 1;
            }
        }
        // Ring systems: atoms joined by ring bonds.
        let mut system: Vec<Option<usize>> = vec![None; graph.atoms.len()];
        let mut count = 0;
        for start in 0..graph.atoms.len() {
            let ring = |(a, b): &(usize, usize)| *a == start || *b == start;
            if system.get(start).copied().flatten().is_some()
                || !graph.edges.iter().any(|(e, c)| c.ring && ring(e))
            {
                continue;
            }
            let mut stack = vec![start];
            while let Some(at) = stack.pop() {
                if let Some(slot) = system.get_mut(at).filter(|slot| slot.is_none()) {
                    *slot = Some(count);
                    stack.extend(
                        (graph.edges.iter())
                            .filter(|((a, b), c)| c.ring && (*a == at || *b == at))
                            .map(|((a, b), _)| if *a == at { *b } else { *a }),
                    );
                }
            }
            count += 1;
        }
        Some(Self {
            index,
            bonds: classes.values().sum(),
            classes,
            system,
            count,
            at: (graph.atoms.iter().enumerate())
                .map(|(i, atom)| (atom.id, i))
                .collect(),
            graph,
        })
    }

    /// Small enough to search.
    fn searchable(&self) -> bool {
        self.graph.atoms.len() <= ATOMS
    }

    /// The most bonds any correspondence could keep with `other`.
    fn bound(&self, other: &Self) -> u32 {
        (self.classes.iter())
            .map(|(class, n)| (*n).min(other.classes.get(class).copied().unwrap_or(0)))
            .sum()
    }
}

/// `kept` bonds as a similarity, in twentieths, rounded: kept over the
/// bonds of either.
fn step(kept: u32, a: u32, b: u32) -> u32 {
    let all = (a + b).saturating_sub(kept).max(1);
    (40 * kept + all) / (2 * all)
}

/// Enough bonds kept, at least half of the bonds of either.
fn similar(kept: u32, a: u32, b: u32) -> bool {
    kept >= BONDS && 2 * kept >= (a + b).saturating_sub(kept)
}

/// Every ring system of each side pairs with exactly one of the other's,
/// by ring bonds kept between them.
fn rings_correspond(left: &Molecule, right: &Molecule, pairs: &[(u64, u64)]) -> bool {
    let map: HashMap<u64, u64> = pairs.iter().copied().collect();
    let to_right = |i: usize| {
        right
            .at
            .get(map.get(&left.graph.atoms.get(i)?.id)?)
            .copied()
    };
    let mut links = BTreeSet::new();
    for (&(a, b), class) in left.graph.edges.iter().filter(|(_, class)| class.ring) {
        let (Some(x), Some(y)) = (to_right(a), to_right(b)) else {
            continue;
        };
        let kept = (right.graph.edges.get(&(x.min(y), x.max(y))))
            .is_some_and(|other| (other.order, other.ring) == (class.order, class.ring));
        if let (true, Some(Some(s)), Some(Some(t))) =
            (kept, left.system.get(a), right.system.get(x))
        {
            links.insert((*s, *t));
        }
    }
    let ends = |pick: fn(&(usize, usize)) -> usize| links.iter().map(pick).collect::<BTreeSet<_>>();
    links.len() == left.count
        && ends(|link| link.0).len() == left.count
        && ends(|link| link.1).len() == right.count
}

#[cfg(test)]
mod tests;

/// What is known of one candidate pair.
struct Candidate {
    left: usize,
    right: usize,
    /// The similarity found, if it is acceptable.
    found: Option<u32>,
    /// The most it could be: as found if the search finished.
    most: u32,
    complete: bool,
    distance: f64,
    pairs: Vec<(u64, u64)>,
}

/// A cost for pairing atom `l` of molecule `i` with atom `r` of molecule
/// `j`: their distance in hundredths of `unit` once `guess` carries the
/// right side over and the two centroids meet.
pub(super) fn cost<'a>(
    (left, i): (&'a Side, usize),
    (right, j): (&'a Side, usize),
    guess: &'a Fit,
    unit: f64,
) -> impl Fn(u64, u64) -> u64 + 'a {
    let (to, from) = (left.centroid(i), guess.apply(right.centroid(j)));
    move |l, r| {
        let (Some(a), Some(b)) = (left.position(l), right.position(r)) else {
            return 0;
        };
        let b = guess.apply(b);
        let d = (a.0 - (b.0 - from.0 + to.0)).hypot(a.1 - (b.1 - from.1 + to.1));
        // Bounded, so 128 of them always add up.
        (d / unit * 100.).round().min(1e12) as u64
    }
}

/// The atoms of two drawings of one constitution paired, nearest first;
/// empty unless the pairing keeps every bond within `work`.
pub(super) fn constitution(
    (left, i): (&Side, usize),
    (right, j): (&Side, usize),
    guess: &Fit,
    unit: f64,
    work: &mut u64,
) -> Vec<(u64, u64)> {
    if *work == 0 {
        return Vec::new();
    }
    let (Some(a), Some(b)) = (Molecule::new(left, i), Molecule::new(right, j)) else {
        return Vec::new();
    };
    if !a.searchable() || !b.searchable() {
        return Vec::new();
    }
    let budget = Budget {
        work: PAIR.work.min(*work),
        ..PAIR
    };
    let outcome = correspond(
        &a.graph,
        &b.graph,
        budget,
        cost((left, i), (right, j), guess, unit),
    );
    *work = work.saturating_sub(outcome.work);
    if outcome.kept == a.bonds && a.bonds == b.bonds {
        outcome.pairs
    } else {
        Vec::new()
    }
}

/// Two molecules matched, (left, right), and their atoms as (left id,
/// right id).
pub(super) type Matched = (usize, usize, Vec<(u64, u64)>);

/// Pairs molecules `open_l` and `open_r`, unmatched and readable, by
/// structure. `work` is what is left of [`TOTAL`].
pub(super) fn matches(
    (left, open_l): (&Side, &[usize]),
    (right, open_r): (&Side, &[usize]),
    guess: &Fit,
    unit: f64,
    work: &mut u64,
) -> Vec<Matched> {
    if open_l.len() > MOLECULES || open_r.len() > MOLECULES {
        return Vec::new();
    }
    let mine: Vec<Molecule> = open_l
        .iter()
        .filter_map(|i| Molecule::new(left, *i))
        .collect();
    let theirs: Vec<Molecule> = open_r
        .iter()
        .filter_map(|j| Molecule::new(right, *j))
        .collect();
    let mut candidates = Vec::new();
    for a in &mine {
        for b in &theirs {
            let bound = a.bound(b);
            if a.count != b.count || !similar(bound, a.bonds, b.bonds) {
                continue;
            }
            let moved = guess.apply(right.centroid(b.index));
            let at = left.centroid(a.index);
            let mut candidate = Candidate {
                left: a.index,
                right: b.index,
                found: None,
                most: step(bound, a.bonds, b.bonds),
                complete: false,
                distance: (moved.0 - at.0).hypot(moved.1 - at.1),
                pairs: Vec::new(),
            };
            // Too large to search, or out of budget: a rival of unknown worth.
            if *work > 0 && a.searchable() && b.searchable() {
                let budget = Budget {
                    work: PAIR.work.min(*work),
                    ..PAIR
                };
                let outcome = correspond(
                    &a.graph,
                    &b.graph,
                    budget,
                    cost((left, a.index), (right, b.index), guess, unit),
                );
                *work = work.saturating_sub(outcome.work);
                let kept = outcome.kept;
                let enough = similar(kept, a.bonds, b.bonds);
                let rings = rings_correspond(a, b, &outcome.pairs);
                candidate.found = (enough && rings).then(|| step(kept, a.bonds, b.bonds));
                // A pairing whose rings do not correspond leaves unknown how
                // good one that does would be, up to what this one kept.
                candidate.complete = outcome.complete && (rings || !enough);
                if outcome.complete {
                    candidate.most = step(kept, a.bonds, b.bonds);
                }
                candidate.pairs = outcome.pairs;
            }
            candidates.push(candidate);
        }
    }
    let best = |side: fn(&Candidate) -> usize| -> HashMap<usize, usize> {
        let mut groups = BTreeMap::<usize, Vec<(usize, &Candidate)>>::new();
        for (n, candidate) in candidates.iter().enumerate() {
            groups
                .entry(side(candidate))
                .or_default()
                .push((n, candidate));
        }
        groups
            .into_iter()
            .filter_map(|(molecule, group)| Some((molecule, clear_best(&group)?)))
            .collect()
    };
    let (by_left, by_right) = (best(|c| c.left), best(|c| c.right));
    let mut found = Vec::new();
    for (molecule, n) in &by_left {
        let Some(candidate) = candidates.get(*n) else {
            continue;
        };
        if by_right.get(&candidate.right) == Some(n) {
            found.push((*molecule, candidate.right, candidate.pairs.clone()));
        }
    }
    found
}

/// The candidate, by index, that is clearly one molecule's best: the most
/// similar acceptable one, any equal rival at least twice as far, and no
/// unfinished or skipped rival that could reach it.
fn clear_best(group: &[(usize, &Candidate)]) -> Option<usize> {
    let (n, best) = group
        .iter()
        .filter(|(_, c)| c.found.is_some())
        .max_by(|(_, a), (_, b)| (a.found.cmp(&b.found)).then(b.distance.total_cmp(&a.distance)))?;
    let similarity = best.found?;
    let clear = group.iter().filter(|(m, _)| m != n).all(|(_, rival)| {
        if !rival.complete {
            rival.most < similarity
        } else if rival.found == Some(similarity) {
            best.distance < rival.distance && best.distance <= rival.distance / 2.
        } else {
            rival.found.is_none_or(|s| s < similarity)
        }
    });
    clear.then_some(*n)
}
