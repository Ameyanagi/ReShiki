//! Resonance-averaged atomic numbers, including native negative-charge order.
use super::{Error, Fraction, Molecule, at, at_mut};
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Other,
    Typed,
    Negative,
}

pub(super) fn calculate(mol: &mut Molecule<'_>) -> Result<Vec<Fraction>, Error> {
    let n = mol.state.graph.atoms.len();
    let mut fractions = mol
        .state
        .graph
        .atoms
        .iter()
        .map(|a| Fraction(u32::from(a.atomic_number), 1))
        .collect::<Vec<_>>();
    let mut kinds = vec![Kind::Other; n];
    let mut resonant = false;
    for i in 0..n {
        let atom = at(&mol.state.graph.atoms, i)?;
        let (number, charge) = (atom.atomic_number, atom.charge);
        let mut types = u64::from(atom.explicit_hydrogens)
            + u64::from(at(&mol.state.valences, i)?.implicit_hydrogens);
        let mut ring = false;
        for j in 0..at(&mol.adjacent, i)?.len() {
            let (_, edge) = *at(at(&mol.adjacent, i)?, j)?;
            types += match mol.bond_order(edge)? {
                1 => 1,
                2 => 0x100,
                _ => 0x1000000,
            };
            ring |= mol.is_in_ring(edge)?;
        }
        let kind = if ring {
            match (number, charge, types) {
                (6 | 14 | 32, 0, 0x102) => Kind::Typed,
                (6 | 14 | 32, -1, 3) | (7 | 15 | 33, -1, 2) => {
                    resonant = true;
                    Kind::Negative
                }
                (7 | 15 | 33, 0, 0x101) | (7 | 15 | 33, 1, 0x102) | (8, 1, 0x101) => {
                    resonant = true;
                    Kind::Typed
                }
                _ => Kind::Other,
            }
        } else {
            Kind::Other
        };
        *at_mut(&mut kinds, i)? = kind;
    }
    if !resonant {
        return Ok(fractions);
    }
    let mut counts = vec![0_i32; n];
    let mut queue = VecDeque::new();
    for (i, neighbors) in mol.adjacent.iter().enumerate() {
        let count = neighbors.iter().try_fold(0, |n, &(a, _)| {
            Ok::<_, Error>(n + i32::from(*at(&kinds, a)? != Kind::Other))
        })?;
        *at_mut(&mut counts, i)? = count;
        if count == 1 {
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        if *at(&kinds, i)? == Kind::Other {
            continue;
        }
        *at_mut(&mut kinds, i)? = Kind::Other;
        for &(a, _) in at(&mol.adjacent, i)? {
            let count = at_mut(&mut counts, a)?;
            *count -= 1;
            if *count == 1 {
                queue.push_back(a);
            }
        }
    }
    let mut parts = vec![0_usize; n];
    let mut numparts = 0;
    for i in 0..n {
        if *at(&parts, i)? != 0 || *at(&kinds, i)? == Kind::Other {
            continue;
        }
        numparts += 1;
        *at_mut(&mut parts, i)? = numparts;
        let mut pending = vec![i];
        while let Some(a) = pending.pop() {
            for j in 0..at(&mol.adjacent, a)?.len() {
                let (b, edge) = *at(at(&mol.adjacent, a)?, j)?;
                if mol.is_in_ring(edge)? && *at(&parts, b)? == 0 && *at(&kinds, b)? != Kind::Other {
                    *at_mut(&mut parts, b)? = numparts;
                    pending.push(b);
                }
            }
        }
    }
    let mut negative = vec![false; numparts + 1];
    for i in 0..n {
        let part = *at(&parts, i)?;
        if part == 0 {
            continue;
        }
        if *at(&kinds, i)? == Kind::Negative {
            *at_mut(&mut negative, part)? = true;
        }
        let (mut numerator, mut denominator) = (0_u32, 0_u32);
        for &(a, _) in at(&mol.adjacent, i)? {
            if *at(&parts, a)? == part {
                numerator = numerator
                    .checked_add(u32::from(at(&mol.state.graph.atoms, a)?.atomic_number))
                    .ok_or(Error::Limit)?;
                denominator += 1;
            }
        }
        *at_mut(&mut fractions, i)? = Fraction::new(numerator, denominator);
    }
    // The pinned implementation assigns the running fraction BEFORE adding
    // each atom's contributions. Preserve that order independently per part.
    let mut sums = vec![(0_u32, 0_u32); numparts + 1];
    for i in 0..n {
        let part = *at(&parts, i)?;
        if !*at(&negative, part)? {
            continue;
        }
        let (numerator, denominator) = *at(&sums, part)?;
        *at_mut(&mut fractions, i)? = Fraction::new(numerator, denominator);
        at_mut(&mut sums, part)?.1 += 1;
        for j in 0..at(&mol.adjacent, i)?.len() {
            let (a, edge) = *at(at(&mol.adjacent, i)?, j)?;
            let order = mol.bond_order(edge)?;
            if order > 1 && *at(&parts, a)? == part {
                let extra =
                    u32::from(order - 1) * u32::from(at(&mol.state.graph.atoms, a)?.atomic_number);
                let sum = &mut at_mut(&mut sums, part)?.0;
                *sum = sum.checked_add(extra).ok_or(Error::Limit)?;
            }
        }
    }
    Ok(fractions)
}
