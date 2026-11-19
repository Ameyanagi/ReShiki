//! The Equipment browser: lab apparatus to place on the drawing. Choosing a
//! size arms it, and each click on the drawing places one copy.
use super::piece_libraries::{Entry, Libraries, Shelf, Source};
use super::workspace::{command, control, horizontal_line, hover_hint, muted_text, section};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use crate::canvas::layered::canvas;
use iced::widget::{checkbox, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::{Document, Point};
use reshiki::equipment::{
    format::{Category, Definition, Gender},
    placed,
    solve::{self, JointRef, Offer, Snap},
};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Filter(pub Option<Category>);
impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(category) = self.0 else {
            return f.write_str("All equipment");
        };
        f.write_str(match category {
            Category::Flask => "Flasks",
            Category::Schlenk => "Schlenk ware",
            Category::Condenser => "Condensers",
            Category::Distillation => "Distillation",
            Category::Adapter => "Adapters",
            Category::Addition => "Stoppers and addition",
            Category::Filtration => "Filtration",
            Category::Nmr => "NMR tubes",
            Category::Chromatography => "Chromatography",
            Category::Vessel => "Vessels",
            Category::Heating => "Heating",
            Category::Holding => "Clamps and stands",
            Category::Other => "Other",
        })
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Search(String),
    Category(Filter),
    /// Arm the piece with this key (`id@revision#hash`).
    Arm(String),
    Repeat(bool),
    Close,
    /// Keyboard Connect: list joints the selected pieces fit …
    Connect,
    /// … move through them …
    Step(i32),
    /// … seat the shown one, or stop.
    Seat,
    Cancel,
    /// Release the seat at this joint and pull the pieces apart.
    Detach(JointRef),
    /// Show one library, or all.
    Shelf(Shelf),
    Import,
    Imported(Result<Vec<Definition>, String>),
    ExportMine,
    /// Export the piece with this key as a file.
    ExportPiece(String),
    Exported(Result<Option<PathBuf>, String>),
    /// Add the drawing's piece with this key to My pieces.
    Keep(String),
    Remove(String),
    UndoLibrary,
    Link,
    Linked(Option<PathBuf>),
    Unlink(usize),
    Reload,
    /// Move these pieces to the definition with this key.
    Update {
        ids: Vec<u64>,
        key: String,
    },
}

/// Keyboard Connect in progress: the pieces, and each target joint with
/// the selected piece's joint that goes into it.
pub struct Connect {
    pub ids: Vec<u64>,
    pub options: Vec<(JointRef, JointRef)>,
    pub index: usize,
    /// The drawing the options were found in; any other edit ends Connect.
    pub base: Document,
}

impl Action {
    /// A file dialog or library read finished: app work that leaves the
    /// drawing and its text drafts alone.
    pub fn completes(&self) -> bool {
        matches!(
            self,
            Self::Imported(_) | Self::Exported(_) | Self::Linked(_)
        )
    }
}

impl Connect {
    /// The target joint shown now, and the selected piece's joint for it.
    pub fn shown(&self) -> Option<&(JointRef, JointRef)> {
        self.options.get(self.index)
    }
}

pub struct State {
    pub query: String,
    pub category: Filter,
    pub armed: Option<Arc<Definition>>,
    pub repeat: bool,
    /// The library shown; all when none.
    pub shelf: Option<Source>,
    pub libraries: Libraries,
}
impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: Filter::default(),
            armed: None,
            repeat: true,
            shelf: None,
            libraries: Libraries::load(),
        }
    }
}

/// The piece's name. The interface is in English; search also reads the
/// other languages.
pub fn name(d: &Definition) -> &str {
    d.names.get("en").map_or(&d.id, String::as_str)
}

/// The name without its size: "Round-bottom flask".
fn title(d: &Definition) -> &str {
    let name = name(d);
    name.split_once(',').map_or(name, |(title, _)| title)
}

/// What a click places, as in "Click to place the flask".
pub(crate) fn noun(d: &Definition) -> String {
    let title = title(d);
    title.rsplit(' ').next().unwrap_or(title).to_lowercase()
}

/// "flask neck": the piece and one of its joints, as snapping names them.
pub(crate) fn joint_name(doc: &Document, j: &JointRef) -> String {
    let piece = (doc.graphics.iter())
        .find(|g| g.id == j.piece)
        .and_then(|g| g.piece.as_ref()?.linked.as_ref())
        .map_or_else(|| "piece".to_owned(), |d| noun(d));
    format!("{piece} {}", j.joint.replace('-', " "))
}

/// What a snap does, for the pill beside the joint and the status bar.
pub(crate) fn snap_text(doc: &Document, snap: &Snap) -> String {
    let target = joint_name(doc, &snap.target.at);
    let mut text = match &snap.offer {
        Offer::Seat => format!("Seat on {target} · {}", snap.target.size()),
        Offer::Adapter(d) => format!("Insert {} on {target}", name(d).replace(" to ", " → ")),
        Offer::Mismatch if snap.target.gender == snap.moving.gender => format!(
            "Two {} do not seat · no adapter fits",
            if snap.target.gender == Gender::F {
                "sockets"
            } else {
                "cones"
            }
        ),
        Offer::Mismatch => format!(
            "No adapter joins {} to {target} {}",
            snap.moving.size(),
            snap.target.size()
        ),
    };
    if snap.scaled {
        text.push_str(" · Scaled to match");
    }
    match snap.behind {
        Some(true) => text.push_str(" · Goes behind the structures"),
        Some(false) => text.push_str(" · Comes in front of the structures"),
        None => {}
    }
    text
}

/// The size fields, volume first: "100 mL · 15/25".
fn size(d: &Definition) -> String {
    let volume = d.variant.get("volume_ml").map(|v| format!("{v} mL"));
    let rest = d
        .variant
        .iter()
        .filter(|(key, _)| *key != "volume_ml")
        .map(|(_, value)| value.clone());
    volume
        .into_iter()
        .chain(rest)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Every word of the query is in a name in any language, a keyword or a size.
pub fn matches(d: &Definition, query: &str) -> bool {
    let fields: Vec<String> = (d.names.values())
        .chain(&d.keywords)
        .chain(d.variant.values())
        .map(|s| s.to_lowercase())
        .collect();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| fields.iter().any(|field| field.contains(word)))
}

/// A drawing holding only the piece, for its thumbnail.
fn thumbnail(e: &Entry, theme: reshiki::canvas_theme::CanvasTheme) -> Document {
    let mut doc = Document {
        canvas_theme: theme,
        ..Document::default()
    };
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic_keyed(
        1,
        e.definition.clone(),
        e.key.clone(),
        Point::default(),
        scale,
    ));
    doc
}

impl App {
    fn armed_piece(&self) -> Option<&Arc<Definition>> {
        self.equipment
            .armed
            .as_ref()
            .filter(|_| self.tool == Tool::Equipment)
    }

    pub(super) fn equipment_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Search(query) => self.equipment.query = query,
            Action::Category(filter) => self.equipment.category = filter,
            Action::Repeat(on) => self.equipment.repeat = on,
            Action::Arm(k) => {
                let Some(definition) = (self.equipment.libraries.entries.iter())
                    .find(|e| e.key == k)
                    .map(|e| e.definition.clone())
                else {
                    return Task::none();
                };
                self.status = format!(
                    "{} · {} · Click to place · Esc cancels",
                    title(&definition),
                    size(&definition)
                );
                self.error = false;
                self.tab.connect = None;
                self.equipment.armed = Some(definition);
                self.tool = Tool::Equipment;
                return super::focus_drawing();
            }
            Action::Close => {
                self.select_tool(Tool::Select);
                self.inspector_tab = InspectorTab::Properties;
                return super::focus_drawing();
            }
            Action::Connect => {
                if self.tool != Tool::Select {
                    self.select_tool(Tool::Select);
                }
                let ids = self.tab.selected.clone();
                let options: Vec<_> = solve::options(&self.tab.doc, &ids)
                    .into_iter()
                    .map(|(s, m, _)| (s.at, m.at))
                    .collect();
                self.tab.connect = (!options.is_empty()).then(|| Connect {
                    ids,
                    options,
                    index: 0,
                    base: self.tab.doc.clone(),
                });
                self.connect_status();
                return super::focus_drawing();
            }
            Action::Step(by) => {
                if let Some(c) = &mut self.tab.connect {
                    let n = c.options.len() as i32;
                    c.index = (c.index as i32 + by).rem_euclid(n) as usize;
                }
                self.connect_status();
            }
            Action::Seat => {
                if self.connecting().is_none() {
                    return Task::none();
                }
                let Some(c) = self.tab.connect.take() else {
                    return Task::none();
                };
                let Some((s, m)) = c.shown() else {
                    return Task::none();
                };
                let before = self.tab.doc.clone();
                if let Some(snap) = solve::connect(&mut self.tab.doc, &c.ids, s, m) {
                    self.tab.selected = solve::closure(&self.tab.doc, &c.ids);
                    self.changed(before);
                    if !self.error {
                        self.snapped(&snap);
                    }
                }
            }
            Action::Cancel => {
                self.tab.connect = None;
                self.status = "Connect canceled".into();
                self.error = false;
            }
            Action::Detach(joint) => {
                let before = self.tab.doc.clone();
                let name = joint_name(&self.tab.doc, &joint);
                if solve::detach(&mut self.tab.doc, &joint) {
                    self.changed(before);
                    if !self.error {
                        self.status = format!("Detached the {name}");
                    }
                }
                return super::focus_drawing();
            }
            other => return self.library_action(other),
        }
        Task::none()
    }

    /// Keyboard Connect, while it still applies: Select is the tool and the
    /// drawing is the one its joints were found in.
    pub(super) fn connecting(&self) -> Option<&Connect> {
        (self.tab.connect.as_ref()).filter(|c| self.tool == Tool::Select && c.base == self.tab.doc)
    }

    /// The selection holds an equipment piece.
    pub(super) fn pieces_selected(&self) -> bool {
        (self.tab.doc.graphics.iter())
            .any(|g| g.piece.is_some() && self.tab.selected.contains(&g.id))
    }

    /// Keyboard Connect's status line: the joint shown and how to go on.
    fn connect_status(&mut self) {
        self.error = false;
        self.status = match &self.tab.connect {
            None => {
                self.error = true;
                "No free joint fits the selected pieces".into()
            }
            Some(c) => {
                let Some((s, _)) = c.shown() else {
                    return;
                };
                format!(
                    "Connect to the {} ({} of {}) · ↑↓ · Return · Esc",
                    joint_name(&self.tab.doc, s),
                    c.index + 1,
                    c.options.len()
                )
            }
        };
    }

    /// One click places one piece, centred on the pointer, as one Undo step.
    pub(super) fn place_piece(&mut self, at: Point, before: Document) {
        if self.tool != Tool::Equipment {
            return;
        }
        let Some(definition) = self.equipment.armed.clone() else {
            self.status = "Choose a piece in the Equipment panel".into();
            self.error = false;
            return;
        };
        let id = self.tab.doc.next_id();
        let scale = placed::world_per_mm(&self.tab.doc, placed::DEFAULT_PT_PER_MM);
        self.tab
            .doc
            .graphics
            .push(placed::graphic(id, definition, at, scale));
        self.tab.selected = vec![id];
        let reach = solve::REACH_PX / self.tab.camera.zoom;
        let snap = solve::snap(&mut self.tab.doc, &[id], reach, true);
        if !self.equipment.repeat {
            self.tool = Tool::Select;
        }
        if snap
            .as_ref()
            .is_some_and(|s| !matches!(s.offer, Offer::Mismatch))
        {
            self.tab.selected = solve::closure(&self.tab.doc, &[id]);
        }
        self.changed(before);
        if let Some(snap) = snap.filter(|_| !self.error) {
            self.snapped(&snap);
        }
    }

    /// Say what a snap did once it is committed.
    pub(super) fn snapped(&mut self, snap: &Snap) {
        let doc = &self.tab.doc;
        let moving = joint_name(doc, &snap.moving.at);
        let target = joint_name(doc, &snap.target.at);
        let kept = solve::partner(doc, &snap.moving.at).is_some();
        self.status = match &snap.offer {
            // A lasting layout can move a piece away again as it settles.
            Offer::Seat | Offer::Adapter(_) if !kept => {
                format!("The layout moved the {moving}, so it is not seated")
            }
            Offer::Seat => format!("Seated the {moving} on the {target}"),
            Offer::Adapter(d) => {
                format!("Inserted the {}", title(d).to_lowercase())
            }
            Offer::Mismatch => snap_text(doc, snap),
        };
        self.error = false;
    }

    pub(super) fn equipment_panel(&self) -> Element<'_, Message> {
        let state = &self.equipment;
        let libraries = &state.libraries;
        let entries = &libraries.entries;
        let arm = |e: &Entry| Message::Equipment(Action::Arm(e.key.clone()));
        let armed = |e: &Entry| {
            self.armed_piece()
                .is_some_and(|a| Arc::ptr_eq(a, &e.definition))
        };
        // A piece shown beside another revision names its revision; one
        // that shares its ID and revision with a different piece names its
        // library, and its content when the library is the same.
        let label = |e: &Entry| {
            let d = &e.definition;
            let mut out = title(d).to_owned();
            let same_id = || {
                entries
                    .iter()
                    .filter(|o| o.definition.id == d.id && o.key != e.key)
            };
            if same_id().any(|o| o.definition.revision != d.revision) {
                out.push_str(&format!(" · revision {}", d.revision));
            }
            if let Some(o) = same_id().find(|o| o.definition.revision == d.revision) {
                let library = libraries.name(e.source);
                out.push_str(&if o.source == e.source {
                    format!(" ({library} · {})", &e.key[e.key.len().saturating_sub(6)..])
                } else {
                    format!(" ({library})")
                });
            }
            out
        };
        let shown: Vec<&Entry> = (entries.iter())
            .filter(|e| state.shelf.is_none_or(|s| e.in_library(s)))
            .collect();
        let mut categories = vec![Filter(None)];
        for e in &shown {
            if !categories.contains(&Filter(Some(e.definition.category))) {
                categories.push(Filter(Some(e.definition.category)));
            }
        }
        let shelves = libraries.shelves();
        let shelf = shelves.iter().find(|s| s.source == state.shelf).cloned();
        let mut body = column![
            row![
                section("Equipment").width(Length::Fill),
                command("Done", Message::Equipment(Action::Close)),
            ]
            .align_y(Alignment::Center),
            crate::appearance::text_input("Search names and keywords…", &state.query)
                .on_input(|s| Message::Equipment(Action::Search(s)))
                .size(12)
                .padding(8),
            row![
                crate::appearance::pick_list(categories, Some(state.category), |f| {
                    Message::Equipment(Action::Category(f))
                })
                .text_size(12)
                .padding(6)
                .width(Length::Fill),
                crate::appearance::pick_list(shelves, shelf, |s| {
                    Message::Equipment(Action::Shelf(s))
                })
                .text_size(12)
                .padding(6)
                .width(Length::Fill),
            ]
            .spacing(6),
        ]
        .spacing(8);
        if let Some(d) = self.armed_piece() {
            let source = (entries.iter())
                .find(|e| Arc::ptr_eq(&e.definition, d))
                .map_or(Source::BuiltIn, |e| e.source);
            let mut card = column![
                text(format!("{} · {}", title(d), size(d))).size(14),
                checkbox(state.repeat)
                    .label("Keep placing")
                    .text_size(12)
                    .size(14)
                    .on_toggle(|v| Message::Equipment(Action::Repeat(v))),
                text(if state.repeat {
                    "Click again to add another copy. Esc stops."
                } else {
                    "Returns to Select after one placement."
                })
                .size(11)
                .style(muted_text),
            ]
            .spacing(6);
            if source != Source::BuiltIn {
                let mut line = row![
                    text(format!(
                        "{} · revision {}",
                        libraries.name(source),
                        d.revision
                    ))
                    .size(11)
                    .style(muted_text)
                    .width(Length::Fill)
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                if source == Source::Mine {
                    line = line.push(command(
                        "Remove",
                        Message::Equipment(Action::Remove(placed::key(d))),
                    ));
                }
                card = card.push(line);
            }
            body = body.push(super::inspector::card(container(card).padding(12)));
        }
        // Each family of each library once, with its sizes as chips.
        let mut families: Vec<(&Entry, Vec<&Entry>)> = Vec::new();
        for e in shown.into_iter().filter(|e| {
            state.category.0.is_none_or(|c| c == e.definition.category)
                && matches(&e.definition, &state.query)
        }) {
            match families.iter_mut().find(|(first, _)| {
                first.source == e.source && first.definition.family == e.definition.family
            }) {
                Some((_, sizes)) => sizes.push(e),
                None => families.push((e, vec![e])),
            }
        }
        let count: usize = families.iter().map(|(_, sizes)| sizes.len()).sum();
        body = body.push(horizontal_line()).push(
            text(if count == 1 {
                "1 piece".to_owned()
            } else {
                format!("{count} pieces")
            })
            .size(11)
            .style(muted_text),
        );
        let theme = self.tab.doc.canvas_theme;
        for (first, sizes) in &families {
            let shown = sizes.iter().copied().find(|e| armed(e)).unwrap_or(*first);
            let d = &shown.definition;
            let preview: Element<'_, Message> =
                canvas(crate::canvas::DrawingThumbnail(thumbnail(shown, theme)))
                    .width(44)
                    .height(44)
                    .into();
            let tile = hover_hint(
                reshiki::accessibility::button(
                    format!("equipment-{}", shown.key),
                    label(shown),
                    preview,
                )
                .checked(armed(shown))
                .padding(4)
                .width(52)
                .height(52)
                .style(control(armed(shown)))
                .on_press(arm(shown)),
                name(d).to_owned(),
                tooltip::Position::Left,
            );
            let mut chips = row![].spacing(4);
            for e in sizes {
                let d = &e.definition;
                chips = chips.push(
                    reshiki::accessibility::button(
                        format!("equipment-size-{}", e.key),
                        format!("{} · {}", label(e), size(d)),
                        text(
                            if sizes
                                .iter()
                                .any(|o| o.key != e.key && size(&o.definition) == size(d))
                            {
                                format!("{} · r{}", size(d), d.revision)
                            } else {
                                size(d)
                            },
                        )
                        .size(11),
                    )
                    .checked(armed(e))
                    .padding([3, 7])
                    .style(control(armed(e)))
                    .on_press(arm(e)),
                );
            }
            body = body.push(
                row![
                    tile,
                    column![
                        text(label(shown)).size(12),
                        chips.wrap().vertical_spacing(4)
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        if families.is_empty() {
            body = body.push(
                text("No matching equipment. Try another search, category or library.")
                    .size(12)
                    .style(muted_text),
            );
        }
        body.push(self.libraries_section()).into()
    }

    /// The selected pieces' name and size. Pieces are always illustration.
    pub(super) fn piece_card(&self) -> Option<Element<'_, Message>> {
        let pieces: Vec<_> = (self.tab.doc.graphics.iter())
            .filter(|g| self.tab.selected.contains(&g.id))
            .filter_map(|g| g.piece.as_ref()?.linked.as_ref())
            .collect();
        let (heading, detail) = match pieces.as_slice() {
            [] => return None,
            // A library piece names its revision, so the one exported or
            // updated is plain.
            [d] if d.id.starts_with("reshiki/") => (title(d).to_owned(), size(d)),
            [d] => (
                title(d).to_owned(),
                format!("{} · revision {}", size(d), d.revision),
            ),
            many => (format!("{} equipment pieces", many.len()), String::new()),
        };
        let mut body = column![
            section("Equipment"),
            text(heading).size(14),
            text(detail).size(12).style(muted_text),
            row![
                text("Purpose").size(12).width(Length::Fill),
                text("Illustration · Picture only")
                    .size(11)
                    .style(muted_text),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);
        // One piece: each joint, what is seated in it, and Detach.
        if let [id] = self.tab.selected.as_slice()
            && let Some(g) = self.tab.doc.graphics.iter().find(|g| g.id == *id)
        {
            let doc = &self.tab.doc;
            for port in solve::ports(g) {
                let partner = solve::partner(doc, &port.at);
                let state = partner.as_ref().map_or_else(
                    || "Free".to_owned(),
                    |p| {
                        // A socket holds a cone; a cone sits in a socket.
                        let verb = if port.gender == Gender::F {
                            "Holds"
                        } else {
                            "Seated in"
                        };
                        format!("{verb} the {}", joint_name(doc, p))
                    },
                );
                let mut line = row![
                    column![
                        text(format!(
                            "{} · {}",
                            port.at.joint.replace('-', " "),
                            port.size()
                        ))
                        .size(12),
                        text(state).size(11).style(muted_text),
                    ]
                    .width(Length::Fill),
                ]
                .align_y(Alignment::Center);
                if partner.is_some() {
                    line = line.push(command(
                        "Detach",
                        Message::Equipment(Action::Detach(port.at.clone())),
                    ));
                }
                body = body.push(line);
            }
        }
        // An assembly: each seat in it, with Detach.
        if self.tab.selected.len() > 1 {
            body = body.push(
                text("Option-click a piece for its own joints and library actions.")
                    .size(11)
                    .style(muted_text),
            );
            let doc = &self.tab.doc;
            for seat in (doc.seats.iter()).filter(|s| {
                self.tab.selected.contains(&s.a.piece) && self.tab.selected.contains(&s.b.piece)
            }) {
                body = body.push(
                    row![
                        text(format!(
                            "{} · {}",
                            joint_name(doc, &seat.a),
                            joint_name(doc, &seat.b)
                        ))
                        .size(12)
                        .width(Length::Fill),
                        command("Detach", Message::Equipment(Action::Detach(seat.a.clone()))),
                    ]
                    .align_y(Alignment::Center),
                );
            }
        }
        // One kind of piece: keep it, export it, or update it.
        if let [d, rest @ ..] = pieces.as_slice()
            && rest.iter().all(|o| placed::key(o) == placed::key(d))
        {
            body = body.push(self.piece_library_rows(d));
        }
        if self.pieces_selected() {
            body = body.push(hover_hint(
                command("Connect…", Message::Equipment(Action::Connect)),
                "Seat on a free joint nearby (J)",
                tooltip::Position::Left,
            ));
        }
        Some(super::inspector::card(container(body).padding(12)))
    }
}

#[cfg(test)]
mod tests;
