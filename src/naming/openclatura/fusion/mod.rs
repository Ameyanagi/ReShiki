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

pub mod audit;
pub mod charges;
pub mod citation_numbering;
pub mod citation_pi;
pub mod citation_pi_donor;
pub mod component_hydrogen;
pub mod composite_bridges;
pub mod config;
pub mod construction_order;
pub mod cover;
pub mod descriptor;
pub mod entry_geometry;
pub mod exocyclic;
pub mod external_pi_consumption;
pub mod faces;
pub mod indicated_hydrogen;
pub mod layout;
pub mod mancude;
pub mod model;
pub mod numbering;
pub mod planner;
pub mod registry;
pub mod replacement_state;
pub mod rules;
pub mod third_component;
pub mod valence;
pub mod wrappers;
