//! OPSIN 2.9.0 StereoAnalyser, commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! MIT, Daniel Lowe and contributors. This is the source's constitutional
//! environment refinement and limited centre identification, not a broader
//! stereochemistry model. Double-bond ghost atoms do not inherit isotopes.
use crate::graph::{AtomId, BondId, Element, FragmentId, Graph, GraphError};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StereoCentre {
    pub atom: AtomId,
    pub true_stereo_centre: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StereoBond {
    pub bond: BondId,
}
impl StereoCentre {
    pub fn get_cip_ordered_atoms(
        &self,
        graph: &Graph,
    ) -> Result<Vec<AtomId>, crate::cip::CipOrderingError> {
        let mut atoms = crate::cip::CipSequenceRules::new(graph, self.atom)
            .get_neighbouring_atoms_in_cip_order()?;
        if atoms.len() == 3 {
            atoms.insert(0, self.atom);
        }
        Ok(atoms)
    }
}
impl StereoBond {
    pub fn get_ordered_stereo_atoms(
        &self,
        graph: &Graph,
    ) -> Result<[AtomId; 4], crate::cip::CipOrderingError> {
        let bond = graph.bond(self.bond);
        let first = crate::cip::CipSequenceRules::new(graph, bond.from)
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(bond.to)?;
        let second = crate::cip::CipSequenceRules::new(graph, bond.to)
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(bond.from)?;
        let first = *first.last().ok_or_else(|| {
            crate::cip::CipOrderingError::InvalidGraph("Stereo bond has no first ligand".into())
        })?;
        let second = *second.last().ok_or_else(|| {
            crate::cip::CipOrderingError::InvalidGraph("Stereo bond has no second ligand".into())
        })?;
        Ok([first, bond.from, bond.to, second])
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StereoAnalysis {
    pub centres: Vec<StereoCentre>,
    pub bonds: Vec<StereoBond>,
    pub environment_numbers: HashMap<AtomId, usize>,
}

impl StereoAnalysis {
    pub fn atom_environment_number(&self, atom: AtomId) -> Option<usize> {
        self.environment_numbers.get(&atom).copied()
    }
}

#[derive(Clone)]
struct Node {
    element: Element,
    isotope: Option<u32>,
    neighbours: Vec<usize>,
}

pub fn analyse(graph: &Graph, fragment: FragmentId) -> Result<StereoAnalysis, GraphError> {
    analyse_atoms_and_bonds(
        graph,
        &graph.fragment(fragment).atoms,
        &graph.fragment(fragment).bonds,
    )
}

pub fn analyse_atoms_and_bonds(
    graph: &Graph,
    atoms: &[AtomId],
    bonds: &[BondId],
) -> Result<StereoAnalysis, GraphError> {
    let index: HashMap<_, _> = atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| (*atom, index))
        .collect();
    if index.len() != atoms.len() {
        return Err(GraphError(
            "Stereo analysis contains duplicate atoms".into(),
        ));
    }
    let mut nodes: Vec<_> = atoms
        .iter()
        .map(|atom| {
            let atom = graph.atom(*atom);
            Node {
                element: atom.element,
                isotope: atom.isotope,
                neighbours: Vec::new(),
            }
        })
        .collect();
    let bond_set: HashSet<_> = bonds.iter().copied().collect();
    for &atom in atoms {
        for &bond in &graph.atom(atom).bonds {
            if !bond_set.contains(&bond) {
                return Err(GraphError(
                    "Stereo analysis atom has a bond outside the provided set".into(),
                ));
            }
        }
    }
    for &bond in bonds {
        let b = graph.bond(bond);
        let from = *index.get(&b.from).ok_or_else(|| {
            GraphError("Stereo analysis bond endpoint is outside the atom set".into())
        })?;
        let to = *index.get(&b.to).ok_or_else(|| {
            GraphError("Stereo analysis bond endpoint is outside the atom set".into())
        })?;
        nodes[from].neighbours.push(to);
        nodes[to].neighbours.push(from);
        for _ in 1..b.order {
            let ghost = nodes.len();
            nodes.push(Node {
                element: nodes[from].element,
                isotope: None,
                neighbours: vec![to],
            });
            nodes[to].neighbours.push(ghost);
            let ghost = nodes.len();
            nodes.push(Node {
                element: nodes[to].element,
                isotope: None,
                neighbours: vec![from],
            });
            nodes[from].neighbours.push(ghost);
        }
    }
    let mut sorted: Vec<_> = (0..nodes.len()).collect();
    sorted.sort_by_key(|index| (nodes[*index].element.atomic_number(), nodes[*index].isotope));
    let mut colours = vec![0; nodes.len()];
    let mut groups = Vec::<Vec<usize>>::new();
    for atom in sorted {
        let same = groups.last().is_some_and(|group| {
            let previous = &nodes[group[0]];
            previous.element == nodes[atom].element && previous.isotope == nodes[atom].isotope
        });
        if same {
            groups.last_mut().unwrap().push(atom);
        } else {
            groups.push(vec![atom]);
        }
    }
    assign_group_colours(&groups, &mut colours);
    loop {
        let adjacent: Vec<Vec<_>> = nodes
            .iter()
            .map(|node| {
                let mut colours: Vec<_> =
                    node.neighbours.iter().map(|atom| colours[*atom]).collect();
                colours.sort();
                colours.reverse();
                colours
            })
            .collect();
        let mut updated = Vec::<Vec<usize>>::new();
        for mut group in groups {
            group.sort_by(|a, b| adjacent[*a].cmp(&adjacent[*b]));
            let mut split = Vec::new();
            for atom in group {
                if split
                    .last()
                    .is_some_and(|previous| adjacent[*previous] != adjacent[atom])
                {
                    updated.push(std::mem::take(&mut split));
                }
                split.push(atom);
            }
            if !split.is_empty() {
                updated.push(split);
            }
        }
        let before = colours.clone();
        assign_group_colours(&updated, &mut colours);
        groups = updated;
        if before == colours {
            break;
        }
    }
    let environments: HashMap<_, _> = atoms
        .iter()
        .copied()
        .enumerate()
        .map(|(index, atom)| (atom, colours[index]))
        .collect();
    let potential: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|atom| is_possibly_stereogenic(graph, *atom))
        .collect();
    let true_centres: Vec<_> = potential
        .iter()
        .copied()
        .filter(|atom| {
            let neighbours = graph.neighbours(*atom);
            if !matches!(neighbours.len(), 3 | 4) {
                return false;
            }
            let mut seen = HashSet::new();
            seen.insert(0);
            neighbours
                .into_iter()
                .all(|neighbour| seen.insert(environments[&neighbour]))
        })
        .collect();
    let mut centres: Vec<_> = true_centres
        .iter()
        .map(|atom| StereoCentre {
            atom: *atom,
            true_stereo_centre: true,
        })
        .collect();
    for atom in potential
        .into_iter()
        .filter(|atom| !true_centres.contains(atom))
    {
        let neighbours = graph.neighbours(atom);
        if neighbours.len() != 4 {
            continue;
        }
        // Preserve upstream's first matching pair for each index. Three equal
        // ligands form three entries and therefore do not qualify.
        let mut pairs = Vec::new();
        for i in 0..4 {
            if let Some(j) =
                (i + 1..4).find(|j| environments[&neighbours[i]] == environments[&neighbours[*j]])
            {
                pairs.push((i, j));
            }
        }
        if matches!(pairs.len(), 1 | 2)
            && pairs.iter().all(|(i, j)| {
                branches_have_true_stereocentre(
                    graph,
                    neighbours[*i],
                    neighbours[*j],
                    atom,
                    &true_centres,
                )
            })
        {
            centres.push(StereoCentre {
                atom,
                true_stereo_centre: false,
            });
        }
    }
    let stereo_bonds = bonds
        .iter()
        .copied()
        .filter(|bond| {
            let b = graph.bond(*bond);
            if b.order != 2 {
                return false;
            }
            let qualifies = |atom, other| {
                let a = graph.atom(atom);
                let neighbours: Vec<_> = graph
                    .neighbours(atom)
                    .into_iter()
                    .filter(|neighbour| *neighbour != other)
                    .collect();
                match neighbours.len() {
                    2 => environments[&neighbours[0]] != environments[&neighbours[1]],
                    1 => {
                        a.element == Element::N
                            && graph.incoming_valency(atom) == 3
                            && a.charge == 0
                    }
                    _ => false,
                }
            };
            qualifies(b.from, b.to) && qualifies(b.to, b.from)
        })
        .map(|bond| StereoBond { bond })
        .collect();
    Ok(StereoAnalysis {
        centres,
        bonds: stereo_bonds,
        environment_numbers: environments,
    })
}

fn assign_group_colours(groups: &[Vec<usize>], colours: &mut [usize]) {
    let mut seen = 0;
    for group in groups {
        seen += group.len();
        for &atom in group {
            colours[atom] = seen;
        }
    }
}

fn branches_have_true_stereocentre(
    graph: &Graph,
    first: AtomId,
    second: AtomId,
    central: AtomId,
    true_centres: &[AtomId],
) -> bool {
    let mut visited = HashSet::from([central]);
    let mut queue = VecDeque::from([first, second]);
    while !queue.is_empty() {
        let mut next = VecDeque::new();
        while let Some(atom) = queue.pop_front() {
            if true_centres.contains(&atom) {
                return true;
            }
            if queue.contains(&atom) {
                queue.retain(|other| *other != atom);
                continue;
            }
            for neighbour in graph.neighbours(atom) {
                if !visited.contains(&neighbour) {
                    next.push_back(neighbour);
                }
            }
            visited.insert(atom);
        }
        queue = next;
    }
    false
}

pub fn is_possibly_stereogenic(graph: &Graph, atom: AtomId) -> bool {
    is_known_potentially_stereogenic(graph, atom)
        && !is_achiral_due_to_resonance_or_tautomerism(graph, atom)
}

pub fn is_known_potentially_stereogenic(graph: &Graph, atom: AtomId) -> bool {
    let a = graph.atom(atom);
    let neighbours = graph.neighbours(atom);
    if neighbours.len() == 4 {
        return matches!(
            a.element,
            Element::B
                | Element::C
                | Element::Si
                | Element::Ge
                | Element::Sn
                | Element::N
                | Element::P
                | Element::As
                | Element::S
                | Element::Se
        );
    }
    if neighbours.len() == 3 {
        let valency = graph.incoming_valency(atom);
        if matches!(a.element, Element::S | Element::Se)
            && (valency == 4 || (a.charge == 1 && valency == 3))
        {
            return true;
        }
        if a.element == Element::N
            && a.charge == 0
            && valency == 3
            && neighbours.iter().any(|atom| {
                graph
                    .neighbours(*atom)
                    .iter()
                    .any(|neighbour| neighbours.contains(neighbour))
            })
        {
            return true;
        }
        if matches!(a.element, Element::P | Element::As) && valency == 3 {
            return true;
        }
    }
    false
}

pub fn is_achiral_due_to_resonance_or_tautomerism(graph: &Graph, atom: AtomId) -> bool {
    let a = graph.atom(atom);
    if !matches!(
        a.element,
        Element::N | Element::P | Element::As | Element::S | Element::Se
    ) {
        return false;
    }
    let mut resonant_elements = HashSet::new();
    for neighbour in graph.neighbours(atom) {
        let n = graph.atom(neighbour);
        if (n.element.is_chalcogen() || n.element == Element::N)
            && graph
                .neighbours(neighbour)
                .into_iter()
                .all(|other| other == atom || graph.atom(other).element == Element::H)
        {
            // Exact source behavior: the isotope belongs to the central atom,
            // not the neighbouring atom. Do not silently repair this quirk.
            if !resonant_elements.insert((n.element, a.isotope)) {
                return true;
            }
        }
        if n.element == Element::H && n.bonds.len() == 1 {
            return true;
        }
    }
    false
}
