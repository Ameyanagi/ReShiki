//! Bounded aromatic fusion planning. Geometry maps every shared vertex first;
//! the complete sigma skeleton is then checked before assigning any pi bonds.
//! Saturated/stereochemical templates keep the ordinary attachment rules.
use super::{capacity, compatible, valence};
use crate::document::{Atom, Document, Point};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type Edge = (u64, u64);
fn edge(a: u64, b: u64) -> Edge {
    (a.min(b), a.max(b))
}
fn donor(doc: &Document, a: &Atom) -> bool {
    matches!(a.element.as_str(), "O" | "S")
        || (a.element == "N"
            && (a.explicit_h == 1
                || (a.explicit_h == 0
                    && (2..=3).contains(
                        &doc.bonds
                            .iter()
                            .filter(|b| b.a == a.id || b.b == a.id)
                            .count(),
                    )
                    && doc
                        .bonds
                        .iter()
                        .filter(|b| b.a == a.id || b.b == a.id)
                        .all(|b| b.order == 1))))
}
fn eligible(a: &Atom) -> bool {
    matches!(a.element.as_str(), "C" | "N" | "O" | "S")
        && a.charge == 0
        && a.stereo.is_none()
        && a.radical_electrons == 0
        && !a.no_implicit
        && a.marks.is_empty()
        && a.centroid.is_empty()
}
fn nodes(edges: &BTreeSet<Edge>) -> BTreeSet<u64> {
    edges.iter().flat_map(|&(a, b)| [a, b]).collect()
}
fn connected(edges: &BTreeSet<Edge>, seeds: &BTreeSet<u64>) -> BTreeSet<Edge> {
    let mut reached = seeds.clone();
    loop {
        let before = reached.len();
        for &(a, b) in edges {
            if reached.contains(&a) || reached.contains(&b) {
                reached.extend([a, b]);
            }
        }
        if before == reached.len() {
            break;
        }
    }
    edges
        .iter()
        .filter(|(a, _)| reached.contains(a))
        .copied()
        .collect()
}

/// Recognize neutral conjugated five/six-membered cycles, including fused cycles
/// whose Kekule double lies in the adjacent ring. Do not guess from shape alone.
fn regions(doc: &Document) -> Option<BTreeSet<Edge>> {
    if doc.atoms.len() > 2048 {
        return None;
    }
    let mut adjacency: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for b in &doc.bonds {
        if matches!(b.order, 1 | 2 | 4)
            && b.display == "plain"
            && b.stereo.is_none()
            && [b.a, b.b]
                .iter()
                .all(|id| doc.atom(*id).is_some_and(eligible))
        {
            adjacency.entry(b.a).or_default().push(b.b);
            adjacency.entry(b.b).or_default().push(b.a);
        }
    }
    let mut cycles = BTreeSet::new();
    let mut budget = 32768usize;
    for &start in adjacency.keys() {
        let mut stack = vec![vec![start]];
        while let Some(path) = stack.pop() {
            budget = budget.checked_sub(1)?;
            let last = *path.last()?;
            for &next in adjacency.get(&last)? {
                if next == start && (5..=6).contains(&path.len()) {
                    let donors = path
                        .iter()
                        .filter(|id| doc.atom(**id).is_some_and(|a| donor(doc, a)))
                        .count();
                    if donors != 6 - path.len() {
                        continue;
                    }
                    let cycle: BTreeSet<_> = path
                        .iter()
                        .zip(path.iter().cycle().skip(1))
                        .take(path.len())
                        .map(|(&a, &b)| edge(a, b))
                        .collect();
                    let orders: Vec<_> = doc
                        .bonds
                        .iter()
                        .filter(|b| cycle.contains(&edge(b.a, b.b)))
                        .map(|b| b.order)
                        .collect();
                    if orders.iter().all(|o| *o == 4) || orders.iter().all(|o| matches!(o, 1 | 2)) {
                        cycles.insert(cycle);
                    }
                } else if path.len() < 6 && next > start && !path.contains(&next) {
                    let mut extended = path.clone();
                    extended.push(next);
                    stack.push(extended);
                }
            }
        }
    }
    loop {
        let edges: BTreeSet<_> = cycles.iter().flat_map(|c| c.iter().copied()).collect();
        let invalid: BTreeSet<_> = nodes(&edges)
            .into_iter()
            .filter(|id| {
                let Some(a) = doc.atom(*id) else {
                    return true;
                };
                let inside: Vec<_> = doc
                    .bonds
                    .iter()
                    .filter(|b| (b.a == *id || b.b == *id) && edges.contains(&edge(b.a, b.b)))
                    .collect();
                let circular = inside.iter().all(|b| b.order == 4);
                let pi = inside.iter().filter(|b| b.order == 2).count();
                let outside_pi = doc.bonds.iter().any(|b| {
                    (b.a == *id || b.b == *id)
                        && !edges.contains(&edge(b.a, b.b))
                        && !matches!(b.order, 0 | 1)
                });
                // An aromatic junction has three sigma bonds and one pi bond,
                // not three times 1.5. The latter would incorrectly reject every
                // degree-three junction in a circle-displayed fused system.
                let load = if circular {
                    valence(doc, *id) - inside.len() as u32 + 2 * u32::from(!donor(doc, a))
                } else {
                    valence(doc, *id)
                };
                outside_pi
                    || load.saturating_add(a.explicit_h.saturating_mul(2)) > capacity(a)
                    || if circular {
                        inside.len() < 2
                    } else {
                        inside.iter().any(|b| b.order == 4) || pi != usize::from(!donor(doc, a))
                    }
            })
            .collect();
        if invalid.is_empty() {
            return Some(edges);
        }
        cycles.retain(|cycle| {
            !cycle
                .iter()
                .any(|(a, b)| invalid.contains(a) || invalid.contains(b))
        });
    }
}

pub(super) struct Fusion {
    source: BTreeSet<Edge>,
    target: Option<BTreeSet<Edge>>,
}
impl Fusion {
    pub(super) fn center(&self, part: &Document, seed: u64) -> Point {
        let ids = nodes(&connected(&self.source, &BTreeSet::from([seed])));
        let count = ids.len().max(1) as f32;
        ids.iter()
            .filter_map(|id| part.atom(*id))
            .fold(Point::default(), |p, a| {
                p.offset(a.position.x / count, a.position.y / count)
            })
    }
    pub(super) fn new(doc: &Document, part: &Document) -> Self {
        let source = regions(part).unwrap_or_default();
        let target = if source.is_empty() {
            Some(BTreeSet::new())
        } else {
            regions(doc)
        };
        Self { source, target }
    }
    pub(super) fn source_edge(&self, a: u64, b: u64) -> bool {
        self.source.contains(&edge(a, b))
    }
    pub(super) fn target_edge(&self, a: u64, b: u64) -> bool {
        self.target
            .as_ref()
            .is_some_and(|e| e.contains(&edge(a, b)))
    }

    pub(super) fn map_vertices(
        &self,
        doc: &Document,
        part: &Document,
        source: &[u64],
        target: &[u64],
        shared: &mut HashMap<u64, u64>,
    ) -> bool {
        if self.target.is_none() {
            return false;
        }
        let ([seed, _], [ta, tb]) = (source, target) else {
            return false;
        };
        let Some(a) = doc.atom(*ta) else {
            return false;
        };
        let Some(b) = doc.atom(*tb) else {
            return false;
        };
        // The author's fusion tolerance, relative to the actual shared edge.
        let tolerance = a.position.distance(b.position) * (5. / 75.);
        let component = connected(&self.source, &BTreeSet::from([*seed]));
        for id in nodes(&component) {
            let Some(atom) = part.atom(id) else {
                return false;
            };
            let nearby: Vec<_> = doc
                .atoms
                .iter()
                .filter(|old| {
                    doc.atom_visible(old.id) && old.position.distance(atom.position) <= tolerance
                })
                .collect();
            let existing = if let Some(id) = shared.get(&id) {
                doc.atom(*id)
            } else if nearby.len() == 1 {
                nearby.first().copied()
            } else if nearby.is_empty() {
                None
            } else {
                return false;
            };
            if let Some(old) = existing {
                if !compatible(atom, old)
                    || doc.abbreviation(old.id).is_some()
                    || part.abbreviation(atom.id).is_some()
                    || shared.iter().any(|(from, to)| *from != id && *to == old.id)
                {
                    return false;
                }
                shared.insert(id, old.id);
            }
        }
        // An overlapping copy of the entire aromatic ring is not a fusion.
        // With no drag, let placement choose the outward candidate instead.
        nodes(&component).iter().any(|id| !shared.contains_key(id))
    }

    pub(super) fn assign(
        &self,
        doc: &Document,
        part: &Document,
        result: &mut Document,
        mapping: &HashMap<u64, u64>,
        source: &[u64],
    ) -> bool {
        let Some(target) = &self.target else {
            return false;
        };
        let Some(&seed) = source.first() else {
            return false;
        };
        let source_edges = connected(&self.source, &BTreeSet::from([seed]));
        let mapped: BTreeSet<_> = source_edges
            .iter()
            .filter_map(|(a, b)| Some(edge(*mapping.get(a)?, *mapping.get(b)?)))
            .collect();
        if mapped.len() != source_edges.len() || mapped.iter().any(|(a, b)| a == b) {
            return false;
        }
        let target_edges = connected(target, &nodes(&mapped));
        let edges: BTreeSet<_> = mapped.union(&target_edges).copied().collect();
        let fixed: BTreeSet<_> = doc
            .bonds
            .iter()
            .filter(|b| {
                b.order == 2
                    && edges.contains(&edge(b.a, b.b))
                    && !target_edges.contains(&edge(b.a, b.b))
            })
            .map(|b| edge(b.a, b.b))
            .collect();
        let affected = nodes(&edges);
        if affected.len() > 64 || edges.len() > 96 {
            return false;
        }
        let mut preferences = BTreeMap::new();
        for b in &doc.bonds {
            if edges.contains(&edge(b.a, b.b)) {
                // Existing stereobonds or query edges must never be rewritten.
                if b.display != "plain" || b.stereo.is_some() || !matches!(b.order, 1 | 2 | 4) {
                    return false;
                }
                preferences.insert(
                    edge(b.a, b.b),
                    (
                        if b.order == 2 {
                            1
                        } else if b.order == 1 && target_edges.contains(&edge(b.a, b.b)) {
                            -1
                        } else {
                            0
                        },
                        0,
                    ),
                );
            }
        }
        for b in &part.bonds {
            if source_edges.contains(&edge(b.a, b.b)) {
                let (Some(a), Some(z)) = (mapping.get(&b.a), mapping.get(&b.b)) else {
                    return false;
                };
                preferences.entry(edge(*a, *z)).or_insert((0, 0)).1 = if b.order == 2 {
                    1
                } else if b.order == 1 {
                    -1
                } else {
                    0
                };
            }
        }
        // Reserve every sigma bond before considering any additional pi bond.
        for b in &mut result.bonds {
            if edges.contains(&edge(b.a, b.b)) && !fixed.contains(&edge(b.a, b.b)) {
                b.order = 1;
            }
        }
        if edges
            .iter()
            .any(|key| !result.bonds.iter().any(|b| edge(b.a, b.b) == *key))
        {
            return false;
        }
        let mut available = BTreeSet::new();
        let mut required = BTreeSet::new();
        let old_aromatic = nodes(&target_edges);
        // Donor identity belongs to the input chemistry, not the temporary
        // all-single skeleton (which would turn a fused pyridine N into a donor).
        let mut donors: BTreeMap<u64, bool> = old_aromatic
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (*id, donor(doc, a))))
            .collect();
        for id in nodes(&source_edges) {
            let (Some(atom), Some(mapped)) = (part.atom(id), mapping.get(&id)) else {
                return false;
            };
            let source_donor = donor(part, atom);
            if donors.get(mapped).is_some_and(|old| *old != source_donor) {
                return false;
            }
            donors.insert(*mapped, source_donor);
        }
        for id in &affected {
            let Some(a) = result.atom(*id) else {
                return false;
            };
            if !eligible(a) {
                return false;
            }
            let load = valence(result, *id).saturating_add(a.explicit_h.saturating_mul(2));
            if capacity(a) == 0 || load > capacity(a) {
                return false;
            }
            if result
                .bonds
                .iter()
                .any(|b| (b.a == *id || b.b == *id) && b.order == 4)
            {
                return false;
            }
            let wants_pi = !donors.get(id).copied().unwrap_or(false);
            let fixed_pi = fixed.iter().any(|(x, y)| x == id || y == id);
            if wants_pi && !fixed_pi && load.saturating_add(2) <= capacity(a) {
                available.insert(*id);
            }
            // Existing aromatic atoms must stay conjugated. New carbon may be
            // methylene at a constrained multi-edge closure; never invent a
            // radical or an overvalent atom to force every ring to be benzene.
            if wants_pi && !fixed_pi && (old_aromatic.contains(id) || a.element == "N") {
                required.insert(*id);
            }
        }
        if !required.is_subset(&available) {
            return false;
        }
        let mut budget = 16384;
        let Some(solution) = matching(&available, &required, &edges, &preferences, &mut budget)
        else {
            return false;
        };
        if budget == 0 {
            return false;
        }
        for b in &mut result.bonds {
            if solution.pairs.contains(&edge(b.a, b.b)) {
                b.order = 2;
            }
        }
        if affected.iter().any(|id| {
            result.atom(*id).is_none_or(|a| {
                valence(result, *id).saturating_add(a.explicit_h.saturating_mul(2)) > capacity(a)
            })
        }) {
            return false;
        }
        result.invalidate_chemistry(&affected.iter().copied().collect::<Vec<_>>());
        for a in &mut result.atoms {
            if affected.contains(&a.id) {
                a.aromatic = false;
            }
        }
        true
    }
}

#[derive(Default)]
struct Assignment {
    pairs: BTreeSet<Edge>,
    retained: i32,
    preferred: i32,
}
impl Assignment {
    fn score(&self) -> (usize, i32, i32) {
        (self.pairs.len(), self.retained, self.preferred)
    }
}
/// Maximum matching with required vertices. Lexicographic ranking maximizes
/// conjugation, then retains existing Kekule bonds, then the source/Shift phase.
fn matching(
    available: &BTreeSet<u64>,
    required: &BTreeSet<u64>,
    edges: &BTreeSet<Edge>,
    preferences: &BTreeMap<Edge, (i32, i32)>,
    budget: &mut usize,
) -> Option<Assignment> {
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    let Some(a) = available
        .iter()
        .min_by_key(|a| {
            (
                !required.contains(a),
                edges
                    .iter()
                    .filter(|(x, y)| {
                        (*x == **a && available.contains(y)) || (*y == **a && available.contains(x))
                    })
                    .count(),
            )
        })
        .copied()
    else {
        return required.is_empty().then(Assignment::default);
    };
    let mut best: Option<Assignment> = None;
    for &(x, y) in edges {
        let b = if x == a {
            y
        } else if y == a {
            x
        } else {
            continue;
        };
        if !available.contains(&b) {
            continue;
        }
        let mut rest = available.clone();
        rest.remove(&a);
        rest.remove(&b);
        let mut needed = required.clone();
        needed.remove(&a);
        needed.remove(&b);
        if let Some(mut solution) = matching(&rest, &needed, edges, preferences, budget) {
            solution.pairs.insert(edge(a, b));
            let (retained, preferred) = preferences.get(&edge(a, b)).copied().unwrap_or_default();
            solution.retained += retained;
            solution.preferred += preferred;
            if best
                .as_ref()
                .is_none_or(|old| solution.score() > old.score())
            {
                best = Some(solution);
            }
        }
    }
    if !required.contains(&a) {
        let mut rest = available.clone();
        rest.remove(&a);
        if let Some(solution) = matching(&rest, required, edges, preferences, budget)
            && best
                .as_ref()
                .is_none_or(|old| solution.score() > old.score())
        {
            best = Some(solution);
        }
    }
    best
}
