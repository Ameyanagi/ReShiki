use super::*;

impl Configuration {
    pub fn sp2(
        mol: &Molecule<'_>,
        bond: usize,
        foci: [usize; 2],
        config: u8,
    ) -> Result<Self, Error> {
        if !matches!(config, 4 | 5) {
            return Err(invalid("Expected cis/trans bond configuration"));
        }
        let b = at(&mol.state.graph.bonds, bond)?;
        let meta = at(&mol.state.metadata.bonds, bond)?;
        if b.order != 2 || meta.stereo <= 1 {
            return Err(invalid("Expected stereo double bond"));
        }
        let carriers = if !meta.stereo_atoms.is_empty() {
            meta.stereo_atoms.iter().copied().map(Some).collect()
        } else if matches!(meta.stereo, 2 | 3) {
            let a = highest(mol, b.a, b.b)?;
            let z = highest(mol, b.b, b.a)?;
            if a.is_some() && z.is_some() {
                vec![a, z]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if carriers.len() != 2 {
            return Err(invalid("Expected two double-bond carriers"));
        }
        Self::bond(mol, Kind::Sp2Bond, bond, foci, config, carriers)
    }
    pub fn atropisomer(
        mol: &Molecule<'_>,
        bond: usize,
        foci: [usize; 2],
        config: u8,
    ) -> Result<Self, Error> {
        if !matches!(config, 6 | 7) {
            return Err(invalid("Expected atropisomer winding"));
        }
        let b = at(&mol.state.graph.bonds, bond)?;
        let mut carriers = Vec::new();
        // Atropisomers.cpp getAtropisomerAtomsAndBonds orders exactly two
        // incident bonds by neighbor index; larger sets keep insertion order.
        for atom in [b.a, b.b] {
            let mut neighbors: Vec<_> = at(&mol.adjacent, atom)?
                .iter()
                .filter(|&&(_, e)| e != bond)
                .map(|&(a, _)| a)
                .collect();
            if neighbors.is_empty() {
                carriers.clear();
                break;
            }
            if neighbors.len() == 2 {
                neighbors.sort_unstable();
            }
            carriers.push(Some(*at(&neighbors, 0)?));
        }
        Self::bond(mol, Kind::AtropisomerBond, bond, foci, config, carriers)
    }
    fn bond(
        mol: &Molecule<'_>,
        kind: Kind,
        bond: usize,
        foci: [usize; 2],
        config: u8,
        carriers: Vec<Option<usize>>,
    ) -> Result<Self, Error> {
        for atom in foci {
            at(&mol.state.graph.atoms, atom)?;
        }
        Ok(Self {
            kind,
            target: Target::Bond(bond),
            foci: foci.to_vec(),
            carriers,
            config,
            ranked: Vec::new(),
        })
    }
    pub(super) fn label_bond<'a>(
        &mut self,
        mol: &mut Molecule<'a>,
        graph: &mut Digraph<'a>,
        root1: usize,
        rules: &Rules,
        iterations: &mut Iterations,
    ) -> Result<Descriptor, Error> {
        let foci = [*at(&self.foci, 0)?, *at(&self.foci, 1)?];
        let mut internal = None;
        for edge in graph.edges(mol, root1)?.to_vec() {
            let e = graph.edge(edge)?;
            if !graph.node(e.begin)?.is_duplicate()
                && !graph.node(e.end)?.is_duplicate()
                && is_internal(graph, edge, foci)?
            {
                internal = Some(edge);
                break;
            }
        }
        let Some(internal) = internal else {
            return Ok(Descriptor::Unknown);
        };
        let root2 = graph.edge(internal)?.other(root1)?;
        let edges1 = graph.edges(mol, root1)?.to_vec();
        let edges2 = graph.edges(mol, root2)?.to_vec();
        let edges1 = filtered(graph, &edges1, foci, self.kind)?;
        let edges2 = filtered(graph, &edges2, foci, self.kind)?;
        let mut carriers = [self.carrier(0)?, self.carrier(1)?];
        if graph.node(root1)?.atom == Some(foci[1]) {
            carriers.swap(0, 1);
        }
        let mut flip = false;
        graph.change_root(mol, root1)?;
        let first = sort(mol, graph, iterations, rules, root1, &edges1)?;
        if !first.unique && rules.num_subrules() != 3 {
            return Ok(Descriptor::Unknown);
        }
        flip ^= swaps(graph, &first.edges, carriers[0], self.kind)?;
        graph.change_root(mol, root2)?;
        let second = sort(mol, graph, iterations, rules, root2, &edges2)?;
        if !first.unique || !second.unique {
            return Ok(Descriptor::Unknown);
        }
        flip ^= swaps(graph, &second.edges, carriers[1], self.kind)?;
        let e1 = *at(&first.edges, 0)?;
        let e2 = *at(&second.edges, 0)?;
        let a = end_atom(graph, e1)?;
        let b = end_atom(graph, e2)?;
        if graph.node(graph.edge(e1)?.begin)?.atom == Some(foci[0]) {
            self.ranked = vec![a, b];
        } else if graph.node(graph.edge(e2)?.begin)?.atom == Some(foci[0]) {
            self.ranked = vec![b, a];
        }
        if self.kind == Kind::Sp2Bond {
            let cis = (self.config == 4) ^ flip;
            Ok(match (cis, first.pseudo != second.pseudo) {
                (true, false) => Descriptor::Z,
                (false, false) => Descriptor::E,
                (true, true) => Descriptor::SeqCis,
                (false, true) => Descriptor::SeqTrans,
            })
        } else {
            let ccw = (self.config == 7) ^ flip;
            Ok(match (ccw, first.pseudo || second.pseudo) {
                (true, false) => Descriptor::M,
                (false, false) => Descriptor::P,
                (true, true) => Descriptor::PseudoM,
                (false, true) => Descriptor::PseudoP,
            })
        }
    }
}

// Chirality.cpp findHighestCIPNeighbor: missing ranks and ties can clear the
// candidate, and a later neighbor may become the candidate again.
fn highest(mol: &Molecule<'_>, atom: usize, skip: usize) -> Result<Option<usize>, Error> {
    let (mut best, mut rank) = (None, 0);
    for &(neighbor, _) in at(&mol.adjacent, atom)? {
        if neighbor == skip {
            continue;
        }
        let Some(next) = at(&mol.state.properties.atoms, neighbor)?.cip_rank else {
            return Ok(None);
        };
        if next > rank || best.is_none() {
            best = Some(neighbor);
            rank = next;
        } else if next == rank {
            best = None;
        }
    }
    Ok(best)
}
fn is_internal(graph: &Digraph<'_>, edge: usize, foci: [usize; 2]) -> Result<bool, Error> {
    let e = graph.edge(edge)?;
    let a = graph.node(e.begin)?.atom;
    let b = graph.node(e.end)?.atom;
    Ok((a == Some(foci[0]) && b == Some(foci[1])) || (a == Some(foci[1]) && b == Some(foci[0])))
}
fn filtered(
    graph: &Digraph<'_>,
    edges: &[usize],
    foci: [usize; 2],
    kind: Kind,
) -> Result<Vec<usize>, Error> {
    let mut output = Vec::new();
    for &edge in edges {
        if is_internal(graph, edge, foci)? {
            continue;
        }
        let e = graph.edge(edge)?;
        if kind == Kind::AtropisomerBond
            && (graph.node(e.begin)?.is_duplicate_or_h() || graph.node(e.end)?.is_duplicate_or_h())
        {
            continue;
        }
        output.push(edge);
    }
    Ok(output)
}
fn swaps(graph: &Digraph<'_>, edges: &[usize], carrier: usize, kind: Kind) -> Result<bool, Error> {
    if edges.len() <= 1 {
        return Ok(false);
    }
    if kind == Kind::Sp2Bond {
        Ok(end_atom(graph, *at(edges, 0)?)? != Some(carrier))
    } else {
        Ok(end_atom(graph, *at(edges, 1)?)? == Some(carrier))
    }
}
