//! VASP 5 POSCAR and CONTCAR: the scaled lattice, the element and count
//! lines, optional selective dynamics, then Direct or Cartesian positions.
//! Velocities and anything after the positions are read past.
use super::build::Lattice;
use super::{Atom, Error, Structure, V3, elements, number};

pub(super) fn read(text: &str) -> Result<Structure, Error> {
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim()));
    let mut next = |what: &str| {
        lines
            .next()
            .ok_or_else(|| Error(format!("The POSCAR ends before its {what}")))
    };
    next("comment")?;
    let (line, factor) = next("scale factor")?;
    // One scale factor, or three for x, y and z; a comment may follow.
    let factors: Vec<f64> = factor.split_whitespace().map_while(number).collect();
    let factors: V3 = match factors.as_slice() {
        [f] if *f != 0. => [*f; 3],
        [x, y, z, ..] if [x, y, z].iter().all(|f| **f > 0.) => [*x, *y, *z],
        _ => {
            return Err(Error::at(
                line,
                "The scale factor must be one nonzero number or three positive numbers",
            ));
        }
    };
    let mut vectors = [[0.; 3]; 3];
    for v in &mut vectors {
        let (line, text) = next("lattice vectors")?;
        *v = three(text).ok_or_else(|| Error::at(line, "A lattice vector needs three numbers"))?;
    }
    // A single negative scale factor is the cell's volume.
    let factor = if factors[0] < 0. {
        let volume = super::dot(vectors[0], super::cross(vectors[1], vectors[2])).abs();
        if volume <= 0. {
            return Err(Error::new("The unit cell has no volume"));
        }
        [(-factors[0] / volume).cbrt(); 3]
    } else {
        factors
    };
    let scaled = |v: V3| [v[0] * factor[0], v[1] * factor[1], v[2] * factor[2]];
    let lattice = Lattice::new(vectors.map(scaled))?;
    let (line, names) = next("element names")?;
    if names.split_whitespace().all(|w| w.parse::<f64>().is_ok()) {
        return Err(Error::at(
            line,
            "This POSCAR has no element names; VASP 4 files are not supported",
        ));
    }
    let symbols = names
        .split_whitespace()
        .map(|name| {
            // POTCAR names such as Fe_pv or O/1a.
            let bare = name.split(['_', '/']).next().unwrap_or(name);
            elements::from_symbol(bare)
                .filter(|(e, _)| e.eq_ignore_ascii_case(bare))
                .map(|(e, _)| e)
                .ok_or_else(|| Error::at(line, &format!("'{name}' is not an element")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (line, counts) = next("atom counts")?;
    let counts = counts
        .split_whitespace()
        .map(|c| c.parse::<usize>().ok())
        .collect::<Option<Vec<_>>>()
        .filter(|c| c.len() == symbols.len())
        .ok_or_else(|| Error::at(line, "The atom counts must be one whole number per element"))?;
    let total = counts.iter().try_fold(0usize, |sum, c| sum.checked_add(*c));
    if total.is_none_or(|t| t > super::MAX_ATOMS) {
        return Err(Error(format!(
            "The structure has more than {} atoms; at most {} can be shown",
            super::MAX_ATOMS,
            super::MAX_ATOMS
        )));
    }
    let (mut line, mut mode) = next("coordinate mode")?;
    if mode.starts_with(['S', 's']) {
        (line, mode) = next("coordinate mode")?;
    }
    let direct = match mode.chars().next() {
        Some('D' | 'd') => true,
        Some('C' | 'c' | 'K' | 'k') => false,
        _ => return Err(Error::at(line, "Expected Direct or Cartesian")),
    };
    let mut atoms = vec![];
    for (symbol, count) in symbols.iter().zip(counts) {
        for _ in 0..count {
            let (line, text) = next("positions")?;
            let p = three(text).ok_or_else(|| Error::at(line, "A position needs three numbers"))?;
            let xyz = if direct {
                lattice.cartesian(p)
            } else {
                scaled(p)
            };
            atoms.push(Atom {
                element: symbol.clone(),
                charge: 0,
                label: String::new(),
                xyz,
                adp: None,
            });
        }
    }
    Ok(Structure {
        lattice: Some(lattice),
        atoms,
        warnings: vec![],
    })
}

/// The first three numbers of a line.
fn three(text: &str) -> Option<V3> {
    let mut words = text.split_whitespace();
    let mut v = [0.; 3];
    for x in &mut v {
        *x = number(words.next()?)?;
    }
    Some(v)
}
