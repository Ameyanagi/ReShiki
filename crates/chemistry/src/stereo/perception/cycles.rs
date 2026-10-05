//! Ring stereo relationships from Chirality.cpp and bridgehead queries from
//! QueryOps.cpp, Copyright (C) 2003-2024 Greg Landrum and RDKit contributors.
use super::*;
use std::collections::VecDeque;
impl Context {
    pub(super) fn index_rings(&mut self, work: &mut Work) -> Result<(), String> {
        let (n, e) = (self.state.graph.atoms.len(), self.state.graph.bonds.len());
        if self.state.rings.kind == RingKind::None && !self.state.rings.atoms.is_empty() {
            return Err("Uninitialized stereo ring cache contains cycles".into());
        }
        self.atom_rings = vec![Vec::new(); n];
        self.bond_rings = vec![Vec::new(); e];
        self.minimum = vec![usize::MAX; e];
        self.bridgeheads = vec![None; n];
        self.ring_bonds.clear();
        let mut storage = 0usize;
        for (id, ring) in self.state.rings.atoms.iter().enumerate() {
            storage = storage
                .checked_add(ring.len())
                .ok_or("Stereo ring storage overflow")?;
            if storage > 2_000_000
                || ring.len() < 3
                || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err("Invalid stereo ring cache".into());
            }
            work.spend(ring.len())?;
            let mut bonds = Vec::new();
            for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                let bond = *self
                    .pairs
                    .get(&(a.min(b), a.max(b)))
                    .ok_or("Missing stereo ring edge")?;
                self.atom_rings
                    .get_mut(a)
                    .ok_or("Missing stereo ring atom")?
                    .push(id);
                self.bond_rings
                    .get_mut(bond)
                    .ok_or("Missing stereo ring bond")?
                    .push(id);
                let minimum = self
                    .minimum
                    .get_mut(bond)
                    .ok_or("Missing minimum ring size")?;
                *minimum = (*minimum).min(ring.len());
                bonds.push(bond);
            }
            self.ring_bonds.push(bonds);
        }
        Ok(())
    }
    pub(super) fn in_three_ring(&self, atom: usize, work: &mut Work) -> Result<bool, String> {
        for &ring in at(&self.atom_rings, atom)? {
            work.spend(1)?;
            if at(&self.state.rings.atoms, ring)?.len() == 3 {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn bridgehead(&mut self, atom: usize, work: &mut Work) -> Result<bool, String> {
        if let Some(value) = *at(&self.bridgeheads, atom)? {
            return Ok(value);
        }
        let result = self.bridgehead_uncached(atom, work)?;
        put(&mut self.bridgeheads, atom, Some(result))?;
        Ok(result)
    }
    fn bridgehead_uncached(&self, atom: usize, work: &mut Work) -> Result<bool, String> {
        if at(&self.neighbors, atom)?.len() < 3 {
            return Ok(false);
        }
        let mut count = 0;
        for &b in at(&self.neighbors, atom)? {
            work.spend(1)?;
            if !at(&self.bond_rings, b)?.is_empty() {
                count += 1;
            }
        }
        if count < 3 {
            return Ok(false);
        }
        let mut overlaps = HashSet::new();
        for &i in at(&self.atom_rings, atom)? {
            work.spend(at(&self.ring_bonds, i)?.len())?;
            let bonds = at(&self.ring_bonds, i)?
                .iter()
                .copied()
                .collect::<HashSet<_>>();
            for &j in at(&self.atom_rings, atom)? {
                work.spend(1)?;
                if j <= i {
                    continue;
                }
                let mut overlap = 0;
                for b in at(&self.ring_bonds, j)? {
                    work.spend(1)?;
                    if bonds.contains(b) {
                        overlap += 1;
                    }
                    if overlap >= 2 {
                        overlaps.insert(i);
                        overlaps.insert(j);
                        break;
                    }
                }
            }
            if !overlaps.contains(&i) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn candidate(&mut self, atom: usize, work: &mut Work) -> Result<bool, String> {
        if let Some(value) = at(&self.state.properties.atoms, atom)?.ring_candidate {
            return Ok(value);
        }
        let mut result = false;
        if !at(&self.atom_rings, atom)?.is_empty() {
            if at(&self.state.graph.atoms, atom)?.atomic_number == 7
                && at(&self.neighbors, atom)?.len() + self.hydrogens(atom)? == 3
                && !self.in_three_ring(atom, work)?
                && !self.bridgehead(atom, work)?
            {
                return Ok(false);
            }
            let mut nonring = Vec::new();
            let mut ring_count = 0;
            let mut ranks = HashSet::new();
            for &b in at(&self.neighbors, atom)? {
                work.spend(1)?;
                let other = self.other(b, atom)?;
                if at(&self.bond_rings, b)?.is_empty() {
                    nonring.push(other);
                } else {
                    ring_count += 1;
                    ranks.insert(*at(&self.ranks, other)?);
                }
            }
            result = match nonring.len() {
                2 => {
                    at(&self.ranks, *at(&nonring, 0)?)? != at(&self.ranks, *at(&nonring, 1)?)?
                        && ring_count != ranks.len()
                }
                1 => ring_count > ranks.len(),
                0 => (ring_count == 4 && ranks.len() == 3) || (ring_count == 3 && ranks.len() == 2),
                _ => false,
            };
        }
        self.state
            .properties
            .atoms
            .get_mut(atom)
            .ok_or("Missing ring stereo properties")?
            .ring_candidate = Some(result);
        Ok(result)
    }
    fn reserve_members(&mut self, amount: usize) -> Result<(), String> {
        self.member_storage = self
            .member_storage
            .checked_add(amount)
            .ok_or("Stereo membership overflow")?;
        if self.member_storage > 2_000_000 {
            return Err("Stereo membership storage limit exceeded".into());
        }
        Ok(())
    }
    fn push_member(&mut self, atom: usize, entry: i32) -> Result<(), String> {
        self.reserve_members(1)?;
        self.state
            .properties
            .atoms
            .get_mut(atom)
            .ok_or("Missing ring stereo properties")?
            .ring_members
            .get_or_insert_with(Vec::new)
            .push(entry);
        self.state
            .metadata
            .atoms
            .get_mut(atom)
            .ok_or("Missing ring stereo atom")?
            .ring_stereo = true;
        Ok(())
    }
    pub(super) fn ring_special_cases(&mut self, work: &mut Work) -> Result<Vec<bool>, String> {
        if self.state.rings.kind != RingKind::Symmetric {
            let rings = rings::perceive(&self.state.graph, rings::Options::default())
                .map_err(|e| e.to_string())?;
            self.state.rings = RingCache {
                kind: RingKind::Symmetric,
                atoms: rings.atoms,
            };
            self.index_rings(work)?;
        }
        let n = self.state.graph.atoms.len();
        let mut special = vec![false; n];
        let mut seen = vec![false; n];
        let mut used = vec![false; n];
        let mut bonds_seen = vec![false; self.state.graph.bonds.len()];
        for root in 0..n {
            work.spend(1)?;
            let tag = at(&self.state.metadata.atoms, root)?.chiral_tag;
            if *at(&seen, root)?
                || tag == 0
                || at(&self.state.properties.atoms, root)?.cip_code.is_some()
                || at(&self.atom_rings, root)?.is_empty()
                || !self.candidate(root, work)?
            {
                continue;
            }
            let mut queue = VecDeque::new();
            for &b in at(&self.neighbors, root)? {
                work.spend(1)?;
                if !*at(&bonds_seen, b)? {
                    put(&mut bonds_seen, b, true)?;
                    if !at(&self.bond_rings, b)?.is_empty() {
                        let other = self.other(b, root)?;
                        if !*at(&seen, other)? {
                            queue.push_back(other);
                            put(&mut used, other, true)?;
                        }
                    }
                }
            }
            let mut members = if queue.is_empty() {
                Vec::new()
            } else {
                at(&self.state.properties.atoms, root)?
                    .ring_members
                    .clone()
                    .unwrap_or_default()
            };
            self.reserve_members(members.len())?;
            while let Some(atom) = queue.pop_front() {
                work.spend(1)?;
                put(&mut seen, atom, true)?;
                let other_tag = at(&self.state.metadata.atoms, atom)?.chiral_tag;
                if other_tag != 0
                    && at(&self.state.properties.atoms, atom)?.cip_code.is_none()
                    && self.candidate(atom, work)?
                {
                    let sign = if tag == other_tag { 1 } else { -1 };
                    self.reserve_members(1)?;
                    members.push(sign * (atom as i32 + 1));
                    self.push_member(atom, sign * (root as i32 + 1))?;
                    put(&mut special, atom, true)?;
                    put(&mut special, root, true)?;
                }
                for &b in at(&self.neighbors, atom)? {
                    work.spend(1)?;
                    if !*at(&bonds_seen, b)? {
                        put(&mut bonds_seen, b, true)?;
                        if !at(&self.bond_rings, b)?.is_empty() {
                            let other = self.other(b, atom)?;
                            if !*at(&seen, other)? && !*at(&used, other)? {
                                queue.push_back(other);
                                put(&mut used, other, true)?;
                            }
                        }
                    }
                }
            }
            if members.is_empty() {
                put(&mut special, root, false)?;
            } else {
                self.state
                    .properties
                    .atoms
                    .get_mut(root)
                    .ok_or("Missing ring stereo properties")?
                    .ring_members = Some(members.clone());
                self.state
                    .metadata
                    .atoms
                    .get_mut(root)
                    .ok_or("Missing ring stereo atom")?
                    .ring_stereo = true;
                for (i, &first) in members.iter().enumerate() {
                    let atom = first.unsigned_abs() as usize - 1;
                    for &second in members.iter().skip(i + 1) {
                        work.spend(1)?;
                        let other = second.unsigned_abs() as usize - 1;
                        let sign = if (first < 0) == (second < 0) { 1 } else { -1 };
                        self.push_member(atom, sign * (other as i32 + 1))?;
                        self.push_member(other, sign * (atom as i32 + 1))?;
                    }
                }
            }
            put(&mut seen, root, true)?;
        }
        Ok(special)
    }
}
