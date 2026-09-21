//! Adapted from libc++ __algorithm/sort.h (LLVM Project).
//! Apache-2.0 WITH LLVM-exception; see licenses/stdlib/llvm-LICENSE.
//! Changes: integer keys, checked indexing and an explicit range stack.
use super::{
    Error, at, at_mut,
    sort_common::{Entry, heap, insert, key, swap},
};

fn sort3(v: &mut [Entry], x: usize, y: usize, z: usize) -> Result<(), Error> {
    if key(v, y)? >= key(v, x)? {
        if key(v, z)? < key(v, y)? {
            swap(v, y, z)?;
            if key(v, y)? < key(v, x)? {
                swap(v, x, y)?;
            }
        }
    } else if key(v, z)? < key(v, y)? {
        swap(v, x, z)?;
    } else {
        swap(v, x, y)?;
        if key(v, z)? < key(v, y)? {
            swap(v, y, z)?;
        }
    }
    Ok(())
}

fn incomplete(v: &mut [Entry], first: usize, last: usize) -> Result<bool, Error> {
    if last - first <= 5 {
        insert(v, first, last)?;
        return Ok(true);
    }
    sort3(v, first, first + 1, first + 2)?;
    let mut changed = 0;
    for i in first + 3..last {
        if key(v, i)? < key(v, i - 1)? {
            let value = *at(v, i)?;
            let mut hole = i;
            while hole > first && value.0 < key(v, hole - 1)? {
                *at_mut(v, hole)? = *at(v, hole - 1)?;
                hole -= 1;
            }
            *at_mut(v, hole)? = value;
            changed += 1;
            if changed == 8 {
                return Ok(i + 1 == last);
            }
        }
    }
    Ok(true)
}

fn partition(
    v: &mut [Entry],
    first: usize,
    last: usize,
    equals_left: bool,
) -> Result<(usize, bool), Error> {
    let pivot = *at(v, first)?;
    let less = |key: i32| {
        if equals_left {
            key <= pivot.0
        } else {
            key < pivot.0
        }
    };
    let mut a = first + 1;
    while a < last && less(key(v, a)?) {
        a += 1;
    }
    let mut b = last;
    while b > a && !less(key(v, b - 1)?) {
        b -= 1;
    }
    if b > a {
        b -= 1;
    }
    let ordered = a >= b;
    while a < b {
        swap(v, a, b)?;
        a += 1;
        while a < last && less(key(v, a)?) {
            a += 1;
        }
        b = b.checked_sub(1).ok_or(Error::Limit)?;
        while b > first && !less(key(v, b)?) {
            b -= 1;
        }
    }
    let position = a.checked_sub(1).ok_or(Error::Limit)?;
    if first != position {
        *at_mut(v, first)? = *at(v, position)?;
    }
    *at_mut(v, position)? = pivot;
    Ok((position, ordered))
}

pub(super) fn sort(v: &mut [Entry]) -> Result<(), Error> {
    if v.len() < 2 {
        return Ok(());
    }
    let mut pending = vec![(0, v.len(), 2 * v.len().ilog2(), true)];
    while let Some((mut first, mut last, mut depth, leftmost)) = pending.pop() {
        loop {
            let len = last - first;
            if len < 24 {
                insert(v, first, last)?;
                break;
            }
            if depth == 0 {
                heap(v, first, len, false)?;
                break;
            }
            depth -= 1;
            let middle = first + len / 2;
            if len > 128 {
                sort3(v, first, middle, last - 1)?;
                sort3(v, first + 1, middle - 1, last - 2)?;
                sort3(v, first + 2, middle + 1, last - 3)?;
                sort3(v, middle - 1, middle, middle + 1)?;
                swap(v, first, middle)?;
            } else {
                sort3(v, middle, first, last - 1)?;
            }
            if !leftmost && first > 0 && key(v, first - 1)? >= key(v, first)? {
                first = partition(v, first, last, true)?.0 + 1;
                continue;
            }
            let (pivot, ordered) = partition(v, first, last, false)?;
            if ordered {
                let left_done = incomplete(v, first, pivot)?;
                let right_done = incomplete(v, pivot + 1, last)?;
                if left_done && right_done {
                    break;
                }
                if right_done {
                    last = pivot;
                    continue;
                }
                if left_done {
                    first = pivot + 1;
                    continue;
                }
            }
            pending.push((pivot + 1, last, depth, false));
            last = pivot;
        }
    }
    Ok(())
}
