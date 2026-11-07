//! #793 writes a crossed (unknown-geometry) double bond as MOL bond stereo 3,
//! "either", where the pinned RDKit writer writes 0. Accept exactly that
//! difference on such bonds; every other byte must still match.
use reshiki::chemistry::{kekulize::Direction, stereo::perception::State};
use std::collections::HashSet;

/// 1-based atom pairs of the crossed double bonds in one molecule.
pub fn pairs(state: &State) -> HashSet<(usize, usize)> {
    state
        .graph
        .bonds
        .iter()
        .zip(&state.directions)
        .filter(|(b, d)| b.order == 2 && **d == Direction::Unknown)
        .map(|(b, _)| (b.a.min(b.b) + 1, b.a.max(b.b) + 1))
        .collect()
}

/// Whether `actual` equals `expected` up to "either" stereo on the double
/// bonds `crossed` accepts, given their 1-based atom pair.
pub fn matches(actual: &str, expected: &str, crossed: impl Fn(usize, usize) -> bool) -> bool {
    matches_in_blocks(actual, expected, |_, a, b| crossed(a, b))
}

/// As `matches`, for an RXN file: `crossed` also gets the 0-based molecule
/// block (`$MOL` or V3000 CTAB) that the bond line belongs to.
pub fn matches_in_blocks(
    actual: &str,
    expected: &str,
    crossed: impl Fn(usize, usize, usize) -> bool,
) -> bool {
    let mut block = None::<usize>;
    let (actual, expected): (Vec<_>, Vec<_>) =
        (actual.lines().collect(), expected.lines().collect());
    actual.len() == expected.len()
        && actual.iter().zip(&expected).all(|(a, e)| {
            if *e == "$MOL" || *e == "M  V30 BEGIN CTAB" {
                block = Some(block.map_or(0, |b| b + 1));
            }
            let pair = |a: &str, b: &str| {
                let (a, b) = (a.trim().parse::<usize>(), b.trim().parse::<usize>());
                matches!((a, b), (Ok(a), Ok(b)) if crossed(block.unwrap_or(0), a.min(b), a.max(b)))
            };
            if a == e {
                return true;
            }
            // V2000 bond line: aaabbbtttsss...
            if a.len() >= 12
                && a.len() == e.len()
                && a.is_char_boundary(12)
                && e.is_char_boundary(12)
            {
                return a[..9] == e[..9]
                    && a[12..] == e[12..]
                    && a[6..9].trim() == "2"
                    && a[9..12].trim() == "3"
                    && e[9..12].trim() == "0"
                    && pair(&a[..3], &a[3..6]);
            }
            // V3000 bond line: M  V30 index type a b [CFG=2]
            let fields: Vec<_> = e.split_whitespace().collect();
            *a == format!("{e} CFG=2")
                && fields.len() == 6
                && fields[..2] == ["M", "V30"]
                && fields[3] == "2"
                && pair(fields[4], fields[5])
        })
}
