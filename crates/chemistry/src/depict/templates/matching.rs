use super::{Error, Input, Result, Work, at, catalog::Template};

enum Candidates {
    All(usize),
    Neighbors(usize, usize),
}
impl Candidates {
    fn next(&mut self, input: &Input<'_>) -> Result<Option<usize>> {
        match self {
            Self::All(i) => {
                if *i >= input.graph.atoms.len() {
                    return Ok(None);
                }
                let next = *i;
                *i += 1;
                Ok(Some(next))
            }
            Self::Neighbors(id, i) => {
                let next = at(&input.adjacent, *id)?.get(*i).copied();
                if next.is_some() {
                    *i += 1;
                }
                Ok(next)
            }
        }
    }
}
impl Input<'_> {
    pub(super) fn first_match(
        &self,
        template: &Template,
        included: &[bool],
        work: &mut Work,
    ) -> Result<Option<Vec<usize>>> {
        let mut mapping = vec![None; template.degrees.len()];
        let mut frames = vec![Candidates::All(0)];
        while let Some(frame) = frames.last_mut() {
            work.spend(1)?;
            let candidate = frame.next(self)?;
            let depth = frames.len() - 1;
            *mapping
                .get_mut(depth)
                .ok_or(Error::Invalid("query depth"))? = None;
            let Some(candidate) = candidate else {
                frames.pop();
                continue;
            };
            work.spend(mapping.len())?;
            if !*at(included, candidate)?
                || mapping.contains(&Some(candidate))
                || at(&template.degrees, depth)?
                    .is_some_and(|d| self.adjacent.get(candidate).is_none_or(|a| a.len() != d))
            {
                continue;
            }
            let mut compatible = true;
            for &[a, b] in &template.edges {
                work.spend(1)?;
                let other = if a == depth {
                    b
                } else if b == depth {
                    a
                } else {
                    continue;
                };
                if let Some(mapped) = *at(&mapping, other)?
                    && !self.bonds.contains_key(&(candidate, mapped))
                {
                    compatible = false;
                    break;
                }
            }
            if !compatible {
                continue;
            }
            *mapping
                .get_mut(depth)
                .ok_or(Error::Invalid("query depth"))? = Some(candidate);
            if depth + 1 == mapping.len() {
                return mapping
                    .into_iter()
                    .map(|a| a.ok_or(Error::Invalid("incomplete mapping")))
                    .collect::<Result<Vec<_>>>()
                    .map(Some);
            }
            let next = depth + 1;
            let parent = template
                .edges
                .iter()
                .find_map(|&[a, b]| {
                    let other = if a == next {
                        b
                    } else if b == next {
                        a
                    } else {
                        return None;
                    };
                    mapping.get(other).copied().flatten()
                })
                .ok_or(Error::Invalid("disconnected query"))?;
            frames.push(Candidates::Neighbors(parent, 0));
        }
        Ok(None)
    }
    pub(super) fn stereo_matches(
        &self,
        template: &Template,
        mapping: &[usize],
        work: &mut Work,
    ) -> Result<bool> {
        for (i, bond) in self.graph.bonds.iter().enumerate() {
            work.spend(1)?;
            let stereo = at(&self.metadata.bonds, i)?;
            if bond.order != 2 || stereo.stereo <= 1 || stereo.stereo_atoms.len() != 2 {
                continue;
            }
            let (first, second) = (bond.a, bond.b);
            let (n1, n2) = (*at(&stereo.stereo_atoms, 0)?, *at(&stereo.stereo_atoms, 1)?);
            let alternate = |id, other, control| -> Result<Option<usize>> {
                let neighbors = at(&self.adjacent, id)?;
                Ok(if neighbors.len() > 2 {
                    neighbors
                        .iter()
                        .copied()
                        .find(|&n| n != other && n != control)
                } else {
                    None
                })
            };
            let (alt1, alt2) = (alternate(first, second, n1)?, alternate(second, first, n2)?);
            let mut selected = [None; 6];
            for (query, &target) in mapping.iter().enumerate() {
                work.spend(1)?;
                // Retain the original else-if precedence for overlapping IDs.
                let slot = if target == first {
                    0
                } else if target == second {
                    1
                } else if target == n1 {
                    2
                } else if target == n2 {
                    3
                } else if Some(target) == alt1 {
                    4
                } else if Some(target) == alt2 {
                    5
                } else {
                    continue;
                };
                *selected.get_mut(slot).ok_or(Error::Limit)? = Some(query);
            }
            let mut swap = false;
            if selected[2].is_none() {
                selected[2] = selected[4];
                swap = !swap;
            }
            if selected[3].is_none() {
                selected[3] = selected[5];
                swap = !swap;
            }
            let [Some(a), Some(b), Some(c), Some(d), _, _] = selected else {
                return Ok(false);
            };
            let (a, b, c, d) = (
                *at(&template.positions, a)?,
                *at(&template.positions, b)?,
                *at(&template.positions, c)?,
                *at(&template.positions, d)?,
            );
            let (v12, v42, v32) = (
                (c.x - a.x, c.y - a.y),
                (d.x - a.x, d.y - a.y),
                (b.x - a.x, b.y - a.y),
            );
            let cross1 = v32.0 * v12.1 - v32.1 * v12.0;
            let cross2 = v32.0 * v42.1 - v32.1 * v42.0;
            let mut cis = cross1 * cross2 > 0.0;
            if swap {
                cis = !cis;
            }
            if cis != matches!(stereo.stereo, 2 | 4) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
