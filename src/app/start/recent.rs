//! Five local file identities; previews are bounded, disposable and never reopened.
use super::{Action, Message};
use iced::{Task, widget::image};
use reshiki::engine::LocalEngine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

const COUNT: usize = 5;
const STORE_LIMIT: u64 = 128 * 1024;
const PREVIEW_LIMIT: u64 = 256 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    version: u32,
    paths: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    resolved: BTreeMap<PathBuf, PathBuf>,
}

pub(super) struct Entry {
    pub path: PathBuf,
    pub preview: Option<image::Handle>,
    resolved: Option<PathBuf>,
    asked: bool,
    generation: u64,
}

#[derive(Default)]
pub(super) struct Recent {
    pub entries: Vec<Entry>,
    store: Option<PathBuf>,
    generation: u64,
}

impl Recent {
    pub(super) fn load() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        let store = reshiki::compatibility::data_directory_location()
            .ok()
            .map(|location| location.path.join("recent-files.json"));
        store.map_or_else(Self::default, Self::load_from)
    }

    fn load_from(store: PathBuf) -> Self {
        use std::io::Read;
        let existed = store.exists();
        let record = (|| {
            let mut bytes = Vec::new();
            std::fs::File::open(&store)
                .ok()?
                .take(STORE_LIMIT + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            (bytes.len() <= STORE_LIMIT as usize).then_some(())?;
            let record: Record = serde_json::from_slice(&bytes).ok()?;
            (record.version == 1).then_some(record)
        })();
        let mut seen = HashSet::new();
        let writable = !existed || record.is_some();
        let entries = record.map_or_else(Vec::new, |record| {
            record
                .paths
                .into_iter()
                .filter(|path| path.is_absolute() && seen.insert(path.clone()))
                .take(COUNT)
                .map(|path| Entry {
                    resolved: record
                        .resolved
                        .get(&path)
                        .filter(|p| p.is_absolute())
                        .cloned(),
                    path,
                    preview: None,
                    asked: false,
                    generation: 0,
                })
                .collect()
        });
        Self {
            entries,
            store: writable.then_some(store),
            generation: 0,
        }
    }

    pub(super) fn contains(&self, path: &Path) -> bool {
        self.entries.iter().any(|entry| entry.path == path)
    }

    pub(super) fn remember(&mut self, path: &Path) {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            let Ok(path) = std::path::absolute(path) else {
                return;
            };
            path
        };
        let Ok(canonical_path) = path.canonicalize() else {
            return;
        };
        if !canonical_path.is_file() {
            return;
        }
        // Reopen with the original spelling for GitHub file classification.
        // Resolve only the file just used. Old paths can be on offline shares.
        self.entries.retain(|entry| {
            entry.path != path
                && entry.path != canonical_path
                && entry.resolved.as_ref() != Some(&canonical_path)
        });
        self.generation = self.generation.wrapping_add(1);
        self.entries.insert(
            0,
            Entry {
                path,
                preview: None,
                resolved: Some(canonical_path),
                asked: false,
                generation: self.generation,
            },
        );
        self.entries.truncate(COUNT);
        if let Some(store) = &self.store {
            let record = Record {
                version: 1,
                paths: self
                    .entries
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect(),
                resolved: self
                    .entries
                    .iter()
                    .filter_map(|entry| entry.resolved.clone().map(|key| (entry.path.clone(), key)))
                    .collect(),
            };
            if let (Some(parent), Ok(bytes)) = (store.parent(), serde_json::to_vec(&record)) {
                if bytes.len() > STORE_LIMIT as usize {
                    return;
                }
                let _ = reshiki::storage::create_dir_all(parent).and_then(|()| {
                    reshiki::storage::write_atomic(store, &bytes).map_err(std::io::Error::other)
                });
            }
        }
    }

    pub(super) fn previewed(
        &mut self,
        path: &Path,
        generation: u64,
        preview: Option<image::Handle>,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.path == path && entry.generation == generation)
        {
            entry.preview = preview;
        }
    }

    pub(super) fn previews(&mut self, engine: LocalEngine) -> Task<Message> {
        Task::batch(
            self.entries
                .iter_mut()
                .filter(|entry| !entry.asked)
                .map(|entry| {
                    entry.asked = true;
                    let path = entry.path.clone();
                    let source = path.clone();
                    let generation = entry.generation;
                    let engine = engine.clone();
                    Task::perform(
                        async move { preview(engine, source).await },
                        move |preview| {
                            Message::Start(Action::Preview(path.clone(), generation, preview))
                        },
                    )
                }),
        )
    }
}

async fn preview(engine: LocalEngine, path: PathBuf) -> Option<image::Handle> {
    let files = crate::app::files::read_paths(vec![path], |_| PREVIEW_LIMIT)
        .await
        .ok()?;
    let file = files.into_iter().next()?;
    if file.contents.len() > PREVIEW_LIMIT as usize {
        return None;
    }
    let first = crate::app::import::records(file)
        .into_iter()
        .take(1)
        .collect();
    let loaded = crate::app::import::load(engine, first, Default::default(), Default::default())
        .await
        .ok()?;
    let drawing = loaded.drawings.into_iter().next()?;
    if drawing.object_ids().take(1025).count() > 1024
        || drawing
            .graphics
            .iter()
            .any(|graphic| graphic.picture.is_some())
    {
        return None;
    }
    reshiki::task::spawn_blocking(move || {
        let preview = reshiki::export::png_preview(
            &drawing,
            reshiki::export::Background::Color([255; 3]),
            (80, 56),
            None,
        )
        .ok()?;
        Some(image::Handle::from_rgba(
            preview.pixels.0,
            preview.pixels.1,
            preview.rgba,
        ))
    })
    .await
    .ok()?
}

#[cfg(test)]
mod tests;
