//! Partition refinement and Hanoi merge sort from RDKit new_canon.h and
//! RDGeneral/hanoiSort.h, Copyright (C) 2014-2025 Greg Landrum/contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Comparison, Ranker, at, put};
use std::cmp::Ordering;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
enum Link {
    Inactive,
    Next(Option<usize>),
}
pub(super) struct Partitions {
    pub order: Vec<usize>,
    pub count: Vec<usize>,
    changed: Vec<bool>,
    next: Vec<Link>,
    active: Option<usize>,
}
impl Partitions {
    pub fn new(n: usize) -> Self {
        let mut count = vec![0; n];
        if let Some(first) = count.first_mut() {
            *first = n;
        }
        Self {
            order: (0..n).collect(),
            count,
            changed: vec![true; n],
            next: vec![Link::Inactive; n],
            active: None,
        }
    }
    pub fn has_ties(&self) -> bool {
        self.count.contains(&0)
    }
    pub fn activate(&mut self, ranker: &mut Ranker<'_>) -> Result<(), String> {
        ranker.work.spend(self.order.len())?;
        self.active = None;
        self.next.fill(Link::Inactive);
        self.changed.fill(true);
        let mut i = 0;
        while i < self.order.len() {
            let atom = *at(&self.order, i)?;
            let count = *at(&self.count, atom)?;
            if count > 1 {
                put(&mut self.next, atom, Link::Next(self.active))?;
                self.active = Some(atom);
                i += count;
            } else {
                i += 1;
            }
        }
        Ok(())
    }
    fn touch(&mut self, touched: BTreeSet<usize>) -> Result<(), String> {
        for position in touched {
            let atom = *at(&self.order, position)?;
            if *at(&self.count, atom)? > 1 && matches!(at(&self.next, atom)?, Link::Inactive) {
                put(&mut self.next, atom, Link::Next(self.active))?;
                self.active = Some(atom);
            }
        }
        Ok(())
    }
    pub fn refine(
        &mut self,
        ranker: &mut Ranker<'_>,
        comparison: Comparison,
    ) -> Result<(), String> {
        while let Some(partition) = self.active {
            ranker.work.spend(1)?;
            self.active = match *at(&self.next, partition)? {
                Link::Next(next) => next,
                Link::Inactive => return Err("Inactive canonical partition".into()),
            };
            put(&mut self.next, partition, Link::Inactive)?;
            let len = *at(&self.count, partition)?;
            let offset = *at(&ranker.classes, partition)?;
            let end = offset
                .checked_add(len)
                .ok_or("Canonical partition overflow")?;
            let mut items = self
                .order
                .get(offset..end)
                .ok_or("Invalid canonical partition")?
                .to_vec();
            if items.is_empty() {
                return Err("Empty canonical partition".into());
            }
            self.sort(&mut items, ranker, comparison)?;
            self.order
                .get_mut(offset..end)
                .ok_or("Invalid canonical partition")?
                .copy_from_slice(&items);
            ranker.work.spend(items.len())?;
            for &atom in &items {
                put(&mut self.changed, atom, false)?;
            }
            let first = *items.first().ok_or("Empty canonical partition")?;
            let skip = *at(&self.count, first)?;
            let mut class = offset;
            for (i, &atom) in items.iter().enumerate().skip(skip) {
                if *at(&self.count, atom)? != 0 {
                    class = offset + i;
                }
                put(&mut ranker.classes, atom, class)?;
                ranker.work.spend(at(&ranker.neighbors, atom)?.len())?;
                for &other in at(&ranker.neighbors, atom)? {
                    put(&mut self.changed, other, true)?;
                }
            }
            let mut touched = BTreeSet::new();
            for &atom in items.iter().skip(skip) {
                for &other in at(&ranker.neighbors, atom)? {
                    touched.insert(*at(&ranker.classes, other)?);
                }
            }
            self.touch(touched)?;
        }
        Ok(())
    }
    // Same left-then-right recursive split/merge order as Hanoi sort, using
    // explicit frames so depth never uses the native call stack. Equal groups
    // retain encounter order, including comparisons skipped by `changed`.
    fn sort(
        &mut self,
        items: &mut [usize],
        ranker: &mut Ranker<'_>,
        comparison: Comparison,
    ) -> Result<(), String> {
        let mut stack = vec![(0usize, items.len(), false)];
        while let Some((start, len, merge)) = stack.pop() {
            ranker.work.spend(1)?;
            if len == 1 {
                put(&mut self.count, *at(items, start)?, 1)?;
                continue;
            }
            if len == 0 {
                return Err("Empty canonical sort".into());
            }
            if !merge {
                let half = len / 2;
                stack.push((start, len, true));
                stack.push((start + half, len - half, false));
                stack.push((start, half, false));
                continue;
            }
            let middle = start + len / 2;
            let end = start + len;
            ranker.work.spend(len)?;
            let left = items
                .get(start..middle)
                .ok_or("Invalid canonical sort range")?
                .to_vec();
            let right = items
                .get(middle..end)
                .ok_or("Invalid canonical sort range")?
                .to_vec();
            let mut result = Vec::with_capacity(len);
            let (mut a, mut b) = (0, 0);
            while a < left.len() && b < right.len() {
                let (x, y) = (*at(&left, a)?, *at(&right, b)?);
                let order = if *at(&self.changed, x)? || *at(&self.changed, y)? {
                    ranker.compare(x, y, comparison)?
                } else {
                    Ordering::Equal
                };
                let (nx, ny) = (*at(&self.count, x)?, *at(&self.count, y)?);
                if nx == 0 || ny == 0 {
                    return Err("Invalid canonical group size".into());
                }
                if order == Ordering::Equal {
                    put(&mut self.count, x, nx + ny)?;
                    put(&mut self.count, y, 0)?;
                }
                if order != Ordering::Greater {
                    result.extend_from_slice(left.get(a..a + nx).ok_or("Invalid canonical group")?);
                    a += nx;
                }
                if order != Ordering::Less {
                    result
                        .extend_from_slice(right.get(b..b + ny).ok_or("Invalid canonical group")?);
                    b += ny;
                }
            }
            result.extend_from_slice(left.get(a..).ok_or("Invalid canonical sort suffix")?);
            result.extend_from_slice(right.get(b..).ok_or("Invalid canonical sort suffix")?);
            if result.len() != len {
                return Err("Canonical sort changed partition size".into());
            }
            items
                .get_mut(start..end)
                .ok_or("Invalid canonical sort range")?
                .copy_from_slice(&result);
        }
        Ok(())
    }
    pub fn break_ties(&mut self, ranker: &mut Ranker<'_>) -> Result<(), String> {
        let mut i = 0;
        while i < self.order.len() {
            ranker.work.spend(1)?;
            let partition = *at(&self.order, i)?;
            let old = *at(&ranker.classes, partition)?;
            while *at(&self.count, partition)? > 1 {
                ranker.work.spend(1)?;
                let len = *at(&self.count, partition)?;
                let offset = *at(&ranker.classes, partition)? + len - 1;
                let atom = *at(&self.order, offset)?;
                put(&mut ranker.classes, atom, offset)?;
                put(&mut self.count, partition, len - 1)?;
                put(&mut self.count, atom, 1)?;
                let mut touched = BTreeSet::new();
                ranker.work.spend(at(&ranker.neighbors, atom)?.len())?;
                for &other in at(&ranker.neighbors, atom)? {
                    touched.insert(*at(&ranker.classes, other)?);
                    put(&mut self.changed, other, true)?;
                }
                self.touch(touched)?;
                self.refine(ranker, Comparison::Normal)?;
            }
            if *at(&ranker.classes, partition)? == old {
                i += 1;
            }
        }
        Ok(())
    }
}
