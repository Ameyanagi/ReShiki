//! Preserve equal-priority traversal order in the pinned Linux reference.
//!
//! Median-of-three introsort, a 16-element insertion cutoff, and a heap fallback
//! reproduce its comparison policy. An explicit range stack bounds call depth.
use super::{Error, at, at_mut};
type Entry = (i32, usize);

fn key(values: &[Entry], i: usize) -> Result<i32, Error> {
    Ok(at(values, i)?.0)
}
fn swap(values: &mut [Entry], a: usize, b: usize) -> Result<(), Error> {
    let first = *at(values, a)?;
    let second = *at(values, b)?;
    *at_mut(values, a)? = second;
    *at_mut(values, b)? = first;
    Ok(())
}

pub(super) fn sort(values: &mut [Entry]) -> Result<(), Error> {
    if values.len() < 2 {
        return Ok(());
    }
    let depth = 2 * values.len().ilog2();
    let mut pending = vec![(0, values.len(), depth)];
    while let Some((first, last, depth)) = pending.pop() {
        if last - first <= 16 {
            continue;
        }
        if depth == 0 {
            heap(values, first, last - first)?;
            continue;
        }
        let a = first + 1;
        let b = first + (last - first) / 2;
        let c = last - 1;
        let median = if key(values, a)? < key(values, b)? {
            if key(values, b)? < key(values, c)? {
                b
            } else if key(values, a)? < key(values, c)? {
                c
            } else {
                a
            }
        } else if key(values, a)? < key(values, c)? {
            a
        } else if key(values, b)? < key(values, c)? {
            c
        } else {
            b
        };
        swap(values, first, median)?;
        let pivot = key(values, first)?;
        let (mut left, mut right) = (first + 1, last);
        loop {
            while key(values, left)? < pivot {
                left += 1;
            }
            right = right.checked_sub(1).ok_or(Error::Limit)?;
            while pivot < key(values, right)? {
                right = right.checked_sub(1).ok_or(Error::Limit)?;
            }
            if left >= right {
                break;
            }
            swap(values, left, right)?;
            left += 1;
        }
        if left <= first || left >= last {
            return Err(Error::Limit);
        }
        pending.push((first, left, depth - 1));
        pending.push((left, last, depth - 1));
    }
    // The partitions are globally ordered; insertion finishes their short
    // runs without moving equal keys past each other.
    for i in 1..values.len() {
        let value = *at(values, i)?;
        let mut hole = i;
        while hole > 0 && value.0 < key(values, hole - 1)? {
            *at_mut(values, hole)? = *at(values, hole - 1)?;
            hole -= 1;
        }
        *at_mut(values, hole)? = value;
    }
    Ok(())
}

fn adjust(
    values: &mut [Entry],
    base: usize,
    len: usize,
    mut hole: usize,
    value: Entry,
) -> Result<(), Error> {
    let top = hole;
    let mut child = 2 * hole + 2;
    while child < len {
        // The right child wins equal keys in the reference's heap fallback.
        if key(values, base + child)? < key(values, base + child - 1)? {
            child -= 1;
        }
        *at_mut(values, base + hole)? = *at(values, base + child)?;
        hole = child;
        child = 2 * hole + 2;
    }
    if child == len {
        *at_mut(values, base + hole)? = *at(values, base + child - 1)?;
        hole = child - 1;
    }
    while hole > top {
        let parent = (hole - 1) / 2;
        if key(values, base + parent)? >= value.0 {
            break;
        }
        *at_mut(values, base + hole)? = *at(values, base + parent)?;
        hole = parent;
    }
    *at_mut(values, base + hole)? = value;
    Ok(())
}
pub(super) fn heap(values: &mut [Entry], base: usize, len: usize) -> Result<(), Error> {
    for parent in (0..len / 2).rev() {
        let value = *at(values, base + parent)?;
        adjust(values, base, len, parent, value)?;
    }
    for remaining in (1..len).rev() {
        let value = *at(values, base + remaining)?;
        *at_mut(values, base + remaining)? = *at(values, base)?;
        adjust(values, base, remaining, 0, value)?;
    }
    Ok(())
}
