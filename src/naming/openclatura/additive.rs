//! Indicated-hydrogen limits for supported retained monocyclic parents.
use super::assembly_parts::AssemblyParts;

/// The non-fusion branch of upstream `parent_indicated_hydrogen_quota`.
pub fn parent_indicated_hydrogen_quota(parts: &AssemblyParts) -> Option<usize> {
    parts
        .retained_parent_metadata
        .as_ref()
        .filter(|metadata| metadata.mancude_double_bonds > 0)
        .map(|metadata| metadata.indicated_hydrogen_count)
}
