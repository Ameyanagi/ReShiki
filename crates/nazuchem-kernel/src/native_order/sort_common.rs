// SPDX-License-Identifier: NOASSERTION
// Origin: mixed ReShiki sort helpers and LLVM libc++/Microsoft STL heap policies; exact source allocation remains unresolved.
//   Microsoft STL, stl/inc/__msvc_heap_algorithms.hpp @ vs-2022-17.13 / MSVC 14.43.34808.
//   LLVM libc++, libcxx/include/__algorithm/make_heap.h @ llvmorg-20.1.8 / Apple MacOSX26.2 SDK.
//   LLVM libc++, libcxx/include/__algorithm/pop_heap.h @ llvmorg-20.1.8 / Apple MacOSX26.2 SDK.
//   LLVM libc++, libcxx/include/__algorithm/push_heap.h @ llvmorg-20.1.8 / Apple MacOSX26.2 SDK.
//   LLVM libc++, libcxx/include/__algorithm/sort_heap.h @ llvmorg-20.1.8 / Apple MacOSX26.2 SDK.
//   ReShiki, crates/chemistry/src/native_order/sort_common.rs @ be63a799aa03a83dc6ec74bb294f4d42b89298d3.
// Copyright (c) Microsoft Corporation.
// Copyright (c) 2003-2019 University of Illinois at Urbana-Champaign.
// Changes: Rust adaptation; ledger-marked local functions; the internal chemistry-to-kernel copy and any extraction changes remain recorded below.
// Unresolved: Known LLVM/MSVC policies and generic storage helpers are interleaved; exact source/independent-origin allocation for adjust/heap is not documented, so no complete license expression is asserted.

//! Checked storage operations shared by the reference sorting policies.
// Copied unchanged from crates/chemistry/src/native_order/sort_common.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3
use super::{Error, at, at_mut};
pub(super) type Entry = (i32, usize);

// Original nazuchem code (not in Microsoft STL/LLVM libc++; ledger-classified local helpers; file source allocation unresolved).
pub(super) fn key(values: &[Entry], i: usize) -> Result<i32, Error> {
    Ok(at(values, i)?.0)
}
// Original nazuchem code (not in Microsoft STL/LLVM libc++; ledger-classified local helpers; file source allocation unresolved).
pub(super) fn swap(values: &mut [Entry], a: usize, b: usize) -> Result<(), Error> {
    let first = *at(values, a)?;
    let second = *at(values, b)?;
    *at_mut(values, a)? = second;
    *at_mut(values, b)? = first;
    Ok(())
}
// Adapted from crates/chemistry/src/native_order/sort_common.rs::insert (local Rust boundary; exact upstream function mapping not recorded).
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

// Adapted from crates/chemistry/src/native_order/sort_common.rs::adjust (local Rust boundary; exact upstream function mapping not recorded).
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
// Adapted from crates/chemistry/src/native_order/sort_common.rs::heap (local Rust boundary; exact upstream function mapping not recorded).
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
