//! Coordination permutation tables adapted from RDKit NontetrahedralStereo.cpp.
//! Copyright (C) 2022 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Error, Result};

/// Convert text neighbor order to storage order, keeping absent ligands.
pub(super) fn permutation(
    tag: u8,
    mut value: u32,
    bonds: &[usize],
    adjacent: &[(usize, usize)],
    first: bool,
) -> Result<u32> {
    let maximum = match tag {
        6 => 4,
        7 => 5,
        8 => 6,
        _ => return Err(Error::Limit),
    };
    if value == 0 || bonds.len() > maximum {
        return Ok(0);
    }
    let mut current = bonds.iter().copied().map(Some).collect::<Vec<_>>();
    let missing = maximum - current.len();
    let insertion = if first { 0 } else { 1.min(current.len()) };
    current.splice(insertion..insertion, std::iter::repeat_n(None, missing));
    let mut target = adjacent.iter().map(|(_, b)| Some(*b)).collect::<Vec<_>>();
    target.resize(maximum, None);
    for (i, desired) in target.iter().enumerate() {
        if current.get(i) == Some(desired) {
            continue;
        }
        let j = current
            .iter()
            .enumerate()
            .skip(i)
            .find(|(_, v)| *v == desired)
            .map(|(j, _)| j)
            .ok_or(Error::Limit)?;
        let column = (0..i).map(|x| maximum - x - 1).sum::<usize>() + j - i - 1;
        value = u32::from(
            match tag {
                6 => SP.get(value as usize).and_then(|row| row.get(column)),
                7 => TB.get(value as usize).and_then(|row| row.get(column)),
                8 => OH.get(value as usize).and_then(|row| row.get(column)),
                _ => None,
            }
            .copied()
            .ok_or(Error::Limit)?,
        );
        // Both indices came from the bounded vector above; use checked access
        // for the mutation as well so malformed state always returns an error.
        let old = *current.get(i).ok_or(Error::Limit)?;
        *current.get_mut(i).ok_or(Error::Limit)? = *desired;
        *current.get_mut(j).ok_or(Error::Limit)? = old;
    }
    Ok(value)
}

const SP: [[u8; 6]; 4] = [
    [0, 0, 0, 0, 0, 0],
    [3, 1, 2, 2, 1, 3],
    [2, 3, 1, 1, 3, 2],
    [1, 2, 3, 3, 2, 1],
];

const TB: [[u8; 10]; 21] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [9, 20, 17, 2, 2, 2, 7, 2, 6, 3],
    [11, 15, 18, 1, 1, 1, 8, 1, 5, 4],
    [10, 19, 4, 18, 4, 8, 4, 5, 4, 1],
    [12, 16, 3, 17, 3, 7, 3, 6, 3, 2],
    [13, 6, 16, 20, 7, 6, 6, 3, 2, 6],
    [14, 5, 19, 15, 8, 5, 5, 4, 1, 5],
    [8, 14, 10, 11, 5, 4, 1, 8, 8, 8],
    [7, 13, 12, 9, 6, 3, 2, 7, 7, 7],
    [1, 11, 11, 8, 15, 18, 11, 11, 14, 10],
    [3, 12, 7, 12, 16, 12, 17, 13, 12, 9],
    [2, 9, 9, 7, 20, 17, 9, 9, 13, 12],
    [4, 10, 8, 10, 19, 10, 18, 14, 10, 11],
    [5, 8, 14, 14, 14, 19, 15, 10, 11, 14],
    [6, 7, 13, 13, 13, 16, 20, 12, 9, 13],
    [20, 2, 20, 6, 9, 20, 13, 17, 20, 16],
    [19, 4, 5, 19, 10, 14, 19, 19, 18, 15],
    [18, 18, 1, 4, 18, 11, 10, 15, 19, 18],
    [17, 17, 2, 3, 17, 9, 12, 20, 16, 17],
    [16, 3, 6, 16, 12, 13, 16, 16, 17, 20],
    [15, 1, 15, 5, 11, 15, 14, 18, 15, 19],
];

const OH: [[u8; 15]; 31] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [17, 16, 30, 21, 2, 14, 2, 10, 25, 8, 2, 22, 4, 7, 3],
    [7, 3, 25, 22, 1, 4, 1, 8, 30, 10, 1, 21, 14, 17, 16],
    [18, 2, 29, 16, 22, 15, 16, 26, 11, 9, 21, 16, 6, 5, 1],
    [15, 18, 19, 28, 14, 2, 8, 14, 27, 14, 10, 24, 1, 6, 5],
    [14, 17, 20, 15, 27, 16, 9, 28, 15, 15, 23, 11, 7, 3, 4],
    [16, 14, 18, 26, 24, 17, 29, 18, 13, 19, 12, 18, 3, 4, 7],
    [2, 15, 17, 23, 25, 18, 30, 12, 17, 20, 17, 13, 5, 1, 6],
    [23, 26, 11, 12, 10, 10, 4, 2, 29, 1, 14, 20, 10, 13, 9],
    [24, 25, 10, 11, 13, 11, 5, 30, 16, 3, 19, 15, 12, 11, 8],
    [20, 29, 9, 13, 8, 8, 14, 1, 26, 2, 4, 23, 8, 12, 11],
    [19, 30, 8, 9, 12, 9, 15, 25, 3, 16, 24, 5, 13, 9, 10],
    [22, 27, 13, 8, 11, 13, 28, 7, 18, 21, 6, 17, 9, 10, 13],
    [21, 28, 12, 10, 9, 12, 27, 17, 6, 22, 18, 7, 11, 8, 12],
    [5, 6, 24, 27, 4, 1, 10, 4, 28, 4, 8, 19, 2, 18, 15],
    [4, 7, 23, 5, 28, 3, 11, 27, 5, 5, 20, 9, 17, 16, 14],
    [6, 1, 26, 3, 21, 5, 3, 29, 9, 11, 22, 3, 18, 15, 2],
    [1, 5, 7, 20, 30, 6, 25, 13, 7, 23, 7, 12, 15, 2, 18],
    [3, 4, 6, 29, 19, 7, 26, 6, 12, 24, 13, 6, 16, 14, 17],
    [11, 24, 4, 30, 18, 25, 23, 24, 22, 6, 9, 14, 21, 24, 20],
    [10, 23, 5, 17, 29, 26, 24, 21, 23, 7, 15, 8, 23, 22, 19],
    [13, 22, 28, 1, 16, 27, 22, 20, 24, 12, 3, 2, 19, 23, 22],
    [12, 21, 27, 2, 3, 28, 21, 23, 19, 13, 16, 1, 24, 20, 21],
    [8, 20, 15, 7, 26, 29, 19, 22, 20, 17, 5, 10, 20, 21, 24],
    [9, 19, 14, 25, 6, 30, 20, 19, 21, 18, 11, 4, 22, 19, 23],
    [30, 9, 2, 24, 7, 19, 17, 11, 1, 29, 30, 28, 27, 30, 26],
    [29, 8, 16, 6, 23, 20, 18, 3, 10, 30, 27, 29, 29, 28, 25],
    [28, 12, 22, 14, 5, 21, 13, 15, 4, 28, 26, 30, 25, 29, 28],
    [27, 13, 21, 4, 15, 22, 12, 5, 14, 27, 29, 25, 30, 26, 27],
    [26, 10, 3, 18, 20, 23, 6, 16, 8, 25, 28, 26, 26, 27, 30],
    [25, 11, 1, 19, 17, 24, 7, 9, 2, 26, 25, 27, 28, 25, 29],
];
