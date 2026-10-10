//! Bounded maximum-cardinality graph matching. Chemistry-changing properties
//! are preferences, never candidate filters. IDs only make traversal repeatable.
use super::{MAX_MAP, check_edit, compatible, ids, reaction, unique};
use crate::{document::Document, reactions::Role};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
};

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub states: u64,
    /// Charged candidate/augment/scoring/search work. Chemical preparation is
    /// bounded separately by input caps; this is not a wall-clock deadline.
    pub work: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            states: 250_000,
            work: 20_000_000,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    pub heavy_adjacency: u32,
    pub adjacency: u32,
    pub unchanged_bonds: u32,
    pub changed_atoms: u32,
    pub property_delta: u64,
}
impl Ord for Score {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.heavy_adjacency,
            self.adjacency,
            self.unchanged_bonds,
            Reverse(self.changed_atoms),
            Reverse(self.property_delta),
        )
            .cmp(&(
                other.heavy_adjacency,
                other.adjacency,
                other.unchanged_bonds,
                Reverse(other.changed_atoms),
                Reverse(other.property_delta),
            ))
    }
}
impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
#[derive(Debug, Clone)]
pub struct Proposal {
    source: Document,
    assignments: Vec<(u64, u32)>,
    pub pairs: Vec<(u64, u64)>,
    pub unmatched_reactants: usize,
    pub unmatched_products: usize,
    pub complete: bool,
    /// True means at least two distinct equal-best mappings were found.
    pub multiple_best: bool,
    pub score: Score,
    pub states: u64,
}
fn snapshot(doc: &Document) -> Document {
    let mut source = doc.clone();
    crate::atom_labels::clear_computed(&mut source);
    for atom in &mut source.atoms {
        atom.label_h = 0;
    }
    source
}
impl Proposal {
    pub fn number(&self, atom: u64) -> Option<u32> {
        self.assignments
            .iter()
            .find(|(id, _)| *id == atom)
            .map(|(_, n)| *n)
    }
    pub fn current(&self, doc: &Document) -> bool {
        self.source == snapshot(doc)
    }
    pub fn apply(&self, doc: &Document, reviewed_uncertainty: bool) -> Result<Document, String> {
        if !self.current(doc) {
            return Err("The drawing changed; run Auto-map again".into());
        }
        if (!self.complete || self.multiple_best) && !reviewed_uncertainty {
            return Err("Review the ambiguous or budget-limited mapping before applying it".into());
        }
        let mut candidate = doc.clone();
        for (id, number) in &self.assignments {
            candidate
                .atom_mut(*id)
                .ok_or("A mapped atom is no longer available")?
                .map_num = *number;
        }
        for (id, _) in &self.assignments {
            check_edit(&candidate, *id, doc)?;
        }
        candidate.validate()?;
        Ok(candidate)
    }
}
#[derive(Debug)]
struct Atom {
    id: u64,
    element: String,
    isotope: u32,
    map: u32,
    charge: i32,
    h: u32,
    radical: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BondClass {
    order: u8,
    donor: Option<usize>,
}
#[derive(Debug)]
struct Graph {
    atoms: Vec<Atom>,
    edges: BTreeMap<(usize, usize), BondClass>,
}
fn edge(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}
impl Graph {
    fn build(doc: &Document, ids: &[u64]) -> Result<Self, String> {
        let part = crate::editing::selection(doc, ids);
        if part.bonds.len() > 768 {
            return Err(
                "Auto-map exceeds the 768-bond input limit; map this reaction manually".into(),
            );
        }
        let molecule = crate::chemistry::document::prepare(&part)
            .map_err(|e| format!("Cannot map this reaction: {e}"))?;
        let mut atoms = Vec::new();
        for (i, id) in molecule.ids.iter().enumerate() {
            let a = doc.atom(*id).ok_or("Missing reaction atom")?;
            let chemical = molecule
                .state
                .graph
                .atoms
                .get(i)
                .ok_or("Missing chemical atom")?;
            let valence = molecule
                .state
                .valences
                .get(i)
                .ok_or("Missing chemical valence")?;
            atoms.push(Atom {
                id: *id,
                element: a.element.clone(),
                isotope: a.isotope,
                map: a.map_num,
                charge: i32::from(chemical.charge),
                h: u32::from(chemical.explicit_hydrogens) + valence.implicit_hydrogens,
                radical: chemical.radical_electrons,
            });
        }
        let edges = molecule
            .state
            .graph
            .bonds
            .iter()
            .filter(|b| b.order != 0)
            .map(|b| {
                (
                    edge(b.a, b.b),
                    BondClass {
                        order: if b.aromatic { 4 } else { b.order },
                        donor: (b.order == 5).then_some(b.a),
                    },
                )
            })
            .collect();
        Ok(Self { atoms, edges })
    }
    fn heavy(&self, a: usize, b: usize) -> bool {
        self.atoms
            .get(a)
            .zip(self.atoms.get(b))
            .is_some_and(|(a, b)| a.element != "H" && b.element != "H")
    }
    fn degree(&self, a: usize) -> usize {
        self.edges
            .keys()
            .filter(|(i, j)| *i == a || *j == a)
            .count()
    }
}
fn can_pair(a: &Atom, b: &Atom) -> bool {
    a.element == b.element && a.isotope == b.isotope && (a.map == 0 || b.map == 0 || a.map == b.map)
}
fn budget_error() -> String {
    "Auto-map search budget reached before a full correspondence was found; add manual anchors"
        .into()
}
fn charge(work: &mut u64, budget: Budget, n: u64) -> Result<(), String> {
    *work = work.saturating_add(n);
    if *work > budget.work {
        Err(budget_error())
    } else {
        Ok(())
    }
}
fn score_work(source: &Graph, target: &Graph, count: usize) -> u64 {
    (source.edges.len() + target.edges.len()) as u64 * (count as u64 + 1) + count as u64 * 4
}
fn same_bond(
    a: &BondClass,
    b: &BondClass,
    map: &HashMap<usize, usize>,
    insert: Option<(usize, usize)>,
) -> bool {
    let donor = a.donor.and_then(|d| {
        insert
            .filter(|(s, _)| *s == d)
            .map(|(_, t)| t)
            .or_else(|| map.get(&d).copied())
    });
    a.order == b.order && donor == b.donor
}
fn augment(
    s: usize,
    domains: &[Vec<usize>],
    unavailable: &HashSet<usize>,
    seen: &mut HashSet<usize>,
    matched: &mut HashMap<usize, usize>,
    work: &mut u64,
    budget: Budget,
) -> Result<bool, String> {
    for t in domains.get(s).into_iter().flatten().copied() {
        charge(work, budget, 1)?;
        if unavailable.contains(&t) || !seen.insert(t) {
            continue;
        }
        let previous = matched.get(&t).copied();
        let possible = if let Some(p) = previous {
            augment(p, domains, unavailable, seen, matched, work, budget)?
        } else {
            true
        };
        if possible {
            matched.insert(t, s);
            return Ok(true);
        }
    }
    Ok(false)
}
fn score(source: &Graph, target: &Graph, map: &HashMap<usize, usize>) -> Score {
    let mut result = Score::default();
    for (&(a, b), &order) in &source.edges {
        if let Some((x, y)) = map.get(&a).zip(map.get(&b))
            && let Some(other) = target.edges.get(&edge(*x, *y))
        {
            result.adjacency += 1;
            result.heavy_adjacency += u32::from(source.heavy(a, b));
            result.unchanged_bonds += u32::from(same_bond(&order, other, map, None));
        }
    }
    let inverse: HashMap<_, _> = map.iter().map(|(a, b)| (*b, *a)).collect();
    for (&a, &b) in map {
        let Some((left, right)) = source.atoms.get(a).zip(target.atoms.get(b)) else {
            continue;
        };
        let delta = u64::from(left.charge.abs_diff(right.charge))
            + u64::from(left.h.abs_diff(right.h))
            + u64::from(left.radical.abs_diff(right.radical));
        result.property_delta += delta;
        let source_incident = source.edges.iter().filter(|((i, j), _)| *i == a || *j == a);
        let target_incident = target.edges.iter().filter(|((i, j), _)| *i == b || *j == b);
        let left_changed = source_incident.into_iter().any(|(&(i, j), order)| {
            let other = if i == a { j } else { i };
            map.get(&other).is_none_or(|mapped| {
                target
                    .edges
                    .get(&edge(b, *mapped))
                    .is_none_or(|other| !same_bond(order, other, map, None))
            })
        });
        let right_changed = target_incident.into_iter().any(|(&(i, j), order)| {
            let other = if i == b { j } else { i };
            inverse.get(&other).is_none_or(|mapped| {
                source
                    .edges
                    .get(&edge(a, *mapped))
                    .is_none_or(|source_bond| !same_bond(source_bond, order, map, None))
            })
        });
        result.changed_atoms += u32::from(delta != 0 || left_changed || right_changed);
    }
    result
}
struct Search<'a> {
    source: &'a Graph,
    target: &'a Graph,
    domains: Vec<Vec<usize>>,
    k: usize,
    map: HashMap<usize, usize>,
    used: HashSet<usize>,
    decided: HashSet<usize>,
    best: HashMap<usize, usize>,
    best_score: Score,
    multiple: bool,
    states: u64,
    work: u64,
    budget: Budget,
    stopped: bool,
}
impl Search<'_> {
    fn spend(&mut self, n: u64) -> bool {
        self.work = self.work.saturating_add(n);
        if self.work > self.budget.work {
            self.stopped = true;
            false
        } else {
            true
        }
    }
    fn signature(map: &HashMap<usize, usize>) -> Vec<(usize, usize)> {
        let mut pairs: Vec<_> = map.iter().map(|(a, b)| (*a, *b)).collect();
        pairs.sort_unstable();
        pairs
    }
    fn visit(&mut self) {
        if self.stopped {
            return;
        }
        self.states += 1;
        if self.states > self.budget.states {
            self.stopped = true;
            return;
        }
        if self.map.len() == self.k {
            if !self.spend(score_work(self.source, self.target, self.map.len())) {
                return;
            }
            let value = score(self.source, self.target, &self.map);
            if value > self.best_score {
                self.best_score = value;
                self.best = self.map.clone();
                self.multiple = false;
            } else if value == self.best_score && self.map != self.best {
                self.multiple = true;
                if Self::signature(&self.map) < Self::signature(&self.best) {
                    self.best = self.map.clone();
                }
            }
            return;
        }
        if !self.spend(
            (self.source.atoms.len()
                + self.target.atoms.len()
                + self.source.edges.len()
                + 2 * self.target.edges.len()
                + 1) as u64,
        ) {
            return;
        }
        // Optimistic cardinality bound. Ignoring map compatibility cannot
        // remove a feasible mapping; exact K was computed before search.
        let mut sources = BTreeMap::<(&str, u32), usize>::new();
        let mut targets = BTreeMap::<(&str, u32), usize>::new();
        for (i, a) in self
            .source
            .atoms
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.decided.contains(i))
        {
            *sources.entry((&a.element, a.isotope)).or_default() += 1;
            let _ = i;
        }
        for (_, a) in self
            .target
            .atoms
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.used.contains(i))
        {
            *targets.entry((&a.element, a.isotope)).or_default() += 1;
        }
        let capacity: usize = sources
            .iter()
            .map(|(key, n)| (*n).min(targets.get(key).copied().unwrap_or(0)))
            .sum();
        if self.map.len() + capacity < self.k {
            return;
        }
        // Each unresolved source edge can contribute at most one target edge.
        // Costs optimistically remain zero, so pruning is safe even for atoms
        // that change charge, hydrogens, degree, ring membership or bond order.
        let mut upper = Score::default();
        let (mut open, mut heavy_open) = (0_u32, 0_u32);
        for (&(a, b), &order) in &self.source.edges {
            if let Some((x, y)) = self.map.get(&a).zip(self.map.get(&b)) {
                if let Some(other) = self.target.edges.get(&edge(*x, *y)) {
                    upper.adjacency += 1;
                    upper.heavy_adjacency += u32::from(self.source.heavy(a, b));
                    upper.unchanged_bonds += u32::from(same_bond(&order, other, &self.map, None));
                }
            } else if (!self.decided.contains(&a) || self.map.contains_key(&a))
                && (!self.decided.contains(&b) || self.map.contains_key(&b))
            {
                open += 1;
                heavy_open += u32::from(self.source.heavy(a, b));
            }
        }
        let available: Vec<_> = self
            .target
            .edges
            .keys()
            .filter(|(a, b)| !self.used.contains(a) || !self.used.contains(b))
            .collect();
        let heavy_available = available
            .iter()
            .filter(|&&(a, b)| self.target.heavy(*a, *b))
            .count() as u32;
        upper.heavy_adjacency += heavy_open.min(heavy_available);
        upper.adjacency += open.min(available.len() as u32);
        upper.unchanged_bonds += open.min(available.len() as u32);
        if upper < self.best_score || (upper == self.best_score && self.multiple) {
            return;
        }
        let mut chosen = None;
        for s in 0..self.source.atoms.len() {
            if self.decided.contains(&s) {
                continue;
            }
            if !self.spend(
                (self.domains.get(s).map_or(0, Vec::len) + 2 * self.source.edges.len() + 1) as u64,
            ) {
                return;
            }
            let choices = self
                .domains
                .get(s)
                .into_iter()
                .flatten()
                .filter(|t| !self.used.contains(t))
                .count();
            let adjacent = self
                .source
                .edges
                .keys()
                .filter(|(a, b)| {
                    (*a == s && self.map.contains_key(b)) || (*b == s && self.map.contains_key(a))
                })
                .count();
            let id = self.source.atoms.get(s).map_or(0, |a| a.id);
            let key = (
                Reverse(adjacent),
                choices,
                Reverse(self.source.degree(s)),
                id,
            );
            if chosen.as_ref().is_none_or(|(_, old)| key < *old) {
                chosen = Some((s, key));
            }
        }
        let Some((s, _)) = chosen else {
            return;
        };
        if !self.spend(self.domains.get(s).map_or(0, Vec::len) as u64) {
            return;
        }
        let mut candidates: Vec<_> = self
            .domains
            .get(s)
            .into_iter()
            .flatten()
            .copied()
            .filter(|t| !self.used.contains(t))
            .collect();
        let sorting = candidates.len()
            * (self.source.edges.len() + 4 + usize::BITS as usize
                - candidates.len().max(1).leading_zeros() as usize);
        if !self.spend(sorting as u64) {
            return;
        }
        candidates.sort_by_cached_key(|t| {
            let (mut adjacency, mut bonds) = (0, 0);
            for (&(a, b), &order) in &self.source.edges {
                let neighbor = if a == s {
                    b
                } else if b == s {
                    a
                } else {
                    continue;
                };
                if let Some(other) = self
                    .map
                    .get(&neighbor)
                    .and_then(|mapped| self.target.edges.get(&edge(*t, *mapped)))
                {
                    adjacency += 1;
                    bonds += u32::from(same_bond(&order, other, &self.map, Some((s, *t))));
                }
            }
            let delta = self
                .source
                .atoms
                .get(s)
                .zip(self.target.atoms.get(*t))
                .map_or(u64::MAX, |(a, b)| {
                    u64::from(a.charge.abs_diff(b.charge))
                        + u64::from(a.h.abs_diff(b.h))
                        + u64::from(a.radical.abs_diff(b.radical))
                });
            (
                Reverse(adjacency),
                Reverse(bonds),
                delta,
                self.target.atoms.get(*t).map_or(0, |a| a.id),
            )
        });
        self.decided.insert(s);
        for t in candidates {
            self.map.insert(s, t);
            self.used.insert(t);
            self.visit();
            self.map.remove(&s);
            self.used.remove(&t);
            if self.stopped {
                break;
            }
        }
        // Leaving an atom unmatched is legal, including omitted coproducts.
        if !self.stopped {
            self.visit();
        }
        self.decided.remove(&s);
    }
}

pub fn propose(doc: &Document, arrow: u64, budget: Budget) -> Result<Proposal, String> {
    if budget.work == 0 {
        return Err(budget_error());
    }
    let mut work = 0;
    charge(
        &mut work,
        budget,
        (doc.atoms.len() + doc.bonds.len() + doc.reactions.len() + 1) as u64,
    )?;
    doc.validate()?;
    let r = reaction(doc, arrow)?;
    if !r.ready() {
        return Err("Assign reactants and products first".into());
    }
    if r.reactants
        .iter()
        .chain(&r.products)
        .any(|p| p.coefficient != 1)
    {
        return Err(
            "Auto-map needs one drawn molecule per participant; expand coefficients first".into(),
        );
    }
    let left_ids = ids(r, Role::Reactant);
    let right_ids = ids(r, Role::Product);
    if left_ids.len() > 128 || right_ids.len() > 128 {
        return Err("Auto-map supports at most 128 explicit atoms per reaction side; map this reaction manually".into());
    }
    let left_maps = unique(doc, &left_ids)?;
    let right_maps = unique(doc, &right_ids)?;
    for (number, a) in &left_maps {
        if let Some(b) = right_maps.get(number)
            && !compatible(doc, *a, *b)
        {
            return Err(format!(
                "Atom map {number} has different elements or isotopes across the reaction"
            ));
        }
    }
    let participant_ids: HashSet<_> = left_ids.iter().chain(&right_ids).copied().collect();
    let bond_count = doc
        .bonds
        .iter()
        .filter(|b| participant_ids.contains(&b.a) && participant_ids.contains(&b.b))
        .count();
    if bond_count > 768 {
        return Err("Auto-map exceeds the 768-bond input limit; map this reaction manually".into());
    }
    charge(
        &mut work,
        budget,
        (2 * (doc.atoms.len() + doc.bonds.len()) + left_ids.len() + right_ids.len()) as u64,
    )?;
    let left = Graph::build(doc, &left_ids)?;
    let right = Graph::build(doc, &right_ids)?;
    if left.edges.len() + right.edges.len() > 768 {
        return Err(
            "Auto-map exceeds the 768-bond search limit; map this reaction manually".into(),
        );
    }
    let swapped = left.atoms.len() > right.atoms.len();
    let (source, target) = if swapped {
        (&right, &left)
    } else {
        (&left, &right)
    };
    charge(
        &mut work,
        budget,
        (2 * source.atoms.len() * target.atoms.len()) as u64,
    )?;
    let domains: Vec<Vec<_>> = source
        .atoms
        .iter()
        .map(|a| {
            target
                .atoms
                .iter()
                .enumerate()
                .filter(|(_, b)| can_pair(a, b))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    if domains.iter().map(Vec::len).sum::<usize>() > 16_384 {
        return Err(
            "Auto-map exceeds the candidate limit; add manual atom-map anchors first".into(),
        );
    }
    let mut seeds = HashMap::new();
    for (s, a) in source.atoms.iter().enumerate().filter(|(_, a)| a.map != 0) {
        if let Some(t) = target.atoms.iter().position(|b| b.map == a.map) {
            seeds.insert(s, t);
        }
    }
    let used: HashSet<_> = seeds.values().copied().collect();
    let mut maximum = HashMap::new();
    for s in 0..source.atoms.len() {
        if !seeds.contains_key(&s) {
            charge(&mut work, budget, 1)?;
            augment(
                s,
                &domains,
                &used,
                &mut HashSet::new(),
                &mut maximum,
                &mut work,
                budget,
            )?;
        }
    }
    let mut incumbent = seeds.clone();
    incumbent.extend(maximum.into_iter().map(|(t, s)| (s, t)));
    let k = incumbent.len();
    charge(&mut work, budget, score_work(source, target, k))?;
    let mut search = Search {
        source,
        target,
        domains,
        k,
        map: seeds.clone(),
        used,
        decided: seeds.keys().copied().collect(),
        best: incumbent.clone(),
        best_score: score(source, target, &incumbent),
        multiple: false,
        states: 0,
        work,
        budget,
        stopped: false,
    };
    search.visit();
    let mut pairs = Vec::new();
    let mut assignments = Vec::new();
    let mut numbers: HashSet<_> = doc.atoms.iter().map(|a| a.map_num).collect();
    let mut next = 1;
    for (s, t) in Search::signature(&search.best) {
        let a = source.atoms.get(s).ok_or("Missing mapping source")?;
        let b = target.atoms.get(t).ok_or("Missing mapping target")?;
        let number = if a.map != 0 {
            a.map
        } else if b.map != 0 {
            b.map
        } else {
            while numbers.contains(&next) {
                next = next
                    .checked_add(1)
                    .filter(|n| *n <= MAX_MAP)
                    .ok_or("No atom map numbers are available")?;
            }
            numbers.insert(next);
            next
        };
        assignments.push((a.id, number));
        assignments.push((b.id, number));
        pairs.push(if swapped { (b.id, a.id) } else { (a.id, b.id) });
    }
    pairs.sort_unstable();
    let proposal = Proposal {
        source: snapshot(doc),
        assignments,
        pairs,
        unmatched_reactants: left.atoms.len() - k,
        unmatched_products: right.atoms.len() - k,
        complete: !search.stopped,
        multiple_best: search.multiple,
        score: search.best_score,
        states: search.states,
    };
    // Shared intermediates can belong to several reaction steps. Inherited
    // labels must remain unique and compatible in every affected scope.
    proposal.apply(doc, true)?;
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn atom(id: u64) -> Atom {
        Atom {
            id,
            element: "Fe".into(),
            isotope: 0,
            map: id as u32,
            charge: 0,
            h: 0,
            radical: 0,
        }
    }
    #[test]
    fn dative_direction_is_compared_after_endpoint_mapping() {
        let source = Graph {
            atoms: vec![atom(1), atom(2)],
            edges: BTreeMap::from([(
                (0, 1),
                BondClass {
                    order: 5,
                    donor: Some(0),
                },
            )]),
        };
        let target = Graph {
            atoms: vec![atom(3), atom(4)],
            edges: BTreeMap::from([(
                (0, 1),
                BondClass {
                    order: 5,
                    donor: Some(1),
                },
            )]),
        };
        let fixed = HashMap::from([(0, 0), (1, 1)]);
        let changed = score(&source, &target, &fixed);
        assert_eq!(changed.adjacency, 1);
        assert_eq!(changed.unchanged_bonds, 0);
        assert_eq!(changed.changed_atoms, 2);
        let swapped = HashMap::from([(0, 1), (1, 0)]);
        let same = score(&source, &target, &swapped);
        assert_eq!(same.unchanged_bonds, 1);
        assert_eq!(same.changed_atoms, 0);
    }
    #[test]
    fn augment_budget_is_checked_before_scanning_domains() {
        let domains = vec![vec![0, 1], vec![0, 1]];
        let mut work = 0;
        assert!(
            augment(
                0,
                &domains,
                &HashSet::new(),
                &mut HashSet::new(),
                &mut HashMap::new(),
                &mut work,
                Budget {
                    states: 100,
                    work: 0
                }
            )
            .is_err()
        );
        assert_eq!(work, 1);
    }
}
