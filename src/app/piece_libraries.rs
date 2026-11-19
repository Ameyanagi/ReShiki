//! Equipment libraries in the Equipment browser: My pieces, linked lab
//! folders, import and export of piece files, and updates to a newer
//! revision. Library changes never touch the open drawing; updating placed
//! pieces is an ordinary edit with one Undo step.
use super::equipment::{Action, name};
use super::workspace::{command, horizontal_line, hover_hint, muted_text, section};
use super::{App, Message};
use iced::widget::{column, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::equipment::{builtin, format::Definition, placed::key, revise};
use reshiki::piece_library::{self as library, Change, Folder};
use std::{path::PathBuf, sync::Arc};

/// Where a piece in the browser comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    BuiltIn,
    Mine,
    /// A linked lab folder, by its place in the list.
    Lab(usize),
}

/// A piece the browser offers.
pub struct Entry {
    pub definition: Arc<Definition>,
    pub source: Source,
    /// `id@revision#hash`, worked out once when the libraries are read.
    pub key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shelf {
    pub source: Option<Source>,
    pub name: String,
}
impl std::fmt::Display for Shelf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Default)]
pub struct Libraries {
    pub mine: Folder,
    pub labs: Vec<(PathBuf, Folder)>,
    /// The last library change, while it can still be undone.
    pub undo: Option<Change>,
    /// Every piece once, by key: built-in pieces first, then My pieces,
    /// then each lab in the order linked.
    pub entries: Vec<Entry>,
}

impl Libraries {
    pub fn load() -> Self {
        let mut libraries = Self::default();
        if cfg!(test) {
            libraries.index();
        } else {
            libraries.reload(library::linked());
        }
        libraries
    }

    pub fn reload(&mut self, labs: Vec<PathBuf>) {
        if let Ok(dir) = library::my_pieces() {
            self.mine = library::scan(&dir);
            // A missing My pieces folder is simply empty.
            if self.mine.pieces.is_empty() {
                self.mine.problems.retain(|p| p.path != dir);
            }
        }
        self.labs = labs
            .into_iter()
            .map(|root| {
                let folder = library::scan(&root);
                (root, folder)
            })
            .collect();
        self.index();
    }

    pub fn name(&self, source: Source) -> String {
        match source {
            Source::BuiltIn => "Built-in".into(),
            Source::Mine => "My pieces".into(),
            Source::Lab(i) => self.labs.get(i).map_or_else(String::new, |(root, _)| {
                root.file_name().map_or_else(
                    || root.display().to_string(),
                    |n| n.to_string_lossy().into(),
                )
            }),
        }
    }

    /// Rebuild [`Self::entries`] after the folders changed.
    pub fn index(&mut self) {
        let mut out: Vec<Entry> = Vec::new();
        let sources = std::iter::once((Source::BuiltIn, builtin::catalogue()))
            .chain(std::iter::once((Source::Mine, &self.mine.pieces[..])))
            .chain(
                (self.labs.iter().enumerate()).map(|(i, (_, f))| (Source::Lab(i), &f.pieces[..])),
            );
        for (source, pieces) in sources {
            for d in pieces {
                let k = key(d);
                if !out.iter().any(|e| e.key == k) {
                    out.push(Entry {
                        definition: d.clone(),
                        source,
                        key: k,
                    });
                }
            }
        }
        self.entries = out;
    }

    /// The highest revision of `d` the libraries hold, if it is newer, and
    /// where it is.
    pub fn newer(&self, d: &Definition) -> Option<(Arc<Definition>, Source)> {
        // On a tie the first library wins: max_by_key keeps the last.
        (self.entries.iter().rev())
            .filter(|e| e.definition.id == d.id && e.definition.revision > d.revision)
            .max_by_key(|e| e.definition.revision)
            .map(|e| (e.definition.clone(), e.source))
    }

    /// The shelves the browser can show.
    pub fn shelves(&self) -> Vec<Shelf> {
        let mut out = vec![Shelf {
            source: None,
            name: "All libraries".into(),
        }];
        let mut sources = vec![Source::BuiltIn, Source::Mine];
        sources.extend((0..self.labs.len()).map(Source::Lab));
        out.extend(sources.into_iter().map(|s| Shelf {
            source: Some(s),
            name: self.name(s),
        }));
        out
    }
}

/// The pieces a file chosen for import may hold.
fn import_limit(_: &std::path::Path) -> u64 {
    library::MAX_COLLECTION_BYTES as u64
}

fn count(n: usize) -> String {
    if n == 1 {
        "1 piece".into()
    } else {
        format!("{n} pieces")
    }
}

impl App {
    fn library_done(
        &mut self,
        result: Result<Change, String>,
        done: impl FnOnce(&Change) -> String,
    ) {
        match result {
            Ok(change) => {
                self.status = done(&change);
                self.error = false;
                if !change.is_empty() {
                    self.equipment.libraries.undo = Some(change);
                }
            }
            Err(e) => {
                self.status = e;
                self.error = true;
            }
        }
        let labs = self.lab_roots();
        self.reload_libraries(labs);
    }

    /// Read the libraries again. An armed piece stays armed as the entry
    /// that now offers it, or is disarmed when none does.
    fn reload_libraries(&mut self, labs: Vec<PathBuf>) {
        let libraries = &mut self.equipment.libraries;
        libraries.reload(labs);
        if let Some(armed) = &self.equipment.armed {
            let k = key(armed);
            self.equipment.armed = (libraries.entries.iter())
                .find(|e| e.key == k)
                .map(|e| e.definition.clone());
        }
    }

    fn lab_roots(&self) -> Vec<PathBuf> {
        (self.equipment.libraries.labs.iter())
            .map(|(root, _)| root.clone())
            .collect()
    }

    /// Change the saved list of linked folders, starting from what is saved
    /// now, as another window may have changed it.
    fn set_labs(&mut self, change: impl FnOnce(&mut Vec<PathBuf>)) -> bool {
        let mut labs = library::linked();
        change(&mut labs);
        if let Err(e) = library::set_linked(&labs) {
            self.status = e;
            self.error = true;
            return false;
        }
        self.equipment.shelf = None;
        self.reload_libraries(labs);
        true
    }

    pub(super) fn library_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Import => {
                return Task::perform(
                    async {
                        let filter = (
                            "ReShiki pieces",
                            &[library::PIECE_EXTENSION, library::COLLECTION_EXTENSION][..],
                        );
                        let files = super::files::pick(
                            "Import equipment pieces",
                            Some(filter),
                            true,
                            import_limit,
                        )
                        .await?;
                        let mut pieces = Vec::new();
                        for file in files {
                            let name = file
                                .path
                                .file_name()
                                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                            pieces.extend(
                                library::read(&file.contents)
                                    .map_err(|e| format!("{name}: {e}"))?,
                            );
                        }
                        Ok(pieces)
                    },
                    |result| Message::Equipment(Action::Imported(result)),
                );
            }
            Action::Imported(result) => {
                let result = result.and_then(|pieces| {
                    let n = pieces.len();
                    library::add(&library::my_pieces()?, &pieces).map(|c| (n, c))
                });
                let total = result.as_ref().map_or(0, |(n, _)| *n);
                if result.as_ref().is_ok_and(|(n, _)| *n == 0) {
                    return Task::none();
                }
                self.library_done(result.map(|(_, c)| c), |c| match c.added.len() {
                    0 if total == 1 => "This piece is already in My pieces".into(),
                    0 => "These pieces are already in My pieces".into(),
                    n => format!("Added {} to My pieces", count(n)),
                });
            }
            Action::ExportMine => {
                let pieces = self.equipment.libraries.mine.pieces.clone();
                if pieces.is_empty() {
                    self.status = "My pieces is empty".into();
                    self.error = false;
                    return Task::none();
                }
                if pieces.len() > library::MAX_PIECES {
                    self.status = format!(
                        "A library file holds at most {} pieces; My pieces has {}",
                        library::MAX_PIECES,
                        pieces.len()
                    );
                    self.error = true;
                    return Task::none();
                }
                let about = library::About {
                    id: "my-pieces".into(),
                    names: [("en".to_owned(), "My pieces".to_owned())].into(),
                    ..Default::default()
                };
                let bytes = library::collection_bytes(&about, &pieces);
                if bytes.len() > library::MAX_COLLECTION_BYTES {
                    self.status = "My pieces is larger than a library file can hold (32 MB)".into();
                    self.error = true;
                    return Task::none();
                }
                return save(
                    "Export My pieces",
                    "My pieces",
                    library::COLLECTION_EXTENSION,
                    bytes,
                );
            }
            Action::ExportPiece(k) => {
                let Some(d) = self.piece_by_key(&k) else {
                    return Task::none();
                };
                let bytes = library::piece_bytes(&d);
                return save("Export piece", name(&d), library::PIECE_EXTENSION, bytes);
            }
            Action::Exported(result) => match result {
                Ok(Some(path)) => {
                    self.status = format!("Exported {}", path.display());
                    self.error = false;
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            },
            Action::Keep(k) => {
                let Some(d) = self.piece_by_key(&k) else {
                    return Task::none();
                };
                let result =
                    library::my_pieces().and_then(|dir| library::add(&dir, &[(*d).clone()]));
                let n = name(&d).to_owned();
                self.library_done(result, |_| format!("Added {n} to My pieces"));
            }
            Action::Remove(k) => {
                let Some(d) = self.piece_by_key(&k) else {
                    return Task::none();
                };
                let result = library::my_pieces().and_then(|dir| library::remove(&dir, &k));
                let n = name(&d).to_owned();
                if self.equipment.armed.as_ref().is_some_and(|a| key(a) == k) {
                    self.equipment.armed = None;
                }
                self.library_done(result, |_| {
                    format!("Removed {n} from My pieces · Drawings that use it keep it")
                });
            }
            Action::UndoLibrary => {
                let Some(change) = self.equipment.libraries.undo.clone() else {
                    return Task::none();
                };
                let result = library::my_pieces().and_then(|dir| change.undo(&dir));
                if result.is_ok() {
                    self.equipment.libraries.undo = None;
                }
                self.library_done(result.map(|_| Change::default()), |_| {
                    "Library change undone".into()
                });
            }
            Action::Link => {
                return Task::perform(pick_folder(), |folder| {
                    Message::Equipment(Action::Linked(folder))
                });
            }
            Action::Linked(Some(root)) => {
                // ReShiki writes My pieces; a linked folder is only read.
                if let Ok(mine) = library::my_pieces()
                    && (root.starts_with(&mine) || mine.starts_with(&root))
                {
                    self.status = "That folder holds My pieces. Link a lab folder instead.".into();
                    self.error = true;
                    return Task::none();
                }
                let linked = root.clone();
                if self.set_labs(|labs| {
                    if !labs.contains(&linked) {
                        labs.push(linked);
                    }
                }) {
                    let libraries = &self.equipment.libraries;
                    if let Some((i, (_, folder))) =
                        (libraries.labs.iter().enumerate()).find(|(_, (r, _))| *r == root)
                    {
                        self.status = format!(
                            "Linked {} · {}",
                            libraries.name(Source::Lab(i)),
                            count(folder.pieces.len())
                        );
                        self.error = false;
                    }
                }
            }
            Action::Linked(None) => {}
            Action::Unlink(i) => {
                let Some((root, _)) = self.equipment.libraries.labs.get(i) else {
                    return Task::none();
                };
                let root = root.clone();
                let name = self.equipment.libraries.name(Source::Lab(i));
                if self.set_labs(|labs| labs.retain(|r| *r != root)) {
                    self.status = format!("Unlinked {name}");
                    self.error = false;
                }
            }
            Action::Reload => {
                self.reload_libraries(library::linked());
                self.status = "Libraries reloaded".into();
                self.error = false;
            }
            Action::Shelf(shelf) => self.equipment.shelf = shelf.source,
            Action::Update { ids, key: k } => {
                let Some(next) = self.piece_by_key(&k) else {
                    return Task::none();
                };
                let before = self.tab.doc.clone();
                let outcome = revise::update(&mut self.tab.doc, &ids, &next);
                self.changed(before);
                if !self.error {
                    self.status = format!(
                        "Updated to revision {}{}",
                        next.revision,
                        losses(&outcome, &next)
                    );
                }
                return super::focus_drawing();
            }
            _ => {}
        }
        Task::none()
    }

    /// A definition by key: a library's, or one the drawing embeds.
    fn piece_by_key(&self, k: &str) -> Option<Arc<Definition>> {
        (self.equipment.libraries.entries.iter())
            .filter(|e| e.key == k)
            .map(|e| e.definition.clone())
            .chain((self.tab.doc.graphics.iter()).filter_map(|g| g.piece.as_ref()?.linked.clone()))
            .find(|d| key(d) == k)
    }

    /// Libraries at the foot of the Equipment browser.
    pub(super) fn libraries_section(&self) -> Element<'_, Message> {
        let libraries = &self.equipment.libraries;
        let act = |a: Action| Message::Equipment(a);
        let mut body = column![
            horizontal_line(),
            section("Libraries"),
            row![
                text(format!(
                    "My pieces · {}",
                    count(libraries.mine.pieces.len())
                ))
                .size(12)
                .width(Length::Fill),
                command("Import…", act(Action::Import)),
                command("Export…", act(Action::ExportMine)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(8);
        for (i, (root, folder)) in libraries.labs.iter().enumerate() {
            let mut about = column![
                text(format!(
                    "{} · {}",
                    libraries.name(Source::Lab(i)),
                    count(folder.pieces.len())
                ))
                .size(12)
            ];
            if !folder.problems.is_empty() {
                let list: Vec<String> = (folder.problems.iter())
                    .map(|p| format!("{}: {}", p.path.display(), p.error))
                    .collect();
                about = about.push(hover_hint(
                    text(match folder.problems.len() {
                        1 => "1 file could not be read".to_owned(),
                        n => format!("{n} files could not be read"),
                    })
                    .size(11)
                    .style(muted_text),
                    list.join("\n"),
                    tooltip::Position::Left,
                ));
            }
            body = body.push(hover_hint(
                row![
                    about.width(Length::Fill),
                    command("Unlink", act(Action::Unlink(i))),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                root.display().to_string(),
                tooltip::Position::Left,
            ));
        }
        let mut buttons = row![].spacing(6);
        if cfg!(not(target_arch = "wasm32")) {
            buttons = buttons.push(hover_hint(
                command("Link lab folder…", act(Action::Link)),
                "Use pieces from a shared folder. ReShiki only reads it.",
                tooltip::Position::Left,
            ));
        }
        buttons = buttons.push(command("Reload", act(Action::Reload)));
        if libraries.undo.is_some() {
            buttons = buttons.push(command("Undo library change", act(Action::UndoLibrary)));
        }
        body.push(buttons.wrap().vertical_spacing(6)).into()
    }

    /// For one kind of selected piece: keep it, export it, or update it to
    /// a newer revision a library holds.
    pub(super) fn piece_library_rows(&self, d: &Arc<Definition>) -> Element<'_, Message> {
        let libraries = &self.equipment.libraries;
        let k = key(d);
        let in_library = libraries.entries.iter().any(|e| e.key == k);
        let mut body = column![].spacing(6);
        if let Some((next, source)) = libraries.newer(d) {
            let ids: Vec<u64> = (self.tab.doc.graphics.iter())
                .filter(|g| g.piece.as_ref().is_some_and(|p| p.definition == k))
                .map(|g| g.id)
                .collect();
            let selected: Vec<u64> = (ids.iter().copied())
                .filter(|id| self.tab.selected.contains(id))
                .collect();
            let outcome = revise::preview(&self.tab.doc, &selected, &next);
            body = body.push(
                text(format!(
                    "Revision {} is in {} (this drawing uses {})",
                    next.revision,
                    libraries.name(source),
                    d.revision
                ))
                .size(12),
            );
            let note = losses(&outcome, &next);
            if !note.is_empty() {
                body = body.push(
                    text(note.trim_start_matches(" · ").to_owned())
                        .size(11)
                        .style(muted_text),
                );
            }
            let mut buttons = row![command(
                "Update",
                Message::Equipment(Action::Update {
                    ids: selected,
                    key: key(&next),
                })
            )]
            .spacing(6);
            if ids.len() > 1 {
                buttons = buttons.push(command(
                    "Update all in drawing",
                    Message::Equipment(Action::Update {
                        ids,
                        key: key(&next),
                    }),
                ));
            }
            body = body.push(buttons);
        }
        let mut buttons = row![].spacing(6);
        if !in_library && !d.id.starts_with("reshiki/") {
            buttons = buttons.push(hover_hint(
                command(
                    "Add to My pieces",
                    Message::Equipment(Action::Keep(k.clone())),
                ),
                "Keep this piece to place it in other drawings",
                tooltip::Position::Left,
            ));
        }
        buttons = buttons.push(command(
            "Export piece…",
            Message::Equipment(Action::ExportPiece(k)),
        ));
        body.push(buttons).into()
    }
}

/// What an update releases and resets, for the status line.
fn losses(outcome: &revise::Outcome, next: &Definition) -> String {
    let mut out = String::new();
    match outcome.released.len() {
        0 => {}
        1 => out.push_str(" · Releases 1 seat"),
        n => out.push_str(&format!(" · Releases {n} seats")),
    }
    if !outcome.reset.is_empty() {
        let names: Vec<_> = (outcome.reset.iter())
            .map(|id| {
                next.params
                    .iter()
                    .find(|p| p.id() == id)
                    .and_then(|p| p.names().get("en").cloned())
                    .unwrap_or_else(|| id.replace('_', " "))
            })
            .collect();
        out.push_str(&format!(" · Resets {}", names.join(", ")));
    }
    out
}

fn save(title: &'static str, name: &str, extension: &'static str, bytes: Vec<u8>) -> Task<Message> {
    let name = name.to_owned();
    Task::perform(
        async move { super::files::save_bytes(title, &name, extension, &bytes).await },
        |result| Message::Equipment(Action::Exported(result)),
    )
}

#[cfg(not(target_arch = "wasm32"))]
async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Link lab folder")
        .pick_folder()
        .await
        .map(|f| f.path().to_path_buf())
}

#[cfg(target_arch = "wasm32")]
async fn pick_folder() -> Option<PathBuf> {
    None
}
