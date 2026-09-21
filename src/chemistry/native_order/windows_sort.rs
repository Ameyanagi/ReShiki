//! Adapted from Microsoft's STL algorithm header, MSVC 14.43.
//! Copyright (c) Microsoft Corporation.
//! Apache-2.0 WITH LLVM-exception; see licenses/stdlib/msvc-LICENSE.
//! Changes: integer keys, checked indexing and an explicit range stack.
use super::{
    Error,
    sort_common::{Entry, heap, insert, key, swap},
};

fn sort3(v: &mut [Entry], a: usize, b: usize, c: usize) -> Result<(), Error> {
    if key(v, b)? < key(v, a)? {
        swap(v, a, b)?;
    }
    if key(v, c)? < key(v, b)? {
        swap(v, b, c)?;
        if key(v, b)? < key(v, a)? {
            swap(v, a, b)?;
        }
    }
    Ok(())
}
fn partition(v: &mut [Entry], first: usize, last: usize) -> Result<(usize, usize), Error> {
    let middle = first + (last - first) / 2;
    let end = last - 1;
    if end - first > 40 {
        let step = (last - first) / 8;
        sort3(v, first, first + step, first + 2 * step)?;
        sort3(v, middle - step, middle, middle + step)?;
        sort3(v, end - 2 * step, end - step, end)?;
        sort3(v, first + step, middle, end - step)?;
    } else {
        sort3(v, first, middle, end)?;
    }
    let (mut p, mut q) = (middle, middle + 1);
    while first < p && key(v, p - 1)? == key(v, p)? {
        p -= 1;
    }
    while q < last && key(v, q)? == key(v, p)? {
        q += 1;
    }
    let (mut a, mut b) = (q, p);
    loop {
        while a < last {
            if key(v, a)? < key(v, p)? {
                break;
            }
            if key(v, a)? == key(v, p)? {
                if q != a {
                    swap(v, q, a)?;
                }
                q += 1;
            }
            a += 1;
        }
        while first < b {
            let previous = b - 1;
            if key(v, previous)? > key(v, p)? {
                break;
            }
            if key(v, previous)? == key(v, p)? {
                p = p.checked_sub(1).ok_or(Error::Limit)?;
                if p != previous {
                    swap(v, p, previous)?;
                }
            }
            b -= 1;
        }
        if b == first && a == last {
            return Ok((p, q));
        }
        if b == first {
            if q != a {
                swap(v, p, q)?;
            }
            q += 1;
            swap(v, p, a)?;
            p += 1;
            a += 1;
        } else if a == last {
            b -= 1;
            p = p.checked_sub(1).ok_or(Error::Limit)?;
            if b != p {
                swap(v, b, p)?;
            }
            q = q.checked_sub(1).ok_or(Error::Limit)?;
            swap(v, p, q)?;
        } else {
            b -= 1;
            swap(v, a, b)?;
            a += 1;
        }
    }
}

pub(super) fn sort(v: &mut [Entry]) -> Result<(), Error> {
    let mut pending = vec![(0, v.len(), v.len())];
    while let Some((first, last, budget)) = pending.pop() {
        if last - first <= 32 {
            insert(v, first, last)?;
            continue;
        }
        if budget == 0 {
            heap(v, first, last - first, true)?;
            continue;
        }
        let (a, b) = partition(v, first, last)?;
        let budget = budget / 2 + budget / 4;
        pending.push((first, a, budget));
        pending.push((b, last, budget));
    }
    Ok(())
}
