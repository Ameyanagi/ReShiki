//! Checked storage operations shared by the reference sorting policies.
use super::{Error, at, at_mut};
pub(super) type Entry = (i32, usize);

pub(super) fn key(values: &[Entry], i: usize) -> Result<i32, Error> {
    Ok(at(values, i)?.0)
}
pub(super) fn swap(values: &mut [Entry], a: usize, b: usize) -> Result<(), Error> {
    let first = *at(values, a)?;
    let second = *at(values, b)?;
    *at_mut(values, a)? = second;
    *at_mut(values, b)? = first;
    Ok(())
}
pub(super) fn insert(values: &mut [Entry], first: usize, last: usize) -> Result<(), Error> {
    for i in first + 1..last {
        let value = *at(values, i)?;
        let mut hole = i;
        while hole > first && value.0 < key(values, hole - 1)? {
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
    right_ties: bool,
) -> Result<(), Error> {
    let top = hole;
    let mut child = 2 * hole + 2;
    while child < len {
        let left = key(values, base + child - 1)?;
        let right = key(values, base + child)?;
        if right < left || !right_ties && right == left {
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
pub(super) fn heap(
    values: &mut [Entry],
    base: usize,
    len: usize,
    right_ties: bool,
) -> Result<(), Error> {
    for parent in (0..len / 2).rev() {
        let value = *at(values, base + parent)?;
        adjust(values, base, len, parent, value, right_ties)?;
    }
    for remaining in (1..len).rev() {
        if !right_ties {
            // libc++ selects the hole from the entire heap, including the
            // last leaf, before moving that leaf's value and sifting it up.
            let top = *at(values, base)?;
            let mut hole = 0;
            loop {
                let mut child = 2 * hole + 1;
                if child > remaining {
                    break;
                }
                if child < remaining && key(values, base + child)? < key(values, base + child + 1)?
                {
                    child += 1;
                }
                *at_mut(values, base + hole)? = *at(values, base + child)?;
                hole = child;
            }
            if hole != remaining {
                let value = *at(values, base + remaining)?;
                while hole > 0 {
                    let parent = (hole - 1) / 2;
                    if key(values, base + parent)? >= value.0 {
                        break;
                    }
                    *at_mut(values, base + hole)? = *at(values, base + parent)?;
                    hole = parent;
                }
                *at_mut(values, base + hole)? = value;
            }
            *at_mut(values, base + remaining)? = top;
            continue;
        }
        let value = *at(values, base + remaining)?;
        *at_mut(values, base + remaining)? = *at(values, base)?;
        adjust(values, base, remaining, 0, value, right_ties)?;
    }
    Ok(())
}
