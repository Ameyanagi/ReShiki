//! The Equipment browser: lab apparatus to place on the drawing. Choosing a
//! size arms it, and each click on the drawing places one copy.
use super::piece_libraries::{Entry, Libraries, Shelf, Source};
use super::workspace::{
    command, control, horizontal_line, hover_hint, muted_text, section, segment, segment_track,
};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use crate::canvas::layered::canvas;
use iced::widget::{checkbox, column, container, row, slider, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::{Document, Point};
use reshiki::equipment::{
    eval::{Value, resolve},
    format::{Category, Definition, Gender, HoldKind, Param, Unit},
    placed,
    solve::{self, JointRef, Offer, Port, Seat, Snap, adjust},
};
use reshiki::graphics::Graphic;
use std::{path::PathBuf, sync::Arc};

pub(super) const INSERT: &str = "Insert equipment…";

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
    /// Put a joint clip on every ground-glass seat of the selection.
    AddClips,
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
    /// Set a parameter of the piece `id`: a valve, a tilt, a liquid level.
    Param {
        id: u64,
        param: String,
        value: Value,
    },
    /// A slider moved: like `Param`, but a drag is one Undo step until
    /// [`Action::SlideEnd`].
    Slide {
        id: u64,
        param: String,
        value: f64,
    },
    SlideEnd,
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
/// A hold is named by what it is: "stand rod", "clamp", "flask".
pub(crate) fn joint_name(doc: &Document, j: &JointRef) -> String {
    let g = doc.graphics.iter().find(|g| g.id == j.piece);
    let piece = g
        .and_then(|g| g.piece.as_ref()?.linked.as_ref())
        .map_or_else(|| "piece".to_owned(), |d| noun(d));
    let hold = g.and_then(|g| solve::ports(g).into_iter().find(|p| p.at == *j)?.hold());
    match hold {
        Some(HoldKind::Rail) => format!("{piece} rod"),
        Some(_) => piece,
        None => format!("{piece} {}", j.joint.replace('-', " ")),
    }
}

fn xy(p: Point) -> [f64; 2] {
    [f64::from(p.x), f64::from(p.y)]
}

/// "Hose on the condenser water in", or that its end hangs free.
fn hose_text(doc: &Document, barbs: &[solve::Port]) -> String {
    let names: Vec<String> = barbs.iter().map(|b| joint_name(doc, &b.at)).collect();
    if names.is_empty() {
        "Hose end free · drag it onto a hose barb".into()
    } else {
        format!("Hose on the {}", names.join(" and the "))
    }
}

/// "Tilt: 25°", "Valve: Open", "Liquid level: 40 %": a parameter of the
/// piece `g` at `value`; its current value when `value` is `None`.
pub(crate) fn value_text(g: &Graphic, param: &str, value: Option<&Value>) -> String {
    let Some((piece, d)) = g.piece.as_ref().and_then(|p| Some((p, p.linked.as_ref()?))) else {
        return String::new();
    };
    let Some(p) = d.params.iter().find(|p| p.id() == param) else {
        return String::new();
    };
    let current = resolve(d, &piece.params)
        .ok()
        .and_then(|v| v.get(param).cloned());
    let Some(value) = value.or(current.as_ref()) else {
        return String::new();
    };
    let en = |names: &std::collections::BTreeMap<String, String>, or: &str| {
        names.get("en").cloned().unwrap_or_else(|| or.to_owned())
    };
    let shown = match (p, value) {
        (Param::States { states, .. }, Value::State(s)) => states
            .iter()
            .find(|x| x.id == *s)
            .map_or_else(|| s.clone(), |x| en(&x.names, s)),
        (
            Param::Fill { .. }
            | Param::Range {
                unit: Unit::Fraction,
                ..
            },
            Value::Number(n),
        ) => format!("{:.0} %", n * 100.),
        (
            Param::Range {
                unit: Unit::Deg, ..
            },
            Value::Number(n),
        ) => format!("{n:.0}°"),
        (Param::Range { unit: Unit::Mm, .. }, Value::Number(n)) => format!("{n} mm"),
        _ => return en(p.names(), param),
    };
    format!("{}: {shown}", en(p.names(), param))
}

/// What setting a parameter let go of: " · releases the flask".
pub(crate) fn released_text(doc: &Document, id: u64, released: &[Seat]) -> String {
    released
        .iter()
        .map(|s| {
            let other = if s.a.piece == id { &s.b } else { &s.a };
            format!(" · releases the {}", joint_name(doc, other))
        })
        .collect()
}

/// What a seat at `port` with `partner` says: "Holds the clamp".
fn seat_state(doc: &Document, port: &Port, partner: &JointRef) -> String {
    let verb = match port.hold() {
        Some(HoldKind::Rail) => "Holds the",
        Some(HoldKind::Grip) => "Held by the",
        Some(HoldKind::Slider) => "On the",
        Some(HoldKind::Jaw) => "Grips the",
        // A socket holds a cone; a cone sits in a socket.
        None if port.gender == Gender::F => "Holds the",
        None => "Seated in the",
    };
    let clip = if solve::clip_at(doc, &port.at).is_some() {
        " · clipped"
    } else {
        ""
    };
    // A rod holds several clamps: say where along it each one is.
    let at = (port.shared())
        .then(|| {
            let g = doc.graphics.iter().find(|g| g.id == partner.piece)?;
            let p = solve::ports(g).into_iter().find(|p| p.at == *partner)?;
            let c = [
                p.point[0] + p.dir[0] * p.length / 2. - port.point[0],
                p.point[1] + p.dir[1] * p.length / 2. - port.point[1],
            ];
            Some(format!(
                " at {:.0} mm",
                (c[0] * port.dir[0] + c[1] * port.dir[1]) / port.mm
            ))
        })
        .flatten()
        .unwrap_or_default();
    format!("{verb} {}{at}{clip}", joint_name(doc, partner))
}

/// A shared rod names the individual seat's other side when detaching.
fn detach_joint(port: &Port, partner: &JointRef) -> JointRef {
    if port.shared() {
        partner.clone()
    } else {
        port.at.clone()
    }
}

fn seat_detach(doc: &Document, seat: &solve::Seat) -> JointRef {
    (doc.graphics.iter())
        .find(|g| g.id == seat.a.piece)
        .and_then(|g| solve::ports(g).into_iter().find(|p| p.at == seat.a))
        .map_or_else(|| seat.a.clone(), |port| detach_joint(&port, &seat.b))
}

/// What a snap does, for the pill beside the joint and the status bar.
pub(crate) fn snap_text(doc: &Document, snap: &Snap) -> String {
    let target = joint_name(doc, &snap.target.at);
    let (held, holder) = if snap.target.gender == Gender::F {
        (&snap.target, &snap.moving)
    } else {
        (&snap.moving, &snap.target)
    };
    let mut text = match &snap.offer {
        Offer::Seat if held.hold() == Some(HoldKind::Rail) => format!(
            "Mount the {} on the {}",
            joint_name(doc, &holder.at),
            joint_name(doc, &held.at)
        ),
        Offer::Seat if held.hold().is_some() => {
            format!("Hold the {} · {}", joint_name(doc, &held.at), held.size())
        }
        Offer::Seat => format!("Seat on {target} · {}", snap.target.size()),
        Offer::Clip => format!("Clip the {target} · {}", snap.target.size()),
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
/// "Round-bottom flask · 100 mL · 15/25", or the title alone for a piece without sizes.
fn named(d: &Definition) -> String {
    let size = size(d);
    if size.is_empty() {
        title(d).to_owned()
    } else {
        format!("{} · {size}", title(d))
    }
}

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
                self.status = format!("{} · Click to place · Esc cancels", named(&definition));
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
            Action::AddClips => {
                let before = self.tab.doc.clone();
                let ids = self.tab.selected.clone();
                let added = solve::add_clips(&mut self.tab.doc, &ids);
                self.tab.selected = solve::closure(&self.tab.doc, &ids);
                self.changed(before);
                if !self.error {
                    self.status = match added {
                        1 => "Added 1 joint clip".into(),
                        n => format!("Added {n} joint clips"),
                    };
                }
                return super::focus_drawing();
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
            Action::Param { id, param, value } => {
                let before = self.tab.doc.clone();
                self.set_piece_param(id, &param, value, before);
                return super::focus_drawing();
            }
            Action::Slide { id, param, value } => {
                // A drag continues only on the same slider while nothing
                // else has been committed.
                let at = (self.tab.revision, id, param.clone());
                // What the drag has let go of so far stays in the status.
                let carried = (self.tab.piece_sliding.take())
                    .filter(|(r, i, p, _)| (*r, *i, p) == (at.0, at.1, &at.2))
                    .map(|(.., note)| note);
                let before = self.tab.doc.clone();
                let note = carried.clone().unwrap_or_default()
                    + &self.set_piece_param_continuing(
                        id,
                        &param,
                        Value::Number(value),
                        before,
                        carried.is_some(),
                    );
                if let Some(g) = (self.tab.doc.graphics.iter()).find(|g| g.id == id)
                    && !self.error
                {
                    self.status = value_text(g, &param, None) + &note;
                }
                self.tab.piece_sliding = Some((self.tab.revision, id, param, note));
            }
            Action::SlideEnd => self.tab.piece_sliding = None,
            other => return self.library_action(other),
        }
        Task::none()
    }

    /// Set a parameter of the piece `id` as one Undo step, re-solving what is
    /// seated on its parts, and say what it is now and what it let go of.
    pub(super) fn set_piece_param(&mut self, id: u64, param: &str, value: Value, before: Document) {
        self.set_piece_param_continuing(id, param, value, before, false);
    }

    /// [`App::set_piece_param`], merged into the last Undo step while `continuing`.
    /// Returns what it let go of, as [`released_text`] says it.
    fn set_piece_param_continuing(
        &mut self,
        id: u64,
        param: &str,
        value: Value,
        before: Document,
        continuing: bool,
    ) -> String {
        match adjust::set(&mut self.tab.doc, id, param, value) {
            Err(error) => {
                self.tab.doc = before;
                self.status = error;
                self.error = true;
                String::new()
            }
            Ok(_) => {
                let (seats, names) = (before.seats.clone(), before.clone());
                self.changed_continuing(before, continuing);
                if self.error {
                    return String::new();
                }
                // Name what the commit let go of, a layout's reflow included.
                let released: Vec<Seat> = (seats.into_iter())
                    .filter(|s| !self.tab.doc.seats.contains(s))
                    .collect();
                let note = released_text(&names, id, &released);
                if let Some(g) = self.tab.doc.graphics.iter().find(|g| g.id == id) {
                    self.status = value_text(g, param, None) + &note;
                }
                note
            }
        }
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

    /// The Properties card's Detach target, only when it is unambiguous.
    pub(super) fn selected_piece_detach(&self) -> Option<JointRef> {
        let doc = &self.tab.doc;
        // Properties needs at least one linked piece before showing its rows.
        if !doc.graphics.iter().any(|g| {
            self.tab.selected.contains(&g.id)
                && g.piece.as_ref().is_some_and(|p| p.linked.is_some())
        }) {
            return None;
        }
        let mut joints = vec![];
        if let [id] = self.tab.selected.as_slice() {
            let graphic = doc.graphics.iter().find(|g| g.id == *id)?;
            for port in solve::ports(graphic) {
                for partner in solve::partners(doc, &port.at) {
                    let joint = detach_joint(&port, &partner);
                    if !joints.contains(&joint) {
                        joints.push(joint);
                    }
                }
            }
        } else {
            for seat in &doc.seats {
                if self.tab.selected.contains(&seat.a.piece)
                    && self.tab.selected.contains(&seat.b.piece)
                {
                    let joint = seat_detach(doc, seat);
                    if !joints.contains(&joint) {
                        joints.push(joint);
                    }
                }
            }
        }
        (joints.len() == 1).then(|| joints.remove(0))
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

    /// Lay the armed hose from `from` to `to` (one point for a click), its
    /// ends pushed onto barbs they reach, as one Undo step.
    pub(super) fn place_hose(&mut self, from: Point, to: Point, before: Document) {
        let Some(definition) = (self.equipment.armed.clone())
            .filter(|d| self.tool == Tool::Equipment && d.tube.is_some())
        else {
            return;
        };
        let reach = f64::from(solve::REACH_PX / self.tab.camera.zoom);
        let (id, barbs) =
            solve::hose::place_hose(&mut self.tab.doc, definition, xy(from), xy(to), reach);
        if !self.equipment.repeat {
            self.tool = Tool::Select;
        }
        self.tab.selected = solve::closure(&self.tab.doc, &[id]);
        self.changed(before);
        if !self.error {
            self.status = hose_text(&self.tab.doc, &barbs);
        }
    }

    /// Drag a hose's end or middle, as one Undo step.
    pub(super) fn drag_hose(
        &mut self,
        id: u64,
        grip: solve::hose::Grip,
        to: Point,
        before: Document,
    ) {
        let reach = f64::from(solve::REACH_PX / self.tab.camera.zoom);
        let barb = solve::hose::drag(&mut self.tab.doc, id, grip, xy(to), reach);
        self.changed(before);
        if !self.error && grip != solve::hose::Grip::Middle {
            self.status = hose_text(&self.tab.doc, barb.as_slice());
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
            Offer::Seat if snap.target.hold().is_some() => {
                let (held, holder) = if snap.target.gender == Gender::F {
                    (&snap.target, &snap.moving)
                } else {
                    (&snap.moving, &snap.target)
                };
                let (held, holder) = (joint_name(doc, &held.at), joint_name(doc, &holder.at));
                if snap.target.hold() == Some(HoldKind::Rail)
                    || snap.moving.hold() == Some(HoldKind::Rail)
                {
                    format!("Mounted the {holder} on the {held}")
                } else {
                    format!("The {holder} holds the {held}")
                }
            }
            Offer::Seat => format!("Seated the {moving} on the {target}"),
            Offer::Clip => format!("Clipped the {target}"),
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
                text(named(d)).size(14),
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
                let size_label = size(d);
                // A lone piece without sizes needs no chip: its tile arms it.
                if size_label.is_empty() && sizes.len() == 1 {
                    continue;
                }
                // Two chips of one size name their revision, and two of one
                // revision the start of their content hash.
                let twin = |o: &Entry| o.key != e.key && size(&o.definition) == size_label;
                let mut parts = vec![size_label.clone()];
                if sizes.iter().any(|o| twin(o)) {
                    parts.push(format!("r{}", d.revision));
                }
                if sizes
                    .iter()
                    .any(|o| twin(o) && o.definition.revision == d.revision)
                {
                    let hash = e.key.rsplit('#').next().unwrap_or_default();
                    parts.push(hash.get(..6).unwrap_or(hash).to_owned());
                }
                parts.retain(|p| !p.is_empty());
                let chip = parts.join(" · ");
                chips = chips.push(
                    reshiki::accessibility::button(
                        format!("equipment-size-{}", e.key),
                        format!("{} · {chip}", label(e)),
                        text(chip).size(11),
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

    fn piece_param<'a>(
        &self,
        g: &'a Graphic,
        param: &'a Param,
        current: &Value,
    ) -> Option<Element<'a, Message>> {
        let id = g.id;
        let param_id = param.id();
        let label = param.names().get("en").map_or(param_id, String::as_str);
        let (range, step, value) = match (param, current) {
            (Param::States { states, .. }, Value::State(current)) => {
                let choices = row(states.iter().map(|state| {
                    let label = state.names.get("en").unwrap_or(&state.id);
                    let selected = &state.id == current;
                    reshiki::accessibility::button(
                        format!("equipment-param-{id}-{param_id}-{}", state.id),
                        label.as_str(),
                        text(label.as_str()).size(11),
                    )
                    .checked(selected)
                    .padding([4, 6])
                    .style(segment(selected))
                    .on_press(Message::Equipment(Action::Param {
                        id,
                        param: param_id.into(),
                        value: Value::State(state.id.clone()),
                    }))
                    .into()
                }))
                .spacing(2);
                return Some(
                    row![
                        text(label).size(12).width(Length::FillPortion(1)),
                        container(choices.wrap())
                            .width(Length::FillPortion(3))
                            .padding(2)
                            .style(segment_track),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
            (Param::Range { min, max, unit, .. }, Value::Number(value)) => {
                let step = match unit {
                    Unit::Deg => 1.,
                    Unit::Mm => 0.5,
                    Unit::Fraction => 0.01,
                };
                (*min..=*max, step, *value)
            }
            (Param::Fill { .. }, Value::Number(value)) => (0. ..=1., 0.01, *value),
            _ => return None,
        };
        let shown = value_text(g, param_id, Some(&Value::Number(value)));
        let shown = shown
            .rsplit_once(": ")
            .map_or(shown.as_str(), |(_, value)| value);
        let param_id = param_id.to_owned();
        let slider = slider(range, value, move |value| {
            Message::Equipment(Action::Slide {
                id,
                param: param_id.clone(),
                value,
            })
        })
        .step(step)
        .on_release(Message::Equipment(Action::SlideEnd));
        Some(
            column![
                row![
                    text(label).size(12).width(Length::Fill),
                    text(shown.to_owned()).size(11).style(muted_text),
                ]
                .align_y(Alignment::Center),
                slider,
            ]
            .spacing(4)
            .into(),
        )
    }

    /// The selected pieces' details and controls. Pieces are always illustration.
    pub(super) fn piece_card(&self) -> Option<Element<'_, Message>> {
        let pieces: Vec<_> = (self.tab.selected.iter())
            .filter_map(|id| {
                let g = self.tab.doc.graphics.iter().find(|g| g.id == *id)?;
                let piece = g.piece.as_ref()?;
                Some((g, piece, piece.linked.as_ref()?))
            })
            .collect();
        let (heading, detail) = match pieces.as_slice() {
            [] => return None,
            // A library piece names its revision, so the one exported or
            // updated is plain.
            [(_, _, d)] if d.id.starts_with("reshiki/") => (title(d).to_owned(), size(d)),
            [(_, _, d)] => (
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
        let movable: Vec<_> = pieces
            .iter()
            .copied()
            .filter(|(_, _, d)| d.params.iter().any(|p| !matches!(p, Param::Color { .. })))
            .collect();
        if !movable.is_empty() {
            body = body.push(section("Movable parts"));
            for &(g, piece, d) in &movable {
                let Ok(values) = resolve(d, &piece.params) else {
                    continue;
                };
                if movable.len() > 1 {
                    body = body.push(text(name(d)).size(12));
                }
                for param in &d.params {
                    if let Some(current) = values.get(param.id())
                        && let Some(control) = self.piece_param(g, param, current)
                    {
                        body = body.push(control);
                    }
                }
            }
        }
        // One piece: each joint and hold, what is seated on it, and Detach.
        // A rod lists every clamp on it.
        if let [id] = self.tab.selected.as_slice()
            && let Some(g) = self.tab.doc.graphics.iter().find(|g| g.id == *id)
        {
            let doc = &self.tab.doc;
            for port in solve::ports(g) {
                let name = format!("{} · {}", port.at.joint.replace('-', " "), port.size());
                let partners = solve::partners(doc, &port.at);
                if partners.is_empty() {
                    body = body.push(column![
                        text(name).size(12),
                        text("Free").size(11).style(muted_text)
                    ]);
                    continue;
                }
                for (i, partner) in partners.iter().enumerate() {
                    // Detach names the side a rod's seat is one of many on.
                    let joint = detach_joint(&port, partner);
                    let mut lines = column![];
                    if i == 0 {
                        lines = lines.push(text(name.clone()).size(12));
                    }
                    body = body.push(
                        row![
                            lines
                                .push(
                                    text(seat_state(doc, &port, partner))
                                        .size(11)
                                        .style(muted_text)
                                )
                                .width(Length::Fill),
                            command("Detach", Message::Equipment(Action::Detach(joint))),
                        ]
                        .align_y(Alignment::Center),
                    );
                }
            }
        }
        // An assembly: each seat in it, with Detach.
        if self.tab.selected.len() > 1 {
            body = body.push(
                text("Click a piece again for its own joints and library actions.")
                    .size(11)
                    .style(muted_text),
            );
            let doc = &self.tab.doc;
            let seats: Vec<_> = (doc.seats.iter())
                .filter(|s| {
                    self.tab.selected.contains(&s.a.piece) && self.tab.selected.contains(&s.b.piece)
                })
                .collect();
            for seat in &seats {
                // A rod holds many: detach the other side.
                let joint = seat_detach(doc, seat);
                let clip = if seat.clip.is_some() {
                    " · clipped"
                } else {
                    ""
                };
                body = body.push(
                    row![
                        text(format!(
                            "{} · {}{clip}",
                            joint_name(doc, &seat.a),
                            joint_name(doc, &seat.b)
                        ))
                        .size(12)
                        .width(Length::Fill),
                        command("Detach", Message::Equipment(Action::Detach(joint))),
                    ]
                    .align_y(Alignment::Center),
                );
            }
            if solve::unclipped(doc, &self.tab.selected) > 0 {
                body = body.push(hover_hint(
                    command(
                        "Add clips to all joints",
                        Message::Equipment(Action::AddClips),
                    ),
                    "A joint clip of the right size on every ground-glass joint without one",
                    tooltip::Position::Left,
                ));
            }
        }
        // One kind of piece: keep it, export it, or update it.
        if let [(_, _, d), rest @ ..] = pieces.as_slice()
            && rest
                .iter()
                .all(|(_, _, o)| placed::key(o) == placed::key(d))
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
