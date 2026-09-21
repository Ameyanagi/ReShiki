use super::*;
impl Context {
    fn directed_neighbors(
        &self,
        atom: usize,
        reference: usize,
        unknown: &mut bool,
        work: &mut Work,
    ) -> Result<Vec<(usize, Direction)>, String> {
        let mut result = Vec::new();
        let mut seen = false;
        for &b in at(&self.neighbors, atom)? {
            work.spend(1)?;
            let mut dir = *at(&self.state.directions, b)?;
            *unknown |=
                dir == Direction::Unknown || at(&self.state.metadata.bonds, b)?.unknown_stereo;
            if b == reference {
                continue;
            }
            if directed(dir) {
                seen = true;
                if at(&self.state.graph.bonds, b)?.a != atom {
                    dir = opposite(dir)?;
                }
            }
            result.push((self.other(b, atom)?, dir));
        }
        if !seen
            || (result.len() == 2
                && at(&self.ranks, at(&result, 0)?.0)? == at(&self.ranks, at(&result, 1)?.0)?)
        {
            return Ok(Vec::new());
        }
        if !directed(at(&result, 0)?.1) {
            let dir = opposite(at(&result, 1)?.1)?;
            result.get_mut(0).ok_or("Missing stereo neighbor")?.1 = dir;
        } else if result.len() > 1 && !directed(at(&result, 1)?.1) {
            let dir = opposite(at(&result, 0)?.1)?;
            result.get_mut(1).ok_or("Missing stereo neighbor")?.1 = dir;
        }
        Ok(result)
    }
    fn best_neighbor(
        &self,
        neighbors: &[(usize, Direction)],
    ) -> Result<(usize, Direction), String> {
        let first = *at(neighbors, 0)?;
        if neighbors.len() == 1 || at(&self.ranks, first.0)? > at(&self.ranks, at(neighbors, 1)?.0)?
        {
            Ok(first)
        } else {
            Ok(*at(neighbors, 1)?)
        }
    }
    pub(super) fn assign_bonds(&mut self, work: &mut Work) -> Result<(bool, bool), String> {
        let (mut unassigned, mut changed) = (0usize, false);
        let mut clear = HashSet::new();
        for i in 0..self.state.graph.bonds.len() {
            work.spend(1)?;
            let bond = at(&self.state.graph.bonds, i)?.clone();
            if bond.order != 2 || at(&self.state.metadata.bonds, i)?.stereo != 0 {
                continue;
            }
            self.ensure_ranks(work)?;
            self.state
                .metadata
                .bonds
                .get_mut(i)
                .ok_or("Missing stereo bond")?
                .stereo_atoms
                .clear();
            if !self.detectable(i)?
                || !matches!(at(&self.neighbors, bond.a)?.len(), 2 | 3)
                || !matches!(at(&self.neighbors, bond.b)?.len(), 2 | 3)
            {
                continue;
            }
            unassigned += 1;
            let mut unknown = at(&self.state.properties.atoms, bond.a)?.unknown
                || at(&self.state.properties.atoms, bond.b)?.unknown;
            let left = self.directed_neighbors(bond.a, i, &mut unknown, work)?;
            let right = self.directed_neighbors(bond.b, i, &mut unknown, work)?;
            if left.is_empty() || right.is_empty() {
                continue;
            }
            let (first, first_dir) = self.best_neighbor(&left)?;
            let (second, second_dir) = self.best_neighbor(&right)?;
            let conflict_left = left.len() == 2 && at(&left, 0)?.1 == at(&left, 1)?.1;
            let conflict_right = right.len() == 2 && at(&right, 0)?.1 == at(&right, 1)?.1;
            if conflict_left || conflict_right {
                for (conflict, neighbors, atom) in [
                    (conflict_left, &left, bond.a),
                    (conflict_right, &right, bond.b),
                ] {
                    if conflict {
                        for &(other, _) in neighbors {
                            clear.insert(
                                *self
                                    .pairs
                                    .get(&(atom.min(other), atom.max(other)))
                                    .ok_or("Missing conflicting stereo bond")?,
                            );
                        }
                    }
                }
            } else {
                let meta = self
                    .state
                    .metadata
                    .bonds
                    .get_mut(i)
                    .ok_or("Missing stereo bond")?;
                meta.stereo_atoms = vec![first, second];
                meta.stereo = if unknown {
                    1
                } else if first_dir == second_dir {
                    2
                } else {
                    3
                };
            }
            changed = true;
            unassigned -= 1;
        }
        for i in clear {
            put(&mut self.state.directions, i, Direction::None)?;
        }
        Ok((unassigned > 0, changed))
    }
    pub(super) fn cleanup_bonds(&mut self, work: &mut Work) -> Result<(), String> {
        for i in 0..self.state.graph.bonds.len() {
            work.spend(1)?;
            let bond = at(&self.state.graph.bonds, i)?.clone();
            let dir = *at(&self.state.directions, i)?;
            if matches!(dir, Direction::Wedge | Direction::Hash)
                && at(&self.state.metadata.atoms, bond.a)?.chiral_tag == 0
                && at(&self.state.metadata.atoms, bond.b)?.chiral_tag == 0
            {
                let mut atrop = false;
                for &b in at(&self.neighbors, bond.a)? {
                    work.spend(1)?;
                    if matches!(at(&self.state.metadata.bonds, b)?.stereo, 6 | 7) {
                        atrop = true;
                        break;
                    }
                }
                if !atrop {
                    put(&mut self.state.directions, i, Direction::None)?;
                }
            }
            if bond.order == 2
                && (dir == Direction::EitherDouble
                    || at(&self.state.metadata.bonds, i)?.stereo == 1)
                && (at(&self.neighbors, bond.a)?.len() == 1
                    || at(&self.neighbors, bond.b)?.len() == 1)
            {
                if dir == Direction::EitherDouble {
                    put(&mut self.state.directions, i, Direction::None)?;
                }
                self.state
                    .metadata
                    .bonds
                    .get_mut(i)
                    .ok_or("Missing stereo bond")?
                    .stereo = 0;
            }
            if bond.order == 2 && matches!(at(&self.state.metadata.bonds, i)?.stereo, 0 | 1) {
                for atom in [bond.a, bond.b] {
                    for &b in at(&self.neighbors, atom)? {
                        work.spend(1)?;
                        if b == i
                            || !directed(*at(&self.state.directions, b)?)
                            || !matches!(at(&self.state.graph.bonds, b)?.order, 1 | 4)
                        {
                            continue;
                        }
                        let mut keep = false;
                        for &other in at(&self.neighbors, self.other(b, atom)?)? {
                            work.spend(1)?;
                            if at(&self.state.graph.bonds, other)?.order == 2
                                && !matches!(at(&self.state.metadata.bonds, other)?.stereo, 0 | 1)
                            {
                                keep = true;
                                break;
                            }
                        }
                        if !keep {
                            put(&mut self.state.directions, b, Direction::None)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
