use super::*;
use std::collections::{BTreeMap, HashSet};

impl Input<'_> {
    fn systems(&self, work: &mut Budget) -> Result<Vec<Vec<usize>>> {
        // Original makeRingNeighborMap receives atom rings here, so a shared
        // atom (including a spiro center) joins the system. Ascending neighbor
        // order and recursive DFS order are preserved with explicit frames.
        let n = self.cache.atoms.len();
        let mut memberships: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, ring) in self.cache.atoms.iter().enumerate() {
            for &atom in ring {
                work.spend(1)?;
                memberships.entry(atom).or_default().push(i);
            }
        }
        let mut adjacent = vec![BTreeSet::new(); n];
        let mut stored = 0usize;
        for member in memberships.values() {
            for (i, &a) in member.iter().enumerate() {
                for &b in member.iter().skip(i + 1) {
                    work.spend(1)?;
                    if a != b && adjacent.get_mut(a).ok_or(Error::Limit)?.insert(b) {
                        adjacent.get_mut(b).ok_or(Error::Limit)?.insert(a);
                        stored = stored.checked_add(2).ok_or(Error::Limit)?;
                        if stored > MAX_STORAGE {
                            return Err(Error::Limit);
                        }
                    }
                }
            }
        }
        let adjacent = adjacent
            .into_iter()
            .map(|a| a.into_iter().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let mut done = vec![false; n];
        let mut systems = Vec::new();
        for first in 0..n {
            if *at(&done, first)? {
                continue;
            }
            let mut system = vec![first];
            *done.get_mut(first).ok_or(Error::Limit)? = true;
            let mut stack = vec![(first, 0usize)];
            while let Some((id, next)) = stack.last_mut() {
                work.spend(1)?;
                let neighbor = at(&adjacent, *id)?.get(*next).copied();
                *next += 1;
                if let Some(neighbor) = neighbor {
                    if !*at(&done, neighbor)? {
                        *done.get_mut(neighbor).ok_or(Error::Limit)? = true;
                        system.push(neighbor);
                        stack.push((neighbor, 0));
                    }
                } else {
                    stack.pop();
                }
            }
            systems.push(system);
        }
        Ok(systems)
    }
    pub fn seed(&self, coordinates: Option<&Coordinates>, options: Options) -> Result<Seeded> {
        self.seed_with_work(coordinates, options, &mut Budget::new(self.work_limit))
    }
    fn seed_with_work(
        &self,
        coordinates: Option<&Coordinates>,
        options: Options,
        work: &mut Budget,
    ) -> Result<Seeded> {
        let mut fragments = Vec::new();
        if let Some(coordinates) = coordinates {
            if coordinates.len() > self.graph.atoms.len() {
                return Err(Error::Limit);
            }
            for (&id, p) in coordinates {
                work.spend(1)?;
                at(&self.graph.atoms, id)?;
                if !p.x.is_finite() || !p.y.is_finite() {
                    return Err(geometry::Error::NonFinite.into());
                }
            }
            if coordinates.len() > 1 {
                let fragment = self
                    .seeds
                    .coordinates_with_budget(coordinates, &mut work.0)?;
                work.fragment(&fragment)?;
                fragments.push(fragment);
            }
        }
        let pre_specified = coordinates.is_some_and(|c| c.len() > 1);
        for system in self.systems(work)? {
            let mut mapped = HashSet::new();
            if let Some(coordinates) = coordinates {
                for &id in &system {
                    for &atom in at(&self.cache.atoms, id)? {
                        work.spend(1)?;
                        if coordinates.contains_key(&atom) {
                            mapped.insert(atom);
                        }
                    }
                }
            }
            let fragment = if options.use_ring_templates && mapped.len() < 2 {
                self.templates
                    .embed_with_budget(&system, options.bond_length, &mut work.0)?
                    .fragment
            } else {
                work.spend(self.graph.atoms.len() + self.graph.bonds.len())?;
                rings::Input::new(self.graph, self.metadata, self.cache, &system)?
                    .with_work_limit(work.0)
                    .embed_with_budget(options.bond_length, &mut work.0)?
            };
            work.spend(fragment.atoms.len())?;
            work.fragment(&fragment)?;
            let fragment = self.attachment.setup_with_budget(&fragment, &mut work.0)?;
            work.fragment(&fragment)?;
            fragments.push(fragment);
        }
        for id in 0..self.graph.atoms.len() {
            work.spend(1)?;
            if let Some(fragment) = self.seeds.coordination_with_budget(
                id,
                &self.ranks,
                &options.ideal_lengths,
                &mut work.0,
            )? {
                work.fragment(&fragment)?;
                fragments.push(fragment);
            }
        }
        let mut ring_bonds = HashSet::new();
        for ring in &self.cache.atoms {
            let Some(&last) = ring.last() else {
                return Err(Error::Invalid("empty cached ring"));
            };
            let mut previous = last;
            for &atom in ring {
                work.spend(1)?;
                ring_bonds.insert((previous.min(atom), previous.max(atom)));
                previous = atom;
            }
        }
        for (id, bond) in self.graph.bonds.iter().enumerate() {
            work.spend(1)?;
            let meta = at(&self.metadata.bonds, id)?;
            if bond.order == 2
                && meta.stereo > 1
                && !ring_bonds.contains(&(bond.a.min(bond.b), bond.a.max(bond.b)))
                && meta.stereo_atoms.len() == 2
            {
                let fragment =
                    self.seeds
                        .cis_trans_with_budget(id, options.bond_length, &mut work.0)?;
                work.fragment(&fragment)?;
                let fragment = self.attachment.setup_with_budget(&fragment, &mut work.0)?;
                work.fragment(&fragment)?;
                fragments.push(fragment);
            }
        }
        self.validate_fragments(&fragments, work)?;
        let mut embedded = vec![false; self.graph.atoms.len()];
        for fragment in &fragments {
            for &id in fragment.atoms.keys() {
                work.spend(1)?;
                *embedded.get_mut(id).ok_or(Error::Invalid("seed atom"))? = true;
            }
        }
        let non_embedded = embedded
            .into_iter()
            .enumerate()
            .filter_map(|(i, p)| (!p).then_some(i))
            .collect();
        Ok(Seeded {
            fragments,
            non_embedded,
            pre_specified,
        })
    }
    pub fn initial(
        &self,
        coordinates: Option<&Coordinates>,
        options: Options,
    ) -> Result<Vec<Fragment>> {
        let mut work = Budget::new(self.work_limit);
        self.initial_with_budget(coordinates, options, &mut work)
    }
    pub(super) fn initial_with_budget(
        &self,
        coordinates: Option<&Coordinates>,
        options: Options,
        work: &mut Budget,
    ) -> Result<Vec<Fragment>> {
        let seed = self.seed_with_work(coordinates, options, work)?;
        self.complete(seed, options.bond_length, work)
    }
}
