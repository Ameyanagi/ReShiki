//! Narrowing a grant set to the folders one client may use.
//!
//! P1 has no callers; this exists for P2's per-client grants.
use super::{
    Grants,
    root::{Root, strip, within},
};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

impl Grants {
    /// The part of these grants that lies inside one of `limits`; the result
    /// is always a subset.
    ///
    /// A root inside a limit is kept whole. A limit inside a root becomes a
    /// root of its own, opened through that root's handle and never with
    /// ambient authority, so it cannot reach outside the root. A limit that
    /// cannot be opened that way (missing, not a folder, not a plain
    /// relative path, or leaving the root) is dropped.
    pub fn narrowed(&self, limits: &[PathBuf]) -> Grants {
        Grants {
            read: narrow(&self.read, limits),
            write: narrow(&self.write, limits),
        }
    }
}

fn narrow(roots: &[Root], limits: &[PathBuf]) -> Vec<Root> {
    let mut kept = Vec::new();
    for root in roots {
        let spellings = [&root.display, &root.canonical];
        if limits
            .iter()
            .any(|limit| spellings.iter().any(|spelling| within(spelling, limit)))
        {
            keep(&mut kept, root.clone());
            continue;
        }
        for limit in limits {
            if let Some(sub) = sub_root(root, limit) {
                keep(&mut kept, sub);
            }
        }
    }
    kept
}

fn keep(kept: &mut Vec<Root>, root: Root) {
    if kept.iter().all(|other| other.canonical != root.canonical) {
        kept.push(root);
    }
}

/// `limit` as a root opened through `root`'s handle, if it lies inside it.
fn sub_root(root: &Root, limit: &Path) -> Option<Root> {
    let (_, rel) = [&root.display, &root.canonical]
        .into_iter()
        .find_map(|spelling| strip(spelling, limit))?;
    if !rel
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let dir = root.dir.open_dir(&rel).ok()?;
    Some(Root {
        display: limit.to_path_buf(),
        canonical: root.canonical.join(&rel),
        dir: Arc::new(dir),
    })
}
