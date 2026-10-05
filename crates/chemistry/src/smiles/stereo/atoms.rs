use super::*;

impl Context<'_> {
    pub(super) fn atoms(&mut self) -> Result<(), Error> {
        let potential = perception::potential_tetrahedral_centers(self.input).map_err(invalid)?;
        let n = self.input.graph.atoms.len();
        let first = match self.walk.tokens().first() {
            Some(Token::Atom(a)) => *a,
            _ => return Err(invalid("Missing traversal start")),
        };
        let mut swaps = vec![false; n];
        let mut permutations = vec![0; n];
        for a in 0..n {
            self.work.spend(1)?;
            let meta = at(&self.input.metadata.atoms, a)?;
            if meta.chiral_tag == 0
                || *at(self.broken, a)?
                || !(*at(&potential, a)? || matches!(meta.chiral_tag, 6..=8))
            {
                continue;
            }
            let order = at(self.walk.atom_bond_order(), a)?;
            let adjacent = at(&self.adjacent, a)?;
            let permutation = if matches!(meta.chiral_tag, 6..=8) {
                meta.chiral_permutation.unwrap_or(0)
            } else {
                0
            };
            let mut odd = false;
            if permutation == 0 {
                // Cycle parity also stays linear for malformed high-degree
                // coordination centers retained by a supplied native cache.
                let positions = adjacent
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| (b, i))
                    .collect::<std::collections::HashMap<_, _>>();
                let mut visited = vec![false; order.len()];
                for start in 0..order.len() {
                    if *at(&visited, start)? {
                        continue;
                    }
                    let (mut i, mut length) = (start, 0usize);
                    while !*at(&visited, i)? {
                        self.work.spend(1)?;
                        put(&mut visited, i, true)?;
                        length += 1;
                        i = *positions.get(at(order, i)?).ok_or(Error::Limit)?;
                    }
                    odd ^= length.is_multiple_of(2);
                }
            } else {
                put(
                    &mut permutations,
                    a,
                    super::super::chirality::output_permutation(
                        meta.chiral_tag,
                        permutation,
                        order,
                        adjacent,
                        a == first,
                    )?,
                )?;
            }
            let atom = at(&self.input.graph.atoms, a)?;
            let valence = at(&self.input.valences, a)?;
            let fourth = atom.explicit_hydrogens == 1 || valence.implicit_hydrogens == 1;
            let mut unsaturated = false;
            for &b in adjacent {
                self.work.spend(1)?;
                unsaturated |= matches!(at(&self.input.graph.bonds, b)?.order, 2 | 3 | 4 | 6 | 7);
            }
            odd ^= adjacent.len() == 3
                && (a == first && atom.explicit_hydrogens == 1
                    || !fourth && at(self.walk.ring_closures(), a)?.len() == 1 && !unsaturated);
            put(&mut swaps, a, odd)?;
        }
        let mut adjusted = vec![false; n];
        for token in self.walk.tokens() {
            let Token::Atom(a) = *token else {
                continue;
            };
            self.work.spend(1)?;
            if at(&self.output.metadata.atoms, a)?.chiral_tag == 0 || *at(self.broken, a)? {
                continue;
            }
            if let Some(members) = &at(&self.input.properties.atoms, a)?.ring_members {
                if !*at(&adjusted, a)? {
                    self.output
                        .metadata
                        .atoms
                        .get_mut(a)
                        .ok_or(Error::Limit)?
                        .chiral_tag = 2;
                    put(&mut adjusted, a, true)?;
                }
                for &signed in members {
                    self.work.spend(1)?;
                    let other = usize::try_from(signed.unsigned_abs())
                        .map_err(|_| Error::Limit)?
                        .checked_sub(1)
                        .ok_or(Error::Limit)?;
                    if !*at(&adjusted, other)?
                        && at(&self.atom_visit, other)? > at(&self.atom_visit, a)?
                    {
                        let mut tag = at(&self.output.metadata.atoms, a)?.chiral_tag;
                        if (signed < 0) ^ (*at(&swaps, a)? != *at(&swaps, other)?) {
                            tag = invert(tag);
                        }
                        self.output
                            .metadata
                            .atoms
                            .get_mut(other)
                            .ok_or(Error::Limit)?
                            .chiral_tag = tag;
                        put(&mut adjusted, other, true)?;
                    }
                }
            } else {
                let meta = self.output.metadata.atoms.get_mut(a).ok_or(Error::Limit)?;
                if matches!(meta.chiral_tag, 1 | 2) {
                    if *at(&swaps, a)? {
                        meta.chiral_tag = invert(meta.chiral_tag);
                    }
                } else if *at(&permutations, a)? != 0 {
                    meta.chiral_permutation = Some(*at(&permutations, a)?);
                }
            }
        }
        Ok(())
    }
}
fn invert(tag: u8) -> u8 {
    match tag {
        1 => 2,
        2 => 1,
        _ => tag,
    }
}
