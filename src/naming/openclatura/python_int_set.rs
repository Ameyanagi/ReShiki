// SPDX-License-Identifier: Python-2.0 AND (MIT OR Apache-2.0)
// Origin: mixed; upstream-derived code with original nazuchem/ReShiki extensions.
//   CPython 3.12, Objects/setobject.c @ 3.12 (exact patch/commit unresolved).
//   ReShiki, src/naming/openclatura/python_int_set.rs @ 5f559fd482e237307a589dfcb7ea333cee0a6e13.
// Copyright (c) 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2010,
// 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022, 2023 Python Software Foundation;
// All Rights Reserved
// Copyright (c) 1995-2001 Corporation for National Research Initiatives; All Rights Reserved
// Copyright (c) 1991 - 1995, Stichting Mathematisch Centrum Amsterdam,
// The Netherlands. All rights reserved.
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: CPython integer-set algorithm adaptation for Python-compatible traversal, plus original Rust collection glue and regression tests.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.
// Unresolved: Header records only CPython 3.12, not a patch/commit. Repository NOTICE pin 3.12.12 / 4a5632fb applies to other files and does not prove this file used that exact source. Replace the unresolved source ref after provenance confirmation.

//! CPython integer-set traversal for upstream rules whose final tie follows a set.
//! Atom IDs have their integer value as their Python hash. This mirrors the
//! insertion, table-copy and difference paths of CPython 3.12 `Objects/setobject.c`.

#[derive(Clone, Copy, Debug, Default)]
enum Slot {
    #[default]
    Empty,
    Deleted,
    Atom(usize),
}

#[derive(Clone, Debug)]
pub struct PythonIntSet {
    slots: Vec<Slot>,
    used: usize,
    fill: usize,
}

impl Default for PythonIntSet {
    // Adapted from Objects/setobject.c (adapted; upstream function mapping not recorded; local fn PythonIntSet::default)
    fn default() -> Self {
        Self {
            slots: vec![Slot::Empty; 8],
            used: 0,
            fill: 0,
        }
    }
}

impl FromIterator<usize> for PythonIntSet {
    // Adapted from Objects/setobject.c (adapted; upstream function mapping not recorded; local fn PythonIntSet::from_iter)
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut result = Self::default();
        for value in iter {
            result.insert(value);
        }
        result
    }
}

impl PythonIntSet {
    // Adapted from Objects/setobject.c (adapted; upstream function mapping not recorded; local fn PythonIntSet::iter)
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.slots.iter().filter_map(|slot| match slot {
            Slot::Atom(value) => Some(*value),
            _ => None,
        })
    }

    // Adapted from Objects/setobject.c::set_lookkey, Objects/setobject.c::set_add_entry (adapted; local fn PythonIntSet::lookup)
    fn lookup(&self, value: usize) -> (usize, bool) {
        let mask = self.slots.len() - 1;
        let mut index = value & mask;
        let mut perturb = value;
        let mut deleted = None;
        loop {
            let probes = if index + 9 <= mask { 9 } else { 0 };
            for offset in 0..=probes {
                let slot = index + offset;
                match self.slots[slot] {
                    Slot::Empty => return (deleted.unwrap_or(slot), false),
                    Slot::Deleted => deleted = Some(slot),
                    Slot::Atom(existing) if existing == value => return (slot, true),
                    Slot::Atom(_) => {}
                }
            }
            perturb >>= 5;
            index = index.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb) & mask;
        }
    }

    // Adapted from Objects/setobject.c::set_add_entry (adapted; local fn PythonIntSet::insert)
    pub fn insert(&mut self, value: usize) {
        let (slot, present) = self.lookup(value);
        if present {
            return;
        }
        let unused = matches!(self.slots[slot], Slot::Empty);
        self.slots[slot] = Slot::Atom(value);
        self.used += 1;
        if unused {
            self.fill += 1;
            if self.fill * 5 >= (self.slots.len() - 1) * 3 {
                self.resize(self.resize_minimum());
            }
        }
    }

    // Adapted from Objects/setobject.c::set_discard_entry (adapted; local fn PythonIntSet::discard)
    pub fn discard(&mut self, value: usize) {
        let (slot, present) = self.lookup(value);
        if present {
            self.slots[slot] = Slot::Deleted;
            self.used -= 1;
        }
    }

    // Adapted from Objects/setobject.c (adapted; upstream function mapping not recorded; local fn PythonIntSet::resize_minimum)
    fn resize_minimum(&self) -> usize {
        self.used * if self.used > 50_000 { 2 } else { 4 }
    }

    // Adapted from Objects/setobject.c::set_table_resize (adapted; local fn PythonIntSet::resize)
    fn resize(&mut self, minimum: usize) {
        let mut size = 8;
        while size <= minimum {
            size *= 2;
        }
        if size == self.slots.len() && self.fill == self.used {
            return;
        }
        let old = std::mem::replace(&mut self.slots, vec![Slot::Empty; size]);
        self.fill = self.used;
        // Existing atoms are distinct, so lookup follows the same probe sequence
        // as CPython's clean reinsertion without needing rich comparisons.
        for slot in old {
            if let Slot::Atom(value) = slot {
                let (index, _) = self.lookup(value);
                self.slots[index] = Slot::Atom(value);
            }
        }
    }

    // Adapted from Objects/setobject.c::set_merge, Objects/setobject.c::set_copy (adapted; local fn PythonIntSet::table_copy)
    fn table_copy(&self) -> Self {
        let mut result = Self::default();
        if self.used * 5 >= 7 * 3 {
            result.resize(self.used * 2);
        }
        if result.slots.len() == self.slots.len() && self.used == self.fill {
            return self.clone();
        }
        for value in self.iter() {
            let (index, _) = result.lookup(value);
            result.slots[index] = Slot::Atom(value);
        }
        result.used = self.used;
        result.fill = self.used;
        result
    }

    // Adapted from Objects/setobject.c::set_difference, Objects/setobject.c::set_difference_update_internal (adapted; local fn PythonIntSet::difference)
    pub fn difference(&self, other: &Self) -> Self {
        if self.used >> 2 > other.used {
            let mut result = self.table_copy();
            for value in other.iter() {
                result.discard(value);
            }
            if result.fill - result.used > (result.slots.len() - 1) / 4 {
                result.resize(result.resize_minimum());
            }
            result
        } else {
            self.iter()
                .filter(|&value| !other.lookup(value).1)
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PythonIntSet;

    // Original nazuchem code (not in CPython): fn small_integer_sets_follow_table_slots
    #[test]
    fn small_integer_sets_follow_table_slots() {
        for (input, expected) in [
            (vec![2, 3, 5, 12], vec![2, 3, 12, 5]),
            (vec![6, 8], vec![8, 6]),
            (vec![1, 9, 17, 25], vec![1, 25, 9, 17]),
            (vec![2, 10, 18, 26, 34, 42], vec![2, 34, 10, 42, 18, 26]),
        ] {
            let set: PythonIntSet = input.into_iter().collect();
            assert_eq!(set.iter().collect::<Vec<_>>(), expected);
        }
    }

    // Original nazuchem code (not in CPython): fn deleting_then_adding_preserves_dummy_probe_order
    #[test]
    fn deleting_then_adding_preserves_dummy_probe_order() {
        let mut set: PythonIntSet = [1, 9, 17, 25].into_iter().collect();
        set.discard(9);
        set.insert(33);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 33, 17, 25]);
    }

    // Original nazuchem code (not in CPython): fn difference_uses_both_fresh_and_copied_tables
    #[test]
    fn difference_uses_both_fresh_and_copied_tables() {
        let set: PythonIntSet = (4..10).collect();
        let removed = [6, 8].into_iter().collect();
        assert_eq!(
            set.difference(&removed).iter().collect::<Vec<_>>(),
            vec![9, 4, 5, 7]
        );
        let set: PythonIntSet = (6..31).collect();
        let removed = [9, 16].into_iter().collect();
        assert_eq!(
            set.difference(&removed).iter().collect::<Vec<_>>(),
            (6..31)
                .filter(|value| ![9, 16].contains(value))
                .collect::<Vec<_>>()
        );
    }
}
