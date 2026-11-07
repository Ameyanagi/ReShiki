//! #892 clears an allene winding (chiral tag 5) from an atom that is not a
//! supported allene centre during chirality cleanup; RDKit keeps the raw tag.
use reshiki::chemistry::{graph::Graph, ranking::Metadata, stereo::allene};

fn unsupported(graph: &Graph, metadata: &Metadata) -> impl Iterator<Item = usize> {
    metadata.atoms.iter().enumerate().filter_map(|(i, atom)| {
        (atom.chiral_tag == 5 && !allene::supported(graph, i, 5, atom.chiral_permutation))
            .then_some(i)
    })
}

/// Whether the input has such a tag. Cleanup also updates its stereo groups,
/// so the whole case differs.
pub fn present(graph: &Graph, metadata: &Metadata) -> bool {
    unsupported(graph, metadata).next().is_some()
}
