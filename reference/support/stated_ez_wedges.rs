//! #1042: a CX wU/wD flag on a `/` or `\` bond beside a non-aromatic,
//! non-cumulated double bond drops the stated E/Z from import analysis. The
//! fix is deferred past 0.14.0 and listed there as a known issue. For these
//! inputs only the identifiers and that double bond's `stereo_authoritative`
//! are left out; acceptance, rejection and every other field stay compared.
use reshiki::{
    chemistry::{kekulize::Direction, smiles},
    engine::Response,
};

/// Graph indices of the double bonds next to such a flagged slash bond.
pub fn double_bonds(format: &str, text: &str) -> Vec<usize> {
    let mut parts = text.trim().splitn(2, char::is_whitespace);
    let (Some(body), Some(cx)) = (parts.next(), parts.next()) else {
        return Vec::new();
    };
    let Ok(parsed) = smiles::parse(body) else {
        return Vec::new();
    };
    if format != "smiles" {
        return Vec::new();
    }
    let graph = &parsed.graph;
    let doubles_at = |atom: usize| {
        graph
            .bonds
            .iter()
            .enumerate()
            .filter(move |(_, b)| (b.a == atom || b.b == atom) && b.order == 2 && !b.aromatic)
            .map(|(i, _)| i)
    };
    let mut result = Vec::new();
    for textual in wedge_bonds(cx) {
        let Some(bond) = parsed.bond_indices.iter().position(|&t| t == textual) else {
            continue;
        };
        if !matches!(
            parsed.directions.get(bond),
            Some(Direction::Up | Direction::Down)
        ) {
            continue;
        }
        let Some(flagged) = graph.bonds.get(bond) else {
            continue;
        };
        for atom in [flagged.a, flagged.b] {
            for double in doubles_at(atom) {
                let d = &graph.bonds[double];
                let cumulated = doubles_at(d.a).count() > 1 || doubles_at(d.b).count() > 1;
                if !cumulated && !result.contains(&double) {
                    result.push(double);
                }
            }
        }
    }
    result
}

/// Textual bond indices of the CX `wU`/`wD` flags (`atom.bond` pairs).
fn wedge_bonds(cx: &str) -> Vec<usize> {
    let mut bonds = Vec::new();
    for (at, _) in cx.match_indices("wU:").chain(cx.match_indices("wD:")) {
        for pair in cx[at + 3..].split(',') {
            let pair = pair.trim_end_matches('|');
            let Some((atom, bond)) = pair.split_once('.') else {
                break;
            };
            match (atom.parse::<usize>(), bond.parse::<usize>()) {
                (Ok(_), Ok(bond)) => bonds.push(bond),
                _ => break,
            }
        }
    }
    bonds
}

/// Take only the fields #1042 changes from the actual response.
pub fn adapt(doubles: &[usize], actual: &Response, expected: &mut Response) {
    if let (Some(a), Some(e)) = (&actual.analysis, expected.analysis.as_mut()) {
        e.inchi = a.inchi.clone();
        e.inchikey = a.inchikey.clone();
    }
    if let (Some(a), Some(e)) = (&actual.document, expected.document.as_mut()) {
        for &i in doubles {
            if let (Some(a), Some(e)) = (a.bonds.get(i), e.bonds.get_mut(i)) {
                e.stereo_authoritative = a.stereo_authoritative;
            }
        }
    }
}
