use super::*;
impl Context {
    pub(super) fn legal_center(&mut self, i: usize, work: &mut Work) -> Result<bool, String> {
        let atom = at(&self.state.graph.atoms, i)?.clone();
        let mut degree = 0;
        for &b in at(&self.neighbors, i)? {
            work.spend(1)?;
            let bond = at(&self.state.graph.bonds, b)?;
            if bond.order != 5 || bond.a != i {
                degree += 1;
            }
        }
        let hs = self.hydrogens(i)?;
        if !(3..=4).contains(&(degree + hs))
            || (degree < 3 && !matches!(atom.atomic_number, 15 | 33))
        {
            return Ok(false);
        }
        if degree != 3 {
            return Ok(true);
        }
        if hs == 1 {
            for &b in at(&self.neighbors, i)? {
                work.spend(1)?;
                let neighbor = at(&self.state.graph.atoms, self.other(b, i)?)?;
                if neighbor.atomic_number == 1 && neighbor.isotope == 0 {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        match atom.atomic_number {
            7 => {
                if *at(&self.state.hybridizations, i)? != Hybridization::Sp3 {
                    return Ok(false);
                }
                for &b in at(&self.neighbors, i)? {
                    work.spend(1)?;
                    if *at(&self.state.conjugated, b)? {
                        return Ok(false);
                    }
                }
                Ok(self.in_three_ring(i, work)? || self.bridgehead(i, work)?)
            }
            15 | 33 => Ok(true),
            16 | 34 => Ok(at(&self.state.valences, i)?.explicit_valence == 4
                || (at(&self.state.valences, i)?.explicit_valence == 3 && atom.charge == 1)),
            _ => Ok(false),
        }
    }
    pub(super) fn assign_atoms(
        &mut self,
        possible: bool,
        work: &mut Work,
    ) -> Result<(bool, bool), String> {
        let (mut unassigned, mut changed) = (0usize, false);
        for i in 0..self.state.graph.atoms.len() {
            work.spend(1)?;
            let mut tag = at(&self.state.metadata.atoms, i)?.chiral_tag;
            if (!possible && matches!(tag, 0 | 3))
                || at(&self.state.properties.atoms, i)?.cip_code.is_some()
            {
                continue;
            }
            self.ensure_ranks(work)?;
            if !self.legal_center(i, work)? {
                continue;
            }
            unassigned += 1;
            let mut decorated = Vec::new();
            let mut seen = HashSet::new();
            let mut duplicate = false;
            for (position, &b) in at(&self.neighbors, i)?.iter().enumerate() {
                work.spend(1)?;
                let rank = *at(&self.ranks, self.other(b, i)?)?;
                decorated.push((rank, position));
                let bond = at(&self.state.graph.bonds, b)?;
                if bond.order == 5 && bond.a == i {
                    continue;
                }
                if !seen.insert(rank) {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            let props = self
                .state
                .properties
                .atoms
                .get_mut(i)
                .ok_or("Missing stereo atom properties")?;
            if possible {
                props.possible = Some(true);
            }
            if matches!(tag, 0 | 3) {
                continue;
            }
            changed = true;
            unassigned -= 1;
            work.spend(
                decorated
                    .len()
                    .saturating_mul(decorated.len().max(1).ilog2() as usize + 1),
            )?;
            decorated.sort_by_key(|p| p.0);
            let mut odd = permutation_odd(&decorated, work)?;
            odd ^= decorated.len() == 3 && self.hydrogens(i)? == 1;
            if odd {
                tag = if tag == 2 { 1 } else { 2 };
            }
            self.state
                .properties
                .atoms
                .get_mut(i)
                .ok_or("Missing stereo atom properties")?
                .cip_code = Some(if tag == 2 { "S" } else { "R" }.into());
        }
        Ok((unassigned > 0, changed))
    }
}

fn permutation_odd(order: &[(u32, usize)], work: &mut Work) -> Result<bool, String> {
    let mut tree = vec![0u32; order.len() + 1];
    let mut odd = false;
    for &(_, position) in order.iter().rev() {
        if position >= order.len() {
            return Err("Invalid stereo neighbor permutation".into());
        }
        let mut i = position;
        while i > 0 {
            work.spend(1)?;
            odd ^= (*at(&tree, i)? & 1) != 0;
            i -= i & i.wrapping_neg();
        }
        i = position + 1;
        while i < tree.len() {
            work.spend(1)?;
            *tree.get_mut(i).ok_or("Missing parity slot")? += 1;
            i += i & i.wrapping_neg();
        }
    }
    Ok(odd)
}
