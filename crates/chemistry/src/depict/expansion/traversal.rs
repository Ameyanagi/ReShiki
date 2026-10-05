use super::*;

struct Remaining {
    order: Vec<usize>,
    active: BTreeSet<usize>,
}
impl Remaining {
    fn values(&self) -> Vec<usize> {
        self.order
            .iter()
            .copied()
            .filter(|i| self.active.contains(i))
            .collect()
    }
}
fn remove_attachment(fragment: &mut Fragment, id: usize, work: &mut Budget) -> Result<()> {
    let a = fragment
        .atoms
        .get(&id)
        .ok_or(Error::Invalid("missing merged atom"))?;
    if a.neighbors.is_empty() {
        work.spend(fragment.attachment_points.len())?;
        if let Some(i) = fragment.attachment_points.iter().position(|&a| a == id) {
            fragment.attachment_points.remove(i);
        }
    }
    Ok(())
}
impl Input<'_> {
    fn common_merges(
        &self,
        master: &mut Fragment,
        pool: &mut [Option<Fragment>],
        length: f64,
        work: &mut Budget,
    ) -> Result<()> {
        loop {
            let mut selected = None;
            for (i, candidate) in pool.iter().enumerate() {
                work.spend(1)?;
                let Some(candidate) = candidate else {
                    continue;
                };
                if candidate.done {
                    continue;
                }
                let mut common = Vec::new();
                for &id in master.atoms.keys() {
                    work.spend(1)?;
                    if candidate.atoms.contains_key(&id) {
                        common.push(id);
                    }
                }
                if !common.is_empty() {
                    selected = Some((i, common));
                    break;
                }
            }
            let Some((i, mut common)) = selected else {
                break;
            };
            let mut incoming = pool
                .get_mut(i)
                .and_then(Option::take)
                .ok_or(Error::Invalid("missing incoming fragment"))?;
            self.merge_common(master, &mut incoming, &mut common, length, work)?;
            for id in common {
                remove_attachment(master, id, work)?;
            }
        }
        Ok(())
    }
    fn expand_in_place(
        &self,
        index: usize,
        pool: &mut [Option<Fragment>],
        remaining: &mut Remaining,
        length: f64,
        work: &mut Budget,
    ) -> Result<()> {
        let mut master = pool
            .get_mut(index)
            .and_then(Option::take)
            .ok_or(Error::Invalid("missing master fragment"))?;
        if !master.done {
            return Err(Error::Invalid("expansion master must be marked done"));
        }
        self.common_merges(&mut master, pool, length, work)?;
        while let Some(&id) = master.attachment_points.first() {
            work.spend(1)?;
            let neighbors = master
                .atoms
                .get(&id)
                .ok_or(Error::Invalid("missing attachment atom"))?
                .neighbors
                .clone();
            work.spend(neighbors.len())?;
            if neighbors.is_empty() {
                return Err(Error::Invalid("empty attachment neighbors"));
            }
            for neighbor in neighbors {
                if remaining.active.remove(&neighbor) {
                    self.add_atom(&mut master, neighbor, id, length, work)?;
                } else {
                    let mut selected = None;
                    for (i, candidate) in pool.iter().enumerate() {
                        work.spend(1)?;
                        if candidate
                            .as_ref()
                            .is_some_and(|f| !f.done && f.atoms.contains_key(&neighbor))
                        {
                            selected = Some(i);
                            break;
                        }
                    }
                    if let Some(i) = selected {
                        let mut incoming = pool
                            .get_mut(i)
                            .and_then(Option::take)
                            .ok_or(Error::Invalid("missing neighbor fragment"))?;
                        self.merge_separate(
                            &mut master,
                            &mut incoming,
                            id,
                            neighbor,
                            length,
                            work,
                        )?;
                        remove_attachment(&mut master, neighbor, work)?;
                    }
                }
            }
            if master.attachment_points.first() != Some(&id) {
                return Err(Error::Invalid("attachment queue changed its head"));
            }
            work.spend(master.attachment_points.len())?;
            master.attachment_points.remove(0);
            master
                .atoms
                .get_mut(&id)
                .ok_or(Error::Invalid("missing attachment atom"))?
                .neighbors
                .clear();
            self.common_merges(&mut master, pool, length, work)?;
        }
        *pool.get_mut(index).ok_or(Error::Invalid("master index"))? = Some(master);
        Ok(())
    }
    /// Direct expandEfrag call. The chosen fragment must already be marked done,
    /// as in computeInitialCoords. All source fragments and IDs stay unchanged.
    pub fn expand(
        &self,
        fragments: &[Fragment],
        master: usize,
        non_embedded: &[usize],
        bond_length: f64,
    ) -> Result<Expanded> {
        let mut work = Budget::new(self.work_limit);
        self.validate_fragments(fragments, &mut work)?;
        let mut remaining = Remaining {
            order: non_embedded.to_vec(),
            active: self.remaining(non_embedded)?,
        };
        for fragment in fragments {
            work.fragment(fragment)?;
        }
        work.reserve(non_embedded.len().checked_mul(2).ok_or(Error::Limit)?)?;
        let mut pool = fragments.iter().cloned().map(Some).collect::<Vec<_>>();
        self.expand_in_place(master, &mut pool, &mut remaining, bond_length, &mut work)?;
        let result = Expanded {
            fragments: pool.into_iter().flatten().collect(),
            non_embedded: remaining.values(),
        };
        self.validate_fragments(&result.fragments, &mut work)?;
        Ok(result)
    }
    pub(super) fn complete(
        &self,
        seed: Seeded,
        length: f64,
        work: &mut Budget,
    ) -> Result<Vec<Fragment>> {
        let mut remaining = Remaining {
            active: self.remaining(&seed.non_embedded)?,
            order: seed.non_embedded,
        };
        let mut pool = seed.fragments.into_iter().map(Some).collect::<Vec<_>>();
        let largest = |pool: &[Option<Fragment>], work: &mut Budget| -> Result<Option<usize>> {
            let mut result = None;
            let mut size = 0;
            for (i, f) in pool.iter().enumerate() {
                work.spend(1)?;
                if let Some(f) = f
                    && !f.done
                    && f.atoms.len() > size
                {
                    size = f.atoms.len();
                    result = Some(i);
                }
            }
            Ok(result)
        };
        let mut master = if seed.pre_specified {
            Some(0)
        } else {
            largest(&pool, work)?
        };
        while master.is_some() || !remaining.active.is_empty() {
            work.spend(1)?;
            if master.is_none() {
                let mut minimum = i32::MAX;
                let mut chosen = None;
                let count = u32::try_from(self.graph.atoms.len()).map_err(|_| Error::Limit)?;
                for &id in &remaining.order {
                    work.spend(1)?;
                    if !remaining.active.contains(&id) {
                        continue;
                    }
                    // getNumAtoms() promotes the native multiplication to u32;
                    // assignment back to int retains these low32 bits.
                    let rank = (*at(&self.ranks, id)? as u32)
                        .wrapping_mul(count)
                        .wrapping_add(u32::try_from(id).map_err(|_| Error::Limit)?)
                        as i32;
                    if rank < minimum {
                        minimum = rank;
                        chosen = Some(id);
                    }
                }
                let id = chosen.ok_or(Error::Invalid("no starting rank below MAX_INT"))?;
                remaining.active.remove(&id);
                master = Some(pool.len());
                let fragment = self.attachment.single_atom_with_budget(id, &mut work.0)?;
                work.fragment(&fragment)?;
                pool.push(Some(fragment));
            }
            let index = master.ok_or(Error::Invalid("missing master"))?;
            pool.get_mut(index)
                .and_then(Option::as_mut)
                .ok_or(Error::Invalid("missing master"))?
                .done = true;
            self.expand_in_place(index, &mut pool, &mut remaining, length, work)?;
            master = largest(&pool, work)?;
        }
        let fragments = pool.into_iter().flatten().collect::<Vec<_>>();
        self.validate_fragments(&fragments, work)?;
        Ok(fragments)
    }
}
