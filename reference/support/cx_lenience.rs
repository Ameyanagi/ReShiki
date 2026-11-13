//! RDKit 2026.09 reads CX numbers with C `atoi`/`atof` (rdkit#9273), so it
//! accepts malformed, padded, hexadecimal, overflowing and underflowing
//! numbers that 2026.03.6 rejected through `boost::lexical_cast`. ReShiki
//! keeps the strict reader; see "Deliberate differences" in reference/README.md.
use reshiki::chemistry::cx;

/// Whether the CX extension of `text` holds a number only the lenient reader
/// accepts: a coordinate the strict reader rejects, an `&#N;` escape beyond
/// `i32`, or another integer beyond `u32`.
pub fn lenient_number(text: &str) -> bool {
    let Some((_, block)) = text.split_once('|') else {
        return false;
    };
    let escape = block.split("&#").skip(1).any(|rest| {
        rest.split_once(';')
            .is_some_and(|(n, _)| !n.is_empty() && n.parse::<i32>().is_err())
    });
    // Parentheses not attached to a label word hold coordinates; RDKit reads
    // up to three values for each `;`-separated atom.
    let (mut coordinate, mut others, mut rest) = (false, String::new(), block);
    while let Some(open) = rest.find('(') {
        others.push_str(&rest[..open]);
        let section = &rest[open + 1..];
        let close = section.find(')').unwrap_or(section.len());
        if !others.ends_with(|c: char| c.is_ascii_alphabetic()) {
            coordinate |= section[..close].split(';').any(|atom| {
                atom.split(',')
                    .take(3)
                    .any(|value| !value.is_empty() && cx::coordinate(value).is_none())
            });
        } else {
            others.push_str(&section[..close]);
        }
        rest = section.get(close + 1..).unwrap_or_default();
    }
    others.push_str(rest);
    escape
        || coordinate
        || others
            .split(|c: char| !c.is_ascii_digit())
            .any(|n| !n.is_empty() && n.parse::<u32>().is_err())
}

/// A molfile whose SMARTSQ data (`FIELDDATA="..."` or `M  SED`) holds such a
/// number: RDKit 2026.09 now applies the query that 2026.03.6 ignored.
pub fn lenient_query(molfile: &str) -> bool {
    molfile.lines().any(|line| {
        (line.contains("FIELDDATA=") || line.starts_with("M  SED")) && lenient_number(line)
    })
}
