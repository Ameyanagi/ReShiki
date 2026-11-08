//! #909/#923: a double bond the input leaves unspecified stays unspecified
//! (`stereo_authoritative` with no stereo) instead of taking E/Z from the new
//! layout, and the identifiers withhold that layout. The reference script
//! lists, from the input alone, which bonds the input leaves open and RDKit's
//! identifiers with those bonds unspecified; the original worker instead reads
//! E/Z from its drawn layout. A bond the input specifies is never adapted.
use reshiki::engine::Response;

#[derive(serde::Deserialize)]
pub struct UnspecifiedEz {
    bonds: Vec<usize>,
    inchi: String,
    inchikey: String,
    smiles: Option<String>,
}

pub fn adapt(open: &UnspecifiedEz, actual: &Response, expected: &mut Response) {
    let (Some(actual), Some(document)) = (&actual.document, expected.document.as_mut()) else {
        return;
    };
    let mut withheld = false;
    for &i in &open.bonds {
        if let (Some(a), Some(e)) = (actual.bonds.get(i), document.bonds.get_mut(i))
            && a.stereo_authoritative
            && a.stereo.is_none()
            && !e.stereo_authoritative
        {
            e.stereo_authoritative = true;
            withheld = true;
        }
    }
    let Some(analysis) = expected.analysis.as_mut() else {
        return;
    };
    if withheld && !analysis.inchi.is_empty() {
        analysis.inchi = open.inchi.clone();
        analysis.inchikey = open.inchikey.clone();
    }
    // SMILES leaves every input-unspecified bond unspecified.
    if let Some(smiles) = &open.smiles
        && !open.bonds.is_empty()
        && !analysis.smiles.is_empty()
    {
        analysis.smiles = smiles.clone();
    }
}
