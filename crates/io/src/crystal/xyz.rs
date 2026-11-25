//! Plain XYZ (a molecule without a cell) and extended XYZ, whose comment
//! line gives the cell as `Lattice="ax ay az bx by bz cx cy cz"` and the
//! columns as `Properties=species:S:1:pos:R:3:…`. The crystal import shows
//! the first frame. The quantum chemistry reader uses the same tokenizer.
use super::build::Lattice;
use super::{Atom, Error, Structure, V3, elements, number};
use reshiki_qcdata::MAX_FRAMES;

/// The `key=value` pairs of an extended XYZ comment line; keys and values
/// may be quoted with double quotes.
pub(crate) fn pairs(comment: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = comment.trim();
    while !rest.is_empty() {
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let key = key
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .trim_matches('"');
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

/// The element (species or atomic number) and position columns from `Properties`.
pub(crate) fn columns(properties: &str) -> Option<(usize, bool, usize)> {
    let fields: Vec<&str> = properties.split(':').collect();
    let (mut column, mut species, mut atomic_number, mut position) = (0usize, None, None, None);
    for chunk in fields.chunks(3) {
        let [name, kind, count] = chunk else {
            return None;
        };
        let count: usize = count.parse().ok().filter(|c| (1..=1_000).contains(c))?;
        match (name.to_ascii_lowercase().as_str(), *kind) {
            ("species", "S") if count == 1 => species = Some(column),
            ("z", "I") if count == 1 => atomic_number = Some(column),
            ("pos", "R") if count == 3 => position = Some(column),
            _ => {}
        }
        column = column.checked_add(count)?;
    }
    // Preserve the established species column when both representations are
    // present. Z is a fallback and must contain integer atomic numbers.
    let (element, atomic_numbers) = species
        .map(|column| (column, false))
        .or_else(|| atomic_number.map(|column| (column, true)))?;
    Some((element, atomic_numbers, position?))
}

impl crate::qc_input::XyzColumns {
    /// Read extended XYZ columns, using ordinary XYZ columns when absent.
    pub fn from_comment(comment: &str) -> Option<Self> {
        let (species, position) = match pairs(comment)
            .into_iter()
            .find(|(key, _)| key == "properties")
        {
            // Export reads element symbols, so a Z-only column is not accepted.
            Some((_, properties)) => match columns(&properties)? {
                (species, false, position) => (species, position),
                (_, true, _) => return None,
            },
            None => (0, 1),
        };
        Some(Self { species, position })
    }
}

/// Numbered XYZ lines, shared by the crystal and quantum chemistry readers.
pub(crate) type Lines<'a> = std::iter::Enumerate<std::str::Lines<'a>>;

/// The count and comment lines of one XYZ frame.
pub(crate) struct Header<'a> {
    /// One-based line of the atom count.
    pub line: usize,
    /// Number of atom rows after the comment.
    pub count: usize,
    /// Original comment text.
    pub comment: &'a str,
}

/// The atom rows and extended XYZ metadata of one frame.
pub(crate) struct Frame<A> {
    /// Resolved elements and positions in file order.
    pub atoms: Vec<(A, V3)>,
    /// Extended XYZ properties with lowercase keys.
    pub pairs: Vec<(String, String)>,
}

/// Read the next frame header, skipping blank lines between frames.
pub(crate) fn header<'a>(lines: &mut Lines<'a>) -> Result<Option<Header<'a>>, Error> {
    let Some((index, count)) = lines.find(|(_, l)| !l.trim().is_empty()) else {
        return Ok(None);
    };
    let line = index + 1;
    let count = count
        .trim()
        .parse()
        .map_err(|_| Error::at(line, "The first line must be the number of atoms"))?;
    let comment = lines.next().map_or("", |(_, comment)| comment);
    Ok(Some(Header {
        line,
        count,
        comment,
    }))
}

/// Advance over discarded atom rows without allocating or parsing their coordinates.
pub(crate) fn skip_atoms(lines: &mut Lines<'_>, count: usize) -> bool {
    count == 0 || lines.nth(count - 1).is_some()
}

/// A malformed atom or comment line, or the one-based line of a missing atom.
pub(crate) enum FrameError {
    /// The comment or atom row breaks the format.
    Invalid(Error),
    /// The file ended before this one-based atom line.
    Incomplete(usize),
}

/// Parse one frame in file order. The caller resolves element names and can
/// validate comment metadata before the atom rows are read.
pub(crate) fn read_frame<A>(
    header: &Header<'_>,
    lines: &mut Lines<'_>,
    element: impl Fn(&str) -> Option<A>,
    check_comment: impl FnOnce(&[(String, String)]) -> Result<(), Error>,
) -> Result<Frame<A>, FrameError> {
    let pairs = pairs(header.comment);
    check_comment(&pairs).map_err(FrameError::Invalid)?;
    let (species, atomic_numbers, position) = match value(&pairs, "properties") {
        Some(p) => columns(p).ok_or_else(|| {
            FrameError::Invalid(Error::at(
                header.line + 1,
                "Properties needs species:S:1 or Z:I:1, and pos:R:3 columns",
            ))
        })?,
        None => (0, false, 1),
    };
    let mut atoms = Vec::with_capacity(header.count);
    for i in 0..header.count {
        let (index, text) = lines
            .next()
            .ok_or(FrameError::Incomplete(header.line + 2 + i))?;
        let line = index + 1;
        let missing =
            || FrameError::Invalid(Error::at(line, "An atom line needs an element and x, y, z"));
        let words: Vec<&str> = text.split_whitespace().collect();
        let name = words.get(species).ok_or_else(missing)?;
        let invalid_element =
            || FrameError::Invalid(Error::at(line, &format!("'{name}' is not an element")));
        let digits = name.strip_prefix('+').unwrap_or(name);
        if atomic_numbers && (digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit())) {
            return Err(FrameError::Invalid(Error::at(
                line,
                "Expected an atomic number in the Z column",
            )));
        }
        let element = element(name).ok_or_else(invalid_element)?;
        let mut xyz = [0.; 3];
        for (k, v) in xyz.iter_mut().enumerate() {
            *v = words
                .get(position + k)
                .and_then(|w| number(w))
                .ok_or_else(missing)?;
        }
        atoms.push((element, xyz));
    }
    Ok(Frame { atoms, pairs })
}

/// Look up an extended XYZ comment property by its lowercase key.
pub(crate) fn value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

pub(super) fn read(text: &str) -> Result<Structure, Error> {
    let mut lines = text.lines().enumerate();
    let first = header(&mut lines)?.ok_or_else(|| Error::new("The XYZ file is empty"))?;
    let count = first.count;
    if count > super::MAX_ATOMS {
        return Err(Error(format!(
            "The structure has {count} atoms; at most {} can be shown",
            super::MAX_ATOMS
        )));
    }
    let mut lattice = None;
    let frame = read_frame(
        &first,
        &mut lines,
        |name| match name.parse::<usize>() {
            Ok(z) => elements::from_number(z),
            Err(_) => elements::from_symbol(name)
                .filter(|(e, _)| e.eq_ignore_ascii_case(name) || name.eq_ignore_ascii_case("D"))
                .map(|(e, _)| e),
        },
        |pairs| {
            lattice = read_lattice(first.line + 1, pairs)?;
            Ok(())
        },
    )
    .map_err(|error| match error {
        FrameError::Invalid(error) => error,
        FrameError::Incomplete(_) => Error(format!("The XYZ file ends before its {count} atoms")),
    })?;

    let atoms = frame
        .atoms
        .into_iter()
        .map(|(element, xyz)| Atom {
            element,
            charge: 0,
            label: String::new(),
            xyz,
            adp: None,
            asymmetric: false,
        })
        .collect();
    let mut frames = 1;
    while frames <= MAX_FRAMES
        && let Ok(Some(next)) = header(&mut lines)
        && skip_atoms(&mut lines, next.count)
    {
        frames += 1;
    }
    let mut warnings = Vec::new();
    if frames > 1 {
        let frames = if frames > MAX_FRAMES {
            format!("at least {frames}")
        } else {
            frames.to_string()
        };
        warnings.push(format!(
            "This XYZ file has {frames} frames; the first is shown"
        ));
    }
    Ok(Structure {
        lattice,
        atoms,
        warnings,
    })
}

fn read_lattice(line: usize, pairs: &[(String, String)]) -> Result<Option<Lattice>, Error> {
    // Partly periodic structures, such as slabs, are not supported; a
    // structure periodic along no axis is a molecule.
    let periodic = match value(pairs, "pbc").map(|p| {
        p.split_whitespace()
            .map(|f| matches!(f, "T" | "t" | "True" | "true" | "1"))
            .collect::<Vec<_>>()
    }) {
        None => true,
        Some(flags) if flags.len() == 3 && flags.iter().all(|f| *f) => true,
        Some(flags) if flags.len() == 3 && flags.iter().all(|f| !*f) => false,
        Some(_) => {
            return Err(Error::at(
                line,
                "Only pbc=\"T T T\" or \"F F F\" is supported",
            ));
        }
    };
    value(pairs, "lattice")
        .filter(|_| periodic)
        .map(|text| {
            let numbers: Option<Vec<f64>> = text.split_whitespace().map(number).collect();
            match numbers.as_deref() {
                Some([ax, ay, az, bx, by, bz, cx, cy, cz]) => {
                    Lattice::new([[*ax, *ay, *az], [*bx, *by, *bz], [*cx, *cy, *cz]])
                        .map_err(|e| Error::at(line, &e.to_string()))
                }
                _ => Err(Error::at(line, "Lattice needs nine numbers")),
            }
        })
        .transpose()
}
