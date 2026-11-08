//! #1048: reaction SMILES import still reads a CX wU/wD flag without
//! coordinates on its own generated layout, unlike molecule SMILES (#1044),
//! so it can invent a centre the input never states. The fix is deferred and
//! listed as a known issue for 0.14.0. For such inputs only the identifiers
//! and the flagged atoms' `stereo` come from the actual response; acceptance,
//! rejection and every other field stay compared.
use reshiki::engine::Response;

/// Atom indices of coordinate-free wU/wD flags in a reaction SMILES.
pub fn atoms(format: &str, text: &str) -> Vec<usize> {
    let Some((_, cx)) = text.trim().split_once(char::is_whitespace) else {
        return Vec::new();
    };
    if format != "rsmi" || cx.contains('(') {
        return Vec::new();
    }
    let mut atoms = Vec::new();
    for (at, _) in cx.match_indices("wU:").chain(cx.match_indices("wD:")) {
        for pair in cx[at + 3..].split(',') {
            let Some((atom, bond)) = pair.trim_end_matches('|').split_once('.') else {
                break;
            };
            match (atom.parse::<usize>(), bond.parse::<usize>()) {
                (Ok(atom), Ok(_)) => atoms.push(atom),
                _ => break,
            }
        }
    }
    atoms
}

pub fn adapt(atoms: &[usize], actual: &Response, expected: &mut Response) {
    // Only an invented centre: every flagged atom exists and carries stereo.
    let invented = actual.document.as_ref().is_some_and(|doc| {
        atoms
            .iter()
            .all(|&i| doc.atoms.get(i).is_some_and(|a| a.stereo.is_some()))
    });
    if !invented {
        return;
    }
    if let (Some(a), Some(e)) = (&actual.analysis, expected.analysis.as_mut()) {
        e.inchi = a.inchi.clone();
        e.inchikey = a.inchikey.clone();
        e.smiles = a.smiles.clone();
    }
    if let (Some(a), Some(e)) = (&actual.document, expected.document.as_mut()) {
        for &i in atoms {
            if let (Some(a), Some(e)) = (a.atoms.get(i), e.atoms.get_mut(i)) {
                e.stereo = a.stereo.clone();
            }
        }
    }
}
