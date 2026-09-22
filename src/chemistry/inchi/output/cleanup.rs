//! InChI-specific native cleanup, before RDKit's general sanitizer.
mod matching;
use super::{Assembly, Error, Topology, Work, at, at_mut, invalid, validate_assembly};
use crate::chemistry::{graph::Graph, stereo::perception::State};

/// Apply every native InchiToMol cleanup rule in atom order. Cache updates and
/// bond insertion order are retained; neither sanitization nor perception runs.
pub fn clean_up(input: &Assembly) -> Result<Assembly, Error> {
    validate_assembly(input)?;
    let mut result = input.clone();
    let Some(state) = result.state.take() else {
        return Ok(result);
    };
    let topology = Topology::new(&state.graph)?;
    let mut context = Context {
        state,
        topology,
        unspecified: result.unspecified_bonds,
        work: Work(50_000_000),
    };
    for id in 0..context.state.graph.atoms.len() {
        context.work.spend(1)?;
        context.atom(id)?;
    }
    result.unspecified_bonds = context.unspecified;
    result.state = Some(context.state);
    Ok(result)
}

struct Context {
    state: State,
    topology: Topology,
    unspecified: Vec<bool>,
    work: Work,
}
impl Context {
    fn number(&self, atom: usize) -> Result<u8, Error> {
        Ok(at(&self.state.graph.atoms, atom)?.atomic_number)
    }
    fn charge(&self, atom: usize) -> Result<i8, Error> {
        Ok(at(&self.state.graph.atoms, atom)?.charge)
    }
    fn set_charge(&mut self, atom: usize, charge: i8) -> Result<(), Error> {
        at_mut(&mut self.state.graph.atoms, atom)?.charge = charge;
        Ok(())
    }
    fn set_number(&mut self, atom: usize, number: u8) -> Result<(), Error> {
        at_mut(&mut self.state.graph.atoms, atom)?.atomic_number = number;
        Ok(())
    }
    fn order(&self, bond: usize) -> Result<u8, Error> {
        Ok(at(&self.state.graph.bonds, bond)?.order)
    }
    fn set_order(&mut self, bond: usize, order: u8) -> Result<(), Error> {
        at_mut(&mut self.state.graph.bonds, bond)?.order = order;
        *at_mut(&mut self.unspecified, bond)? = false;
        Ok(())
    }
    fn edge(&self, a: usize, b: usize) -> Result<usize, Error> {
        self.topology
            .bond(a, b)?
            .ok_or_else(|| invalid("Missing cleanup bond"))
    }
    fn explicit(&mut self, atom: usize) -> Result<u32, Error> {
        // The existing cache implementation only needs this atom's incident
        // environment. A detached star avoids repeated whole-molecule scans
        // while preserving charge/aromaticity/dative direction semantics.
        let edges = at(&self.topology.edges, atom)?;
        self.work.spend(edges.len() + 1)?;
        let mut local = Graph {
            atoms: vec![at(&self.state.graph.atoms, atom)?.clone()],
            bonds: Vec::with_capacity(edges.len()),
        };
        for &(other, bond) in edges {
            let next = local.atoms.len();
            local
                .atoms
                .push(at(&self.state.graph.atoms, other)?.clone());
            let mut edge = at(&self.state.graph.bonds, bond)?.clone();
            (edge.a, edge.b) = if edge.a == atom { (0, next) } else { (next, 0) };
            local.bonds.push(edge);
        }
        let cache = local.provisional_valences().map_err(Error::Chemistry)?;
        let value = cache
            .first()
            .ok_or_else(|| invalid("Missing explicit cache"))?
            .explicit_valence;
        if value > 127 {
            return Err(Error::NativeCacheBoundary);
        }
        at_mut(&mut self.state.valences, atom)?.explicit_valence = value;
        Ok(value)
    }
    fn flip(&mut self, path: &[usize]) -> Result<(), Error> {
        for &bond in path.iter().rev() {
            self.set_order(bond, if self.order(bond)? == 2 { 1 } else { 2 })?;
        }
        Ok(())
    }
    fn atom(&mut self, id: usize) -> Result<(), Error> {
        match self.number(id)? {
            7 => {
                if self.explicit(id)? == 4 {
                    if self.nitrogen_ring(id, 0)? {
                        return Ok(());
                    }
                    if self.charge(id)? == -1
                        && let Some((target, path)) = self.path(id, 7, 0, 2, 2, 1)?
                    {
                        self.set_order(
                            *path.last().ok_or_else(|| invalid("Missing direct path"))?,
                            1,
                        )?;
                        self.set_charge(id, 0)?;
                        self.set_charge(target, -1)?;
                    }
                    return Ok(());
                }
                if self.charge(id)? != 0 {
                    return Ok(());
                }
                let aromatic = at(&self.state.graph.atoms, id)?.aromatic;
                at_mut(&mut self.state.graph.atoms, id)?.aromatic = false;
                if self.explicit(id)? == 5 {
                    self.nitrogen_five(id)?;
                }
                at_mut(&mut self.state.graph.atoms, id)?.aromatic = aromatic;
            }
            17 => {
                if self.explicit(id)? == 8 && self.chlorine_eight(id)? {
                    return Ok(());
                }
                // Preserve the original dispatch/predicate discrepancy: this
                // branch dispatches at five, but its helper requires six.
                if self.explicit(id)? == 5
                    && self.explicit(id)? == 6
                    && self.charge(id)? == 1
                    && let Some((target, path)) = self.path(id, 8, -1, 1, 1, 1)?
                {
                    self.set_order(
                        *path
                            .last()
                            .ok_or_else(|| invalid("Missing chlorine path"))?,
                        2,
                    )?;
                    self.set_charge(id, 0)?;
                    self.set_charge(target, 0)?;
                    self.explicit(id)?;
                    return Ok(());
                }
                if self.explicit(id)? == 3
                    && self.charge(id)? == 0
                    && let Some((_, path)) = self.path(id, 16, 0, 3, 3, 1)?
                {
                    self.set_order(
                        *path
                            .last()
                            .ok_or_else(|| invalid("Missing chlorine path"))?,
                        1,
                    )?;
                    self.explicit(id)?;
                }
            }
            16 => {
                let valence = self.explicit(id)?;
                if valence == 7 {
                    if self.sulfur_one(id)?
                        || self.sulfur_path(id, 3, 3)?
                        || self.sulfur_path(id, 2, 1)?
                    {
                        return Ok(());
                    }
                    self.sulfur_path(id, 2, 9)?;
                } else if valence == 8 {
                    // The original valence-eight helper requires valence seven.
                    self.explicit(id)?;
                }
            }
            35 if self.explicit(id)? == 3
                && self.charge(id)? == 0
                && at(&self.topology.edges, id)?.len() == 1 =>
            {
                let &(other, bond) = at(&self.topology.edges, id)?
                    .first()
                    .ok_or_else(|| invalid("Missing bromine bond"))?;
                if self.number(other)? == 34 {
                    self.set_order(bond, 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn nitrogen_five(&mut self, id: usize) -> Result<(), Error> {
        for rule in 1..=4 {
            if self.nitrogen_ring(id, rule)? {
                return Ok(());
            }
        }
        if self.nitrogen_azo(id)? {
            return Ok(());
        }
        if let Some((target, path)) = self.path(id, 7, 1, 2, 2, 5)? {
            self.set_charge(target, 0)?;
            self.explicit(target)?;
            self.flip(&path)?;
            self.set_charge(id, 1)?;
            return Ok(());
        }
        if let Some((target, path)) = self.path(id, 7, -1, 3, 1, 2)? {
            let &bond = path
                .last()
                .ok_or_else(|| invalid("Missing nitrogen path"))?;
            self.set_order(bond, 1)?;
            let edge = at(&self.state.graph.bonds, bond)?;
            self.set_charge(if edge.a == id { edge.b } else { edge.a }, -1)?;
            let &last = path
                .first()
                .ok_or_else(|| invalid("Missing nitrogen path end"))?;
            self.set_order(last, 2)?;
            self.set_charge(target, 0)?;
            self.explicit(target)?;
            self.explicit(id)?;
            return Ok(());
        }
        if let Some((target, path)) = self.path(id, 7, 0, 2, 2, 1)? {
            if self.path(id, 8, 0, 2, 2, 1)?.is_none() {
                self.set_charge(target, -1)?;
                self.explicit(target)?;
                self.set_order(
                    *path
                        .last()
                        .ok_or_else(|| invalid("Missing nitrogen path"))?,
                    1,
                )?;
                self.set_charge(id, 1)?;
                self.explicit(id)?;
            }
            return Ok(());
        }
        let silicon = at(&self.topology.edges, id)?
            .iter()
            .copied()
            .filter(|&(a, b)| {
                self.number(a).is_ok_and(|n| n == 14)
                    && self.charge(a).is_ok_and(|n| n == -1)
                    && self.order(b).is_ok_and(|n| n == 2)
            })
            .collect::<Vec<_>>();
        if silicon.len() == 2 {
            for (other, bond) in silicon {
                self.set_charge(other, 0)?;
                self.set_order(bond, 1)?;
            }
            return Ok(());
        }
        for number in [8, 16, 9, 17] {
            let neutral = self.path(id, number, 0, 2, 2, 7)?;
            let charged = self.path(id, number, 1, 2, 2, 7)?;
            if neutral.is_none() && charged.is_none() {
                continue;
            }
            if let (Some(_), Some((target, _))) = (&neutral, &charged) {
                self.set_charge(*target, 0)?;
                at_mut(&mut self.state.graph.atoms, *target)?.explicit_hydrogens = 0;
            }
            self.set_charge(id, 1)?;
            let (_, path) = neutral
                .as_ref()
                .or(charged.as_ref())
                .ok_or_else(|| invalid("Missing cleanup target"))?;
            self.flip(path)?;
            match (&neutral, &charged) {
                (Some((target, _)), Some(_)) => {
                    at_mut(&mut self.state.graph.atoms, *target)?.explicit_hydrogens = 1
                }
                (Some((target, _)), None) => self.set_charge(*target, -1)?,
                (None, Some((target, _))) => self.set_charge(*target, 0)?,
                _ => return Err(invalid("Missing cleanup target")),
            }
            if let Some((target, _)) = charged {
                self.explicit(target)?;
            }
            if let Some((target, _)) = neutral {
                self.explicit(target)?;
            }
            return Ok(());
        }
        if let Some((target, path)) = self.path(id, 6, 0, 2, 2, 1)? {
            self.set_charge(target, -1)?;
            self.explicit(target)?;
            self.set_order(
                *path.last().ok_or_else(|| invalid("Missing carbon path"))?,
                1,
            )?;
            self.set_charge(id, 1)?;
            self.explicit(id)?;
        }
        Ok(())
    }
    fn sulfur_one(&mut self, id: usize) -> Result<bool, Error> {
        if self.charge(id)? != -1 || self.explicit(id)? != 7 {
            return Ok(false);
        }
        let (mut carbon, mut oxygen, mut last) = (0, 0, None);
        for &(other, bond) in at(&self.topology.edges, id)? {
            match self.number(other)? {
                8 => {
                    if self.order(bond)? == 2 {
                        oxygen += 1;
                        last = Some((other, bond));
                    } else {
                        oxygen = 100;
                        break;
                    }
                }
                6 if self.order(bond)? == 1 => {
                    carbon += 1;
                }
                _ => {
                    carbon = 100;
                    break;
                }
            }
        }
        if let Some((other, bond)) = last
            && (carbon == 1 || oxygen == 3)
        {
            self.set_order(bond, 1)?;
            self.set_charge(other, -1)?;
            self.set_charge(id, 0)?;
            self.explicit(other)?;
            self.explicit(id)?;
            return Ok(true);
        }
        Ok(false)
    }
    fn sulfur_path(&mut self, id: usize, ending: u8, length: usize) -> Result<bool, Error> {
        if self.charge(id)? != -1 || self.explicit(id)? != 7 {
            return Ok(false);
        }
        let Some((target, path)) = self.path(id, 7, 0, 2, ending, length)? else {
            return Ok(false);
        };
        if ending == 3 {
            for &bond in path.iter().rev() {
                self.set_order(bond, if self.order(bond)? == 2 { 1 } else { 2 })?;
            }
        } else if length == 1 {
            self.set_order(
                *path.last().ok_or_else(|| invalid("Missing sulfur path"))?,
                1,
            )?;
            self.set_charge(target, -1)?;
        } else {
            self.flip(&path)?;
            self.set_charge(target, -1)?;
            self.explicit(target)?;
            at_mut(&mut self.state.graph.atoms, target)?.explicit_hydrogens = 0;
        }
        self.set_charge(id, 0)?;
        self.explicit(id)?;
        Ok(true)
    }
    fn chlorine_eight(&mut self, id: usize) -> Result<bool, Error> {
        if self.explicit(id)? != 8 || self.charge(id)? != -1 {
            return Ok(false);
        }
        if at(&self.topology.edges, id)?
            .iter()
            .any(|&(other, _)| !self.number(other).is_ok_and(|n| n == 8))
        {
            return Ok(false);
        }
        self.set_charge(id, 3)?;
        for (other, bond) in at(&self.topology.edges, id)?.clone() {
            if self.order(bond)? == 2 {
                self.set_order(bond, 1)?;
                self.set_charge(other, -1)?;
                self.explicit(other)?;
            }
        }
        self.explicit(id)?;
        Ok(true)
    }
    fn nitrogen_ring(&mut self, id: usize, rule: usize) -> Result<bool, Error> {
        let expected = if rule == 0 { 4 } else { 5 };
        if self.charge(id)? != if rule == 0 { -1 } else { 0 } || self.explicit(id)? != expected {
            return Ok(false);
        }
        let path = if rule == 2 {
            self.path(id, 8, 0, 2, 2, 5)?
        } else {
            None
        };
        if rule == 2 && path.is_none() {
            return Ok(false);
        }
        self.set_number(id, 50)?;
        if rule == 0 {
            self.set_charge(id, 0)?;
        }
        let (numbers, bonds): (&[u8], &[(usize, usize, u8)]) = match rule {
            0 => (
                &[6, 7, 50, 7, 7],
                &[(0, 1, 1), (1, 2, 2), (2, 3, 2), (3, 4, 1), (4, 0, 2)],
            ),
            1 => (
                &[6, 6, 50, 6, 6, 7, 6],
                &[
                    (0, 1, 1),
                    (1, 2, 2),
                    (2, 3, 2),
                    (3, 4, 0),
                    (4, 5, 1),
                    (5, 0, 2),
                    (2, 6, 1),
                ],
            ),
            2 => (
                &[6, 6, 50, 7, 6, 8, 6],
                &[
                    (0, 1, 0),
                    (1, 2, 2),
                    (2, 3, 2),
                    (3, 4, 1),
                    (4, 5, 1),
                    (5, 0, 1),
                    (2, 6, 1),
                ],
            ),
            3 => (
                &[6, 7, 6, 7, 7, 50],
                &[
                    (0, 1, 1),
                    (1, 2, 2),
                    (2, 3, 1),
                    (3, 4, 2),
                    (4, 0, 1),
                    (5, 0, 2),
                ],
            ),
            4 => (
                &[6, 7, 7, 6, 6, 50],
                &[
                    (0, 1, 1),
                    (1, 2, 2),
                    (2, 3, 1),
                    (3, 4, 2),
                    (4, 0, 1),
                    (5, 0, 2),
                ],
            ),
            _ => return Err(invalid("Unknown nitrogen rule")),
        };
        let matches = self.matches(numbers, bonds)?;
        self.set_number(id, 7)?;
        if rule == 0 {
            self.set_charge(id, -1)?;
        }
        if matches.len() != 1 {
            return Ok(false);
        }
        let mapping = matches
            .first()
            .ok_or_else(|| invalid("Missing nitrogen match"))?;
        let edits: &[(usize, usize, u8)] = match rule {
            0 => &[(0, 1, 2), (1, 2, 1), (2, 3, 1), (3, 4, 2), (4, 0, 1)],
            1 => &[(0, 1, 2), (1, 2, 1), (4, 5, 2), (5, 0, 1)],
            2 => &[(1, 2, 1)],
            3 => &[(1, 2, 1), (2, 3, 2), (3, 4, 1), (4, 0, 2), (5, 0, 1)],
            4 => &[(0, 1, 2), (1, 2, 1), (5, 0, 1)],
            _ => return Err(invalid("Unknown nitrogen rule")),
        };
        for &(a, b, order) in edits {
            self.set_order(self.edge(*at(mapping, a)?, *at(mapping, b)?)?, order)?;
        }
        if let Some((target, path)) = path {
            self.flip(&path)?;
            self.set_charge(target, -1)?;
        }
        if matches!(rule, 1 | 3 | 4) {
            self.set_charge(id, 1)?;
        }
        if rule == 3 {
            self.set_charge(*at(mapping, 1)?, -1)?;
        }
        if rule == 4 {
            self.set_charge(*at(mapping, 2)?, -1)?;
        }
        Ok(true)
    }
    fn nitrogen_azo(&mut self, id: usize) -> Result<bool, Error> {
        if self.charge(id)? != 0 || self.explicit(id)? != 5 {
            return Ok(false);
        }
        let matches = self.matches(&[7, 7], &[(0, 1, 2)])?;
        let mut best = Vec::new();
        for pair in matches {
            if pair.contains(&id) {
                continue;
            }
            for &atom in &pair {
                self.set_number(atom, 50)?;
            }
            let path = self.path(id, 50, 0, 2, 2, 9)?;
            for &atom in &pair {
                self.set_number(atom, 7)?;
            }
            if let Some((_, path)) = path
                && (best.is_empty() || path.len() < best.len())
            {
                best = path;
            }
        }
        if best.is_empty() {
            return Ok(false);
        }
        self.flip(&best)?;
        self.set_charge(id, 1)?;
        self.explicit(id)?;
        Ok(true)
    }
}
