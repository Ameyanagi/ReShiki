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
    fn default() -> Self {
        Self {
            slots: vec![Slot::Empty; 8],
            used: 0,
            fill: 0,
        }
    }
}

impl FromIterator<usize> for PythonIntSet {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut result = Self::default();
        for value in iter {
            result.insert(value);
        }
        result
    }
}

impl PythonIntSet {
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.slots.iter().filter_map(|slot| match slot {
            Slot::Atom(value) => Some(*value),
            _ => None,
        })
    }

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

    pub fn discard(&mut self, value: usize) {
        let (slot, present) = self.lookup(value);
        if present {
            self.slots[slot] = Slot::Deleted;
            self.used -= 1;
        }
    }

    fn resize_minimum(&self) -> usize {
        self.used * if self.used > 50_000 { 2 } else { 4 }
    }

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

    #[test]
    fn deleting_then_adding_preserves_dummy_probe_order() {
        let mut set: PythonIntSet = [1, 9, 17, 25].into_iter().collect();
        set.discard(9);
        set.insert(33);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 33, 17, 25]);
    }

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
