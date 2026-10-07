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
    /// relative path, or leaving the root) is dropped. Containment is
    /// lexical, so a spelling with `..` never matches: `a/../b` lies in `b`,
    /// not `a`.
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
        if limits
            .iter()
            .any(|limit| spellings(root).any(|spelling| within(spelling, limit)))
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

/// The spellings containment is judged by: the canonical path, and the
/// user's spelling unless it has a `..` (a relative command-line folder
/// resolved against cwd), which only canonicalization can place.
fn spellings(root: &Root) -> impl Iterator<Item = &Path> {
    let plain = !root
        .display
        .components()
        .any(|part| part == Component::ParentDir);
    plain
        .then_some(root.display.as_path())
        .into_iter()
        .chain([root.canonical.as_path()])
}

fn keep(kept: &mut Vec<Root>, root: Root) {
    if kept.iter().all(|other| other.canonical != root.canonical) {
        kept.push(root);
    }
}

/// `limit` as a root opened through `root`'s handle, if it lies inside it.
fn sub_root(root: &Root, limit: &Path) -> Option<Root> {
    let (_, rel) = spellings(root).find_map(|spelling| strip(spelling, limit))?;
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
