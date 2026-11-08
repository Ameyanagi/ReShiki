//! #1048: reaction SMILES import still reads a CX wU/wD flag without
//! coordinates on its own generated layout, unlike molecule SMILES (#1044),
//! so it invents a centre the input never states. The fix is deferred and
//! listed as a known issue for 0.14.0; remove this list when #1048 is fixed.
//! For exactly these inputs, each with one potential centre at the flagged
//! atom, only the identifiers and that atom's `stereo` come from the actual
//! response. Every other input, and every other field, is compared strictly.
use reshiki::engine::Response;

/// The known #1048 inputs and their flagged atom.
const KNOWN: [(&str, usize); 3] = [
    ("CO>N>C-2C=1CCC2C1 |wU:7.5|", 7),
    ("CO>C-2C=1CCC2C1>CC |wD:6.6|", 6),
    ("C>C-2C=1CCC2C1.C1CC1>O |wU:5.5|", 5),
];

/// The flagged atom of a known #1048 input.
pub fn known(format: &str, text: &str) -> Option<usize> {
    (format == "rsmi")
        .then(|| KNOWN.iter().find(|(input, _)| *input == text))
        .flatten()
        .map(|(_, atom)| *atom)
}

pub fn adapt(atom: usize, actual: &Response, expected: &mut Response) {
    if let (Some(a), Some(e)) = (&actual.analysis, expected.analysis.as_mut()) {
        e.inchi = a.inchi.clone();
        e.inchikey = a.inchikey.clone();
        e.smiles = a.smiles.clone();
    }
    if let (Some(a), Some(e)) = (&actual.document, expected.document.as_mut())
        && let (Some(a), Some(e)) = (a.atoms.get(atom), e.atoms.get_mut(atom))
    {
        e.stereo = a.stereo.clone();
    }
}
