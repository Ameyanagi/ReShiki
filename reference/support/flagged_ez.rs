//! #1042: where a wU/wD flag sits on a `/` or `\` and cannot mean a wedge,
//! SMILES import keeps the E/Z the direction states. RDKit drops it, and the
//! original worker's InChI then reads E/Z from its own depiction. Live
//! comparisons check only acceptance for these inputs; the kept E/Z has its
//! own native_import test, and the smiles_read suite compares
//! `read_dropping_flagged_ez` with RDKit for every input.
use reshiki::chemistry::smiles;

/// SMILES whose prepared molecule differs from RDKit's reading only because
/// import keeps the E/Z under a wU/wD flag.
pub fn flagged(format: &str, text: &str) -> bool {
    let stereo = |imported: smiles::Imported| serde_json::to_value(imported.prepared).ok();
    format == "smiles"
        && match (smiles::read(text), smiles::read_dropping_flagged_ez(text)) {
            (Ok(kept), Ok(dropped)) => stereo(kept) != stereo(dropped),
            _ => false,
        }
}
