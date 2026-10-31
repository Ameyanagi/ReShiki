//! Audited graph decomposition, numbering and bond-state proofs for fused parents.
use std::{cell::RefCell, collections::VecDeque};

struct ThreadLocalCache<K, V> {
    entries: RefCell<VecDeque<(K, V)>>,
}

impl<K, V> ThreadLocalCache<K, V> {
    const fn new() -> Self {
        Self {
            entries: RefCell::new(VecDeque::new()),
        }
    }

    fn get(&self, mut matches: impl FnMut(&K) -> bool) -> Option<V>
    where
        V: Clone,
    {
        let mut entries = self.entries.borrow_mut();
        let index = entries.iter().position(|(key, _)| matches(key))?;
        let entry = entries.remove(index)?;
        let value = entry.1.clone();
        entries.push_back(entry);
        Some(value)
    }

    fn insert(&self, key: K, value: V) {
        let mut entries = self.entries.borrow_mut();
        if entries.len() == 128 {
            entries.pop_front();
        }
        entries.push_back((key, value));
    }
}

mod audit;
mod charges;
mod citation_numbering;
mod citation_pi;
mod citation_pi_donor;
mod component_hydrogen;
mod composite_bridges;
mod config;
mod construction_order;
mod cover;
pub(super) mod descriptor;
mod entry_geometry;
pub(super) mod exocyclic;
mod external_pi_consumption;
mod faces;
mod indicated_hydrogen;
mod layout;
pub(super) mod mancude;
pub(super) mod model;
pub(super) mod numbering;
pub(super) mod planner;
pub(super) mod registry;
pub(super) mod replacement_state;
mod rules;
pub(super) mod third_component;
mod valence;
pub(super) mod wrappers;
