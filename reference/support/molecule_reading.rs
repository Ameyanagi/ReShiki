//! #1043: the original worker's reaction import keeps E/Z from slashes that
//! RDKit itself reads as unknown (a CX `_UnknownStereo` atom property on a
//! double-bond atom) or as unspecified (a slash bond not adjacent to the
//! double bond); RDKit's molecule and reaction readers both give no E/Z.
//! ReShiki follows RDKit's reading. For reaction SMILES the identifiers must
//! equal the worker's, or else exactly RDKit's molecule reading of the same
//! participants; only then do the double bonds RDKit leaves without E/Z take
//! `stereo_authoritative` from the actual response.
use reshiki::engine::Response;

#[derive(serde::Deserialize)]
pub struct MoleculeReading {
    bonds: Vec<usize>,
    inchi: String,
    inchikey: String,
    smiles: String,
}

pub fn adapt(reading: &MoleculeReading, actual: &Response, expected: &mut Response) {
    let (Some(a), Some(e)) = (&actual.analysis, expected.analysis.as_mut()) else {
        return;
    };
    if a.inchi == e.inchi
        || (a.inchi.as_str(), a.inchikey.as_str(), a.smiles.as_str())
            != (
                reading.inchi.as_str(),
                reading.inchikey.as_str(),
                reading.smiles.as_str(),
            )
    {
        return;
    }
    e.inchi = reading.inchi.clone();
    e.inchikey = reading.inchikey.clone();
    e.smiles = reading.smiles.clone();
    if let (Some(a), Some(e)) = (&actual.document, expected.document.as_mut()) {
        for &i in &reading.bonds {
            if let (Some(a), Some(e)) = (a.bonds.get(i), e.bonds.get_mut(i)) {
                e.stereo_authoritative = a.stereo_authoritative;
            }
        }
    }
}
