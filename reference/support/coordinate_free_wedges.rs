//! #1044: without coordinates the @/@@ tags state the configuration and SMILES
//! import draws wedges from them, while the original worker reads CX wedge
//! flags on its own depiction. Live comparisons check only acceptance for these
//! inputs; the tag result has its own native_import test. Reaction SMILES keep
//! the worker's behaviour (#1048), as do inputs with wavy flags (#1049).
use reshiki::chemistry::{kekulize::Direction, smiles};

/// SMILES with CX wedge or hash flags, no wavy flags and no coordinates.
pub fn flagged(format: &str, text: &str) -> bool {
    format == "smiles"
        && smiles::read(text).is_ok_and(|imported| {
            let directions = &imported.prepared.state.directions;
            imported.conformers.is_empty()
                && !directions.contains(&Direction::Unknown)
                && directions
                    .iter()
                    .any(|d| matches!(d, Direction::Wedge | Direction::Hash))
        })
}
