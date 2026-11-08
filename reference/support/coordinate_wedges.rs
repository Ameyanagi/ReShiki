//! #1045: SMILES import drops input wedge flags before the fresh layout and
//! wedges the molecule again from its stereo state (#885 orientation then
//! applies). The original worker keeps the flags on its new layout, where they
//! can describe other stereo. For such inputs only atom positions and wedge
//! placement come from the actual response. The identifiers must equal the
//! worker's, or else exactly RDKit's reading of the input itself. The worker
//! also Kekulizes with the input wedges still set, which RDKit's Kekulize
//! weighs. Only for the two ZM374979_atropBad1 cases may the bond orders
//! instead equal, as a whole, RDKit's Kekule form of the input with the
//! wedges cleared; partial mixes fail, and every other input keeps the
//! worker's bond orders exactly.
//! Every other field is compared.
use reshiki::engine::Response;

#[derive(serde::Deserialize)]
pub struct CoordinateWedges {
    inchi: String,
    inchikey: String,
    smiles: String,
    /// RDKit's Kekule form with the input wedges cleared: [a, b, order].
    #[serde(default)]
    kekule: Vec<[usize; 3]>,
}

/// The cases whose Kekule form #1045 changes (RDKit source tree).
const KEKULE: [&str; 2] = [
    "source-cx/ZM374979_atropBad1.cxsmi/0",
    "source-cx/ZM374979_atropBad1.cxsmi.expected.cxsmi/0",
];

pub fn adapt(name: &str, input: &CoordinateWedges, actual: &Response, expected: &mut Response) {
    if let (Some(a), Some(e)) = (&actual.document, expected.document.as_mut()) {
        for (a, e) in a.atoms.iter().zip(e.atoms.iter_mut()) {
            e.position = a.position;
        }
        // A Kekule difference is accepted only as a whole: every bond order
        // equals RDKit's Kekule form of the input with the wedges cleared.
        let index: std::collections::HashMap<u64, usize> = a
            .atoms
            .iter()
            .enumerate()
            .map(|(i, atom)| (atom.id, i))
            .collect();
        let kekule = |x: u64, y: u64| {
            let (x, y) = (*index.get(&x)?, *index.get(&y)?);
            input
                .kekule
                .iter()
                .find(|[p, q, _]| (*p, *q) == (x, y) || (*p, *q) == (y, x))
                .map(|[_, _, order]| *order)
        };
        let whole = KEKULE.contains(&name)
            && a.bonds.len() == e.bonds.len()
            && a.bonds.len() == input.kekule.len()
            && a.bonds
                .iter()
                .all(|b| kekule(b.a, b.b) == Some(usize::from(b.order)));
        if whole {
            for (a, e) in a.bonds.iter().zip(e.bonds.iter_mut()) {
                e.order = a.order;
            }
        }
        for (a, e) in a.bonds.iter().zip(e.bonds.iter_mut()) {
            let wedged = |display: &str| matches!(display, "wedge" | "hash");
            let placed = |display: &str| matches!(display, "wedge" | "hash" | "plain");
            if ((a.a, a.b) == (e.a, e.b) || (a.a, a.b) == (e.b, e.a))
                && (wedged(&a.display) || wedged(&e.display))
                && placed(&a.display)
                && placed(&e.display)
            {
                (e.a, e.b, e.display) = (a.a, a.b, a.display.clone());
            }
        }
    }
    // The identifiers stay the worker's unless the actual ones are exactly
    // RDKit's reading of the input, as for BMS-986142 and Sotorasib.
    if let (Some(a), Some(e)) = (&actual.analysis, expected.analysis.as_mut())
        && a.inchi != e.inchi
        && (a.inchi.as_str(), a.inchikey.as_str(), a.smiles.as_str())
            == (
                input.inchi.as_str(),
                input.inchikey.as_str(),
                input.smiles.as_str(),
            )
    {
        e.inchi = input.inchi.clone();
        e.inchikey = input.inchikey.clone();
        e.smiles = input.smiles.clone();
    }
}
