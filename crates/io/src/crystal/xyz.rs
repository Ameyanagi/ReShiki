//! Plain XYZ (a molecule without a cell) and extended XYZ, whose comment
//! line gives the cell as `Lattice="ax ay az bx by bz cx cy cz"` and the
//! columns as `Properties=species:S:1:pos:R:3:…`. Only the first frame is
//! read; other properties are read past.
use super::build::Lattice;
use super::{Atom, Error, Structure, V3, elements, number};

/// The `key=value` pairs of an extended XYZ comment line; values may be
/// quoted with double quotes.
fn pairs(comment: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = comment.trim();
    while !rest.is_empty() {
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let key = key.split_whitespace().last().unwrap_or_default();
        let after = after.trim_start();
        let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
            quoted.split_once('"').unwrap_or((quoted, ""))
        } else {
            after.split_once(char::is_whitespace).unwrap_or((after, ""))
        };
        out.push((key.to_ascii_lowercase(), value.to_owned()));
        rest = tail.trim_start();
    }
    out
}

/// The species and position columns from a `Properties` value.
fn columns(properties: &str) -> Option<(usize, usize)> {
    let fields: Vec<&str> = properties.split(':').collect();
    let (mut column, mut species, mut position) = (0usize, None, None);
    for chunk in fields.chunks(3) {
        let [name, kind, count] = chunk else {
            return None;
        };
        let count: usize = count.parse().ok().filter(|c| (1..=1_000).contains(c))?;
        match (name.to_ascii_lowercase().as_str(), *kind) {
            ("species", "S") if count == 1 => species = Some(column),
            ("pos", "R") if count == 3 => position = Some(column),
            _ => {}
        }
        column = column.checked_add(count)?;
    }
    Some((species?, position?))
}

pub(super) fn read(text: &str) -> Result<Structure, Error> {
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l));
    let (line, count) = lines
        .find(|(_, l)| !l.trim().is_empty())
        .ok_or_else(|| Error::new("The XYZ file is empty"))?;
    let count: usize = count
        .trim()
        .parse()
        .map_err(|_| Error::at(line, "The first line must be the number of atoms"))?;
    if count > super::MAX_ATOMS {
        return Err(Error(format!(
            "The structure has {count} atoms; at most {} can be shown",
            super::MAX_ATOMS
        )));
    }
    let (_, comment) = lines.next().unwrap_or((line + 1, ""));
    let pairs = pairs(comment);
    let value = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    // Partly periodic structures, such as slabs, are not supported; a
    // structure periodic along no axis is a molecule.
    let periodic = match value("pbc").map(|p| {
        p.split_whitespace()
            .map(|f| matches!(f, "T" | "t" | "True" | "true" | "1"))
            .collect::<Vec<_>>()
    }) {
        None => true,
        Some(flags) if flags.len() == 3 && flags.iter().all(|f| *f) => true,
        Some(flags) if flags.len() == 3 && flags.iter().all(|f| !*f) => false,
        Some(_) => {
            return Err(Error::at(
                line + 1,
                "Only pbc=\"T T T\" or \"F F F\" is supported",
            ));
        }
    };
    let lattice = value("lattice")
        .filter(|_| periodic)
        .map(|text| {
            let numbers: Option<Vec<f64>> = text.split_whitespace().map(number).collect();
            match numbers.as_deref() {
                Some([ax, ay, az, bx, by, bz, cx, cy, cz]) => {
                    Lattice::new([[*ax, *ay, *az], [*bx, *by, *bz], [*cx, *cy, *cz]])
                        .map_err(|e| Error::at(line + 1, &e.to_string()))
                }
                _ => Err(Error::at(line + 1, "Lattice needs nine numbers")),
            }
        })
        .transpose()?;
    let (species, position) = match value("properties") {
        Some(p) => columns(p).ok_or_else(|| {
            Error::at(line + 1, "Properties needs species:S:1 and pos:R:3 columns")
        })?,
        None => (0, 1),
    };
    let mut atoms = vec![];
    for _ in 0..count {
        let (line, text) = lines
            .next()
            .ok_or_else(|| Error(format!("The XYZ file ends before its {count} atoms")))?;
        let words: Vec<&str> = text.split_whitespace().collect();
        let name = words
            .get(species)
            .ok_or_else(|| Error::at(line, "An atom line needs an element and x, y, z"))?;
        let element = match name.parse::<usize>() {
            Ok(z) => elements::from_number(z),
            Err(_) => elements::from_symbol(name)
                .filter(|(e, _)| e.eq_ignore_ascii_case(name) || name.eq_ignore_ascii_case("D"))
                .map(|(e, _)| e),
        }
        .ok_or_else(|| Error::at(line, &format!("'{name}' is not an element")))?;
        let mut xyz: V3 = [0.; 3];
        for (k, v) in xyz.iter_mut().enumerate() {
            *v = words
                .get(position + k)
                .and_then(|w| number(w))
                .ok_or_else(|| Error::at(line, "An atom line needs an element and x, y, z"))?;
        }
        atoms.push(Atom {
            element,
            charge: 0,
            label: String::new(),
            xyz,
            adp: None,
        });
    }
    Ok(Structure {
        lattice,
        atoms,
        warnings: vec![],
    })
}
