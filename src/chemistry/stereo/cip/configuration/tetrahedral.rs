use super::*;
use crate::chemistry::stereo::cip::{
    at_mut,
    digraph::{BOND_DUPLICATE, IMPLICIT_HYDROGEN},
};

impl Configuration {
    pub fn tetrahedral(mol: &Molecule<'_>, focus: usize) -> Result<Self, Error> {
        let config = at(&mol.state.metadata.atoms, focus)?.chiral_tag;
        if !matches!(config, 1 | 2) {
            return Err(invalid("Invalid tetrahedral winding"));
        }
        let mut carriers: Vec<_> = at(&mol.adjacent, focus)?
            .iter()
            .map(|&(a, _)| Some(a))
            .collect();
        if carriers.len() < 4 {
            carriers.push(Some(focus));
        }
        if carriers.len() < 4 {
            carriers.push(None);
        }
        if carriers.len() != 4 {
            return Err(invalid("Expected four tetrahedral carriers"));
        }
        Ok(Self {
            kind: Kind::Tetrahedral,
            target: Target::Atom(focus),
            foci: vec![focus],
            carriers,
            config,
            ranked: Vec::new(),
        })
    }
    pub(super) fn label_tetrahedral<'a>(
        &mut self,
        mol: &mut Molecule<'a>,
        graph: &mut Digraph<'a>,
        node: usize,
        rules: &Rules,
        iterations: &mut Iterations,
    ) -> Result<Descriptor, Error> {
        let edges = graph.edges(mol, node)?.to_vec();
        if edges.len() < 3 {
            return Ok(Descriptor::Other);
        }
        let mut priority = sort(mol, graph, iterations, rules, node, &edges)?;
        if !priority.unique && edges.len() == 4 {
            if rules.num_subrules() == 3 {
                return Ok(Descriptor::Unknown);
            }
            let partition =
                rules.groups(&mut Context::new(mol, graph, iterations)?, &priority.edges)?;
            if partition.len() == 2 {
                let atom = end_atom(graph, *at(&priority.edges, 1)?)?;
                graph.set_rule6_reference(atom)?;
                let result = sort(mol, graph, iterations, rules, node, &priority.edges);
                // Clear the temporary rule-6 anchor even on a bounded failure.
                graph.set_rule6_reference(None)?;
                priority = result?;
            } else if partition.len() == 1 {
                let result = (|| {
                    graph.set_rule6_reference(end_atom(graph, *at(&priority.edges, 0)?)?)?;
                    let first = sort(mol, graph, iterations, rules, node, &priority.edges)?;
                    graph.set_rule6_reference(end_atom(graph, *at(&first.edges, 1)?)?)?;
                    let second = sort(mol, graph, iterations, rules, node, &first.edges)?;
                    Ok::<_, Error>((first, second))
                })();
                graph.set_rule6_reference(None)?;
                let (first, second) = result?;
                if parity4(&first.edges, &second.edges)? == 1 {
                    return Ok(Descriptor::Unknown);
                }
                priority = second;
            }
            if !priority.unique {
                return Ok(Descriptor::Unknown);
            }
        } else if !priority.unique {
            return Ok(Descriptor::Unknown);
        }
        let mut ordered = [None; 4];
        let mut index = 0;
        for &edge in &priority.edges {
            let end = graph.node(graph.edge(edge)?.end)?;
            if end.flags & (BOND_DUPLICATE | IMPLICIT_HYDROGEN) != 0 {
                continue;
            }
            let atom = end
                .atom
                .ok_or_else(|| invalid("Missing tetrahedral neighbor atom"))?;
            *at_mut(&mut ordered, index)? = Some(atom);
            self.ranked.push(Some(atom));
            index += 1;
        }
        if index < 4 {
            *at_mut(&mut ordered, index)? = Some(self.focus()?);
        }
        let parity = parity4(&ordered, &self.carriers)?;
        if parity == 0 {
            return Err(invalid("Could not calculate parity: carrier mismatch"));
        }
        let ccw = (self.config == 2) ^ (parity == 1);
        Ok(match (ccw, priority.pseudo) {
            (true, false) => Descriptor::S,
            (false, false) => Descriptor::R,
            (true, true) => Descriptor::PseudoS,
            (false, true) => Descriptor::PseudoR,
        })
    }
}
