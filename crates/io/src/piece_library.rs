//! Libraries of equipment pieces: My pieces in the data folder, lab folders
//! the user links, and the `.reshiki-piece` and `.reshiki-pieces` files that
//! carry pieces between people. Library changes never touch an open drawing:
//! drawings keep the definitions they embed.
//!
//! A library file is named after its piece's full identity, so two windows
//! that save the same piece write the same bytes, and a piece edited without
//! a new revision lands beside the old one instead of replacing it.
use crate::equipment::{
    format::{self, Definition},
    placed::key,
};
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(target_arch = "wasm32")]
use web_time::{SystemTime, UNIX_EPOCH};

pub const PIECE_EXTENSION: &str = "reshiki-piece";
pub const COLLECTION_EXTENSION: &str = "reshiki-pieces";
pub const COLLECTION_FORMAT: &str = "reshiki-pieces";
pub const MAX_COLLECTION_BYTES: usize = 32 << 20;
pub const MAX_PIECES: usize = 2048;
/// A linked folder is read at most this many folders deep …
pub const MAX_DEPTH: usize = 4;
/// … and up to this many files.
pub const MAX_FILES: usize = 5000;

/// A `.reshiki-pieces` file: a lab's set of pieces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collection {
    format: String,
    format_version: u32,
    library: About,
    pieces: Vec<serde_json::Value>,
}

/// Who made a collection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct About {
    pub id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
}

/// A file a library could not use, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub path: PathBuf,
    pub error: String,
}

/// The pieces found in one folder.
#[derive(Debug, Clone, Default)]
pub struct Folder {
    pub pieces: Vec<Arc<Definition>>,
    pub problems: Vec<Problem>,
}

/// The pieces in a `.reshiki-piece` or `.reshiki-pieces` file, each checked.
/// One bad piece refuses the whole file, so an import is all or nothing.
pub fn read(bytes: &[u8]) -> Result<Vec<Definition>, String> {
    if bytes.len() > MAX_COLLECTION_BYTES {
        return Err("The file is larger than 32 MB.".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "The file is not UTF-8 text.")?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("The file is not valid JSON: {e}."))?;
    if value.get("format").and_then(|v| v.as_str()) != Some(COLLECTION_FORMAT) {
        if bytes.len() > crate::equipment::validate::MAX_FILE_BYTES {
            return Err("The piece file is larger than 1 MB.".into());
        }
        return Ok(vec![piece(text)?]);
    }
    let collection: Collection = serde_json::from_value(value)
        .map_err(|e| format!("The piece library is not valid: {e}."))?;
    if collection.format_version > format::FORMAT_VERSION {
        return Err(format!(
            "This piece library was made with a newer version of ReShiki (format {}). Update ReShiki to open it.",
            collection.format_version
        ));
    }
    if collection.pieces.len() > MAX_PIECES {
        return Err(format!(
            "The library holds {} pieces; the limit is {MAX_PIECES}.",
            collection.pieces.len()
        ));
    }
    (collection.pieces.iter())
        .enumerate()
        .map(|(i, p)| piece(&p.to_string()).map_err(|e| format!("Piece {}: {e}", i + 1)))
        .collect()
}

fn piece(text: &str) -> Result<Definition, String> {
    let d = format::parse(text)?;
    if d.id.starts_with("reshiki/") {
        return Err(format!(
            "{}: the reshiki/ namespace is kept for built-in pieces.",
            d.id
        ));
    }
    Ok(d)
}

/// One piece as a `.reshiki-piece` file.
pub fn piece_bytes(d: &Definition) -> Vec<u8> {
    // A definition holds only strings, numbers and maps with string keys.
    serde_json::to_vec_pretty(d).unwrap_or_default()
}

/// Pieces as a `.reshiki-pieces` file.
pub fn collection_bytes(about: &About, pieces: &[Arc<Definition>]) -> Vec<u8> {
    let collection = Collection {
        format: COLLECTION_FORMAT.into(),
        format_version: format::PREVIEW_VERSION,
        library: about.clone(),
        pieces: (pieces.iter())
            .map(|d| serde_json::to_value(&**d).unwrap_or_default())
            .collect(),
    };
    serde_json::to_vec_pretty(&collection).unwrap_or_default()
}

/// Every piece file under `root`: regular files only, never through a
/// symbolic link, at most [`MAX_DEPTH`] folders deep and [`MAX_FILES`] files.
/// Hidden folders, such as `.deleted` and `.git`, are skipped. A file that
/// cannot be used is listed with its reason; the rest still load.
pub fn scan(root: &Path) -> Folder {
    let mut folder = Folder::default();
    let mut files = 0;
    let mut pending = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(mut entries) = crate::storage::read_dir(&dir) else {
            if depth == 0 {
                folder.problems.push(Problem {
                    path: dir,
                    error: "The folder cannot be read.".into(),
                });
            }
            continue;
        };
        entries.sort();
        for path in entries {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            match kind(&path) {
                Kind::Folder if depth < MAX_DEPTH => pending.push((path, depth + 1)),
                Kind::File if is_piece_file(&path) => {
                    files += 1;
                    if files > MAX_FILES {
                        folder.problems.push(Problem {
                            path: root.to_path_buf(),
                            error: format!("Only the first {MAX_FILES} files were read."),
                        });
                        return folder;
                    }
                    match crate::storage::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| read(&bytes))
                    {
                        Ok(pieces) => folder.pieces.extend(pieces.into_iter().map(Arc::new)),
                        Err(error) => folder.problems.push(Problem { path, error }),
                    }
                }
                _ => {}
            }
        }
    }
    folder
}

fn is_piece_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|x| x == PIECE_EXTENSION || x == COLLECTION_EXTENSION)
}

enum Kind {
    File,
    Folder,
    Other,
}

#[cfg(not(target_arch = "wasm32"))]
fn kind(path: &Path) -> Kind {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => Kind::File,
        Ok(m) if m.is_dir() => Kind::Folder,
        _ => Kind::Other,
    }
}

/// Browser storage is flat files.
#[cfg(target_arch = "wasm32")]
fn kind(_: &Path) -> Kind {
    Kind::File
}

/// The file name a library gives a piece: its whole identity.
pub fn file_name(d: &Definition) -> String {
    let key = key(d);
    format!("{}.{PIECE_EXTENSION}", key.replace(['/', '#'], "_"))
}

/// A library change that can be undone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    /// Files written.
    pub added: Vec<PathBuf>,
    /// Files moved into the archive: where each was, where it is now.
    pub archived: Vec<(PathBuf, PathBuf)>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.archived.is_empty()
    }

    /// Put the library back as it was before the change. Added files go to
    /// the archive rather than away, so an undo loses nothing either.
    pub fn undo(&self, dir: &Path) -> Result<Change, String> {
        let mut undone = archive(dir, &self.added)?;
        for (was, now) in &self.archived {
            crate::storage::rename(now, was).map_err(|e| e.to_string())?;
            undone.added.push(was.clone());
        }
        Ok(undone)
    }
}

/// Add pieces to the library folder `dir`. A piece the folder already holds,
/// by full identity, is skipped, so importing the same file twice adds
/// nothing the second time.
pub fn add(dir: &Path, pieces: &[Definition]) -> Result<Change, String> {
    crate::storage::create_dir_all(dir).map_err(|e| e.to_string())?;
    let held: Vec<String> = scan(dir).pieces.iter().map(|d| key(d)).collect();
    let mut change = Change::default();
    for d in pieces {
        if held.contains(&key(d)) {
            continue;
        }
        let path = dir.join(file_name(d));
        if change.added.contains(&path) {
            continue;
        }
        crate::storage::write_atomic(&path, &piece_bytes(d))?;
        change.added.push(path);
    }
    Ok(change)
}

/// Move the files in `dir` that hold the piece with this key into the
/// archive, `.deleted/<time>/`, where they can be recovered.
pub fn remove(dir: &Path, piece_key: &str) -> Result<Change, String> {
    let paths: Vec<_> = crate::storage::read_dir(dir)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == PIECE_EXTENSION))
        .filter(|p| {
            crate::storage::read(p)
                .ok()
                .and_then(|b| read(&b).ok())
                .is_some_and(|pieces| pieces.iter().any(|d| key(d) == piece_key))
        })
        .collect();
    archive(dir, &paths)
}

fn archive(dir: &Path, paths: &[PathBuf]) -> Result<Change, String> {
    let mut change = Change::default();
    if paths.is_empty() {
        return Ok(change);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let to = dir.join(".deleted").join(stamp.to_string());
    crate::storage::create_dir_all(&to).map_err(|e| e.to_string())?;
    for path in paths {
        let Some(name) = path.file_name() else {
            continue;
        };
        let dest = to.join(name);
        if let Err(e) = crate::storage::rename(path, &dest) {
            for (was, now) in change.archived.iter().rev() {
                let _ = crate::storage::rename(now, was);
            }
            return Err(format!(
                "Could not move {} to the archive: {e}",
                name.display()
            ));
        }
        change.archived.push((path.clone(), dest));
    }
    Ok(change)
}

/// My pieces: the `pieces` folder in ReShiki's data folder.
pub fn my_pieces() -> Result<PathBuf, String> {
    Ok(crate::compatibility::data_directory_location()?
        .path
        .join("pieces"))
}

/// The lab folders the user linked, kept beside My pieces.
fn linked_list() -> Result<PathBuf, String> {
    Ok(crate::compatibility::data_directory_location()?
        .path
        .join("piece-libraries.json"))
}

pub fn linked() -> Vec<PathBuf> {
    linked_list()
        .ok()
        .and_then(|p| crate::storage::read(&p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn set_linked(folders: &[PathBuf]) -> Result<(), String> {
    let path = linked_list()?;
    if let Some(dir) = path.parent() {
        crate::storage::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::storage::write_atomic(
        &path,
        &serde_json::to_vec_pretty(folders).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests;
