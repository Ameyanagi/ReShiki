use crate::canvas::{self, Camera, Edit, MoleculeCanvas, Tool};
use iced::{
    Color, Element, Length, Subscription, Task, Theme,
    widget::{
        Space, button, canvas as drawing, column, container, row, scrollable, text, text_input,
    },
};
use moruno::{
    document::{Annotation, Arrow, Document, History, Point},
    engine::{Analysis, ChemistryEngine, PythonEngine, Request, Response},
    scene,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    Canvas(Edit),
    Tool(Tool),
    Element(String),
    Caption(String),
    Smiles(String),
    Import,
    Example(&'static str),
    Clean,
    Analyze,
    Undo,
    Redo,
    Delete,
    SelectAll,
    Grid,
    Fit,
    Zoom(f32),
    New,
    Open,
    Save,
    Export(&'static str),
    CopySmiles,
    Charge(i32),
    Isotope(String),
    ApplyIsotope,
    EngineDone {
        revision: u64,
        kind: Job,
        result: Box<Result<Response, String>>,
    },
    Opened(Option<(PathBuf, Result<String, String>)>),
    Saved(u64, Document, Result<Option<PathBuf>, String>),
    Exported(Result<Option<PathBuf>, String>),
    Close(iced::window::Id),
    Discard,
    Cancel,
}
#[derive(Debug, Clone)]
pub enum Job {
    Import,
    ImportFile,
    Startup,
    Analyze,
    Clean,
    Export(&'static str),
}
#[derive(Debug, Clone)]
enum Pending {
    New,
    Open,
    Close(iced::window::Id),
}

pub struct App {
    doc: Document,
    history: History,
    selected: Vec<u64>,
    camera: Camera,
    tool: Tool,
    element: String,
    caption: String,
    smiles: String,
    isotope: String,
    grid: bool,
    analysis: Option<Analysis>,
    engine: PythonEngine,
    revision: u64,
    busy: bool,
    status: String,
    error: bool,
    path: Option<PathBuf>,
    saved: Document,
    pending: Option<Pending>,
    file_epoch: u64,
}
impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut app = Self {
            doc: Document::default(),
            history: History::default(),
            selected: vec![],
            camera: Camera::default(),
            tool: Tool::Select,
            element: "C".into(),
            caption: "Reaction conditions".into(),
            smiles: "CC(=O)Oc1ccccc1C(=O)O".into(),
            isotope: String::new(),
            grid: true,
            analysis: None,
            engine: PythonEngine::default(),
            revision: 0,
            busy: false,
            status: "Starting chemistry…".into(),
            error: false,
            path: None,
            saved: Document::default(),
            pending: None,
            file_epoch: 0,
        };
        let task = app.run(Request::import_smiles(&app.smiles), Job::Startup);
        (app, task)
    }
    pub fn title(&self) -> String {
        format!(
            "{}{} — Moruno",
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into()),
            if self.dirty() { " •" } else { "" }
        )
    }
    pub fn theme(&self) -> Theme {
        Theme::custom(
            "Moruno",
            iced::theme::Palette {
                background: Color::from_rgb8(243, 246, 243),
                text: Color::from_rgb8(35, 52, 51),
                primary: Color::from_rgb8(17, 126, 108),
                success: Color::from_rgb8(17, 126, 108),
                danger: Color::from_rgb8(182, 66, 61),
                warning: Color::from_rgb8(174, 120, 42),
            },
        )
    }
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::window::close_requests().map(Message::Close),
            iced::event::listen_with(|event, status, _window| {
                use iced::keyboard::{Key, key::Named};
                if status == iced::event::Status::Captured {
                    return None;
                }
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers: mods,
                    ..
                }) = event
                else {
                    return None;
                };
                match key {
                    Key::Character(c) if mods.command() => match c.as_str() {
                        "z" => Some(if mods.shift() {
                            Message::Redo
                        } else {
                            Message::Undo
                        }),
                        "s" => Some(Message::Save),
                        "o" => Some(Message::Open),
                        "n" => Some(Message::New),
                        "a" => Some(Message::SelectAll),
                        _ => None,
                    },
                    Key::Named(Named::Delete | Named::Backspace) => Some(Message::Delete),
                    Key::Named(Named::Escape) => Some(Message::Tool(Tool::Select)),
                    _ => None,
                }
            }),
        ])
    }
    fn dirty(&self) -> bool {
        self.doc != self.saved
    }
    fn run(&mut self, request: Request, kind: Job) -> Task<Message> {
        if self.busy {
            return Task::none();
        }
        self.busy = true;
        self.error = false;
        self.status = "Working…".into();
        let engine = self.engine.clone();
        let revision = self.revision;
        Task::perform(
            async move { engine.execute(request).await },
            move |result| Message::EngineDone {
                revision,
                kind: kind.clone(),
                result: Box::new(result),
            },
        )
    }
    fn changed(&mut self, before: Document) {
        if self.history.commit(before, &self.doc) {
            self.revision += 1;
            self.analysis = None;
            self.error = false;
            self.status = "Drawing changed · Check structure to refresh properties".into();
        }
        self.selected.retain(|id| self.doc.all_ids().contains(id));
    }
    fn fit(&mut self) {
        let (lo, hi) = self.doc.bounds();
        self.camera.center = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        self.camera.zoom = (600.0 / (hi.x - lo.x).max(180.0))
            .min(450.0 / (hi.y - lo.y).max(140.0))
            .clamp(0.25, 2.0);
    }
    fn pending(&mut self, action: Pending) -> Task<Message> {
        if self.dirty() {
            self.pending = Some(action);
            Task::none()
        } else {
            self.perform(action)
        }
    }
    fn perform(&mut self, action: Pending) -> Task<Message> {
        match action {
            Pending::New => {
                self.file_epoch += 1;
                self.revision += 1;
                let before = self.doc.clone();
                self.doc = Document::default();
                self.changed(before);
                self.saved = self.doc.clone();
                self.path = None;
                self.selected.clear();
                self.camera = Camera::default();
                self.status = "New document".into();
                Task::none()
            }
            Pending::Open => Task::perform(
                async {
                    // Extension filters in the macOS panel can leave valid
                    // selections disabled. Validate the selected file ourselves.
                    let file = rfd::AsyncFileDialog::new()
                        .set_title("Open a Moruno, MOL, CDXML, or SMILES document")
                        .pick_file()
                        .await?;
                    let path = file.path().to_path_buf();
                    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string());
                    Some((path, contents))
                },
                Message::Opened,
            ),
            Pending::Close(id) => iced::window::close(id),
        }
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tool(tool) => {
                self.tool = tool;
                self.error = false;
            }
            Message::Element(e) => {
                self.element = e;
                self.tool = Tool::Atom;
            }
            Message::Caption(s) => self.caption = s,
            Message::Smiles(s) => self.smiles = s,
            Message::Isotope(s) => self.isotope = s,
            Message::Canvas(edit) => self.edit(edit),
            Message::Grid => self.grid = !self.grid,
            Message::Fit => self.fit(),
            Message::Zoom(f) => self.camera.zoom = (self.camera.zoom * f).clamp(0.25, 5.0),
            Message::Import => return self.run(Request::import_smiles(&self.smiles), Job::Import),
            Message::Example(smiles) => {
                self.smiles = smiles.into();
                return self.run(Request::import_smiles(smiles), Job::Import);
            }
            Message::Analyze => {
                return self.run(Request::molecule("analyze", self.doc.clone()), Job::Analyze);
            }
            Message::Clean => {
                return self.run(Request::molecule("clean", self.doc.clone()), Job::Clean);
            }
            Message::EngineDone {
                revision,
                kind,
                result,
            } => {
                self.busy = false;
                match *result {
                    Err(e) => {
                        self.error = true;
                        self.status = e;
                    }
                    Ok(response) => {
                        if let Job::Export(format) = kind {
                            return export_file(response.output.unwrap_or_default(), format);
                        }
                        if self.revision != revision {
                            self.status="Operation finished; newer edits were preserved. Run it again to update.".into();
                            return Task::none();
                        }
                        if let Some(document) = response.document {
                            let before = self.doc.clone();
                            self.doc = document;
                            self.changed(before);
                            self.selected.clear();
                            if matches!(kind, Job::Startup | Job::Import | Job::ImportFile) {
                                self.fit();
                            }
                            if matches!(kind, Job::ImportFile) {
                                self.path = None;
                                self.saved = Document::default();
                                self.file_epoch += 1;
                            }
                            if matches!(kind, Job::Startup) {
                                self.saved = self.doc.clone();
                                self.history = History::default();
                            }
                        }
                        self.analysis = response.analysis;
                        self.status = match kind {
                            Job::Clean => "Structure cleaned",
                            Job::Analyze => "No chemistry errors found",
                            Job::Startup => {
                                "Ready · Try editing the example or start a new drawing"
                            }
                            _ => "Structure imported · Undo restores the previous drawing",
                        }
                        .into();
                        self.error = false;
                    }
                }
            }
            Message::Undo | Message::Redo => {
                let changed = if matches!(message, Message::Undo) {
                    self.history.undo(&mut self.doc)
                } else {
                    self.history.redo(&mut self.doc)
                };
                if changed {
                    self.revision += 1;
                    self.analysis = None;
                    self.selected.clear();
                    self.status = "History restored".into();
                    self.error = false;
                }
            }
            Message::Delete => {
                let before = self.doc.clone();
                self.doc.delete(&self.selected);
                self.changed(before);
            }
            Message::SelectAll => self.selected = self.doc.all_ids(),
            Message::Charge(delta) => {
                let before = self.doc.clone();
                self.doc.invalidate_chemistry(&self.selected);
                for id in &self.selected {
                    if let Some(a) = self.doc.atom_mut(*id) {
                        a.charge = (a.charge + delta).clamp(-8, 8);
                    }
                }
                self.changed(before);
            }
            Message::ApplyIsotope => match self.isotope.parse::<u32>() {
                Ok(value) if value <= 300 => {
                    let before = self.doc.clone();
                    for id in &self.selected {
                        if let Some(a) = self.doc.atom_mut(*id) {
                            a.isotope = value;
                        }
                    }
                    self.changed(before);
                }
                _ => {
                    self.status = "Enter an isotope mass number from 0 to 300 (0 clears it)".into();
                    self.error = true;
                }
            },
            Message::CopySmiles => {
                if let Some(a) = &self.analysis {
                    return iced::clipboard::write(a.smiles.clone());
                }
            }
            Message::New => return self.pending(Pending::New),
            Message::Open => return self.pending(Pending::Open),
            Message::Close(id) => return self.pending(Pending::Close(id)),
            Message::Cancel => self.pending = None,
            Message::Discard => {
                if let Some(action) = self.pending.take() {
                    return self.perform(action);
                }
            }
            Message::Opened(file) => {
                if let Some((path, result)) = file {
                    match result {
                        Err(e) => {
                            self.error = true;
                            self.status = e;
                        }
                        Ok(contents) => {
                            let extension = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or_default()
                                .to_ascii_lowercase();
                            if extension == "moruno" {
                                match serde_json::from_str::<Document>(&contents)
                                    .map_err(|e| e.to_string())
                                    .and_then(|doc| {
                                        doc.validate()?;
                                        Ok(doc)
                                    }) {
                                    Ok(doc) => {
                                        self.file_epoch += 1;
                                        self.doc = doc;
                                        self.saved = self.doc.clone();
                                        self.path = Some(path);
                                        self.history = History::default();
                                        self.revision += 1;
                                        self.analysis = None;
                                        self.selected.clear();
                                        self.fit();
                                        self.status = "Document opened".into();
                                        self.error = false;
                                    }
                                    Err(e) => {
                                        self.error = true;
                                        self.status = format!("Could not open document: {e}");
                                    }
                                }
                            } else {
                                let format = match extension.as_str() {
                                    "mol" => "mol",
                                    "cdxml" => "cdxml",
                                    _ => "smiles",
                                };
                                return self
                                    .run(Request::import(format, &contents), Job::ImportFile);
                            }
                        }
                    }
                }
            }
            Message::Save => {
                let path = self.path.clone();
                let bytes = match serde_json::to_vec_pretty(&self.doc) {
                    Ok(b) => b,
                    Err(e) => {
                        self.status = e.to_string();
                        self.error = true;
                        return Task::none();
                    }
                };
                let snapshot = self.doc.clone();
                let epoch = self.file_epoch;
                return Task::perform(
                    async move {
                        let path = if let Some(p) = path {
                            p
                        } else {
                            let Some(file) = rfd::AsyncFileDialog::new()
                                .add_filter("Moruno document", &["moruno"])
                                .set_file_name("Untitled.moruno")
                                .save_file()
                                .await
                            else {
                                return Ok(None);
                            };
                            file.path().to_path_buf()
                        };
                        moruno::storage::write_atomic(&path, &bytes)?;
                        Ok(Some(path))
                    },
                    move |result| Message::Saved(epoch, snapshot.clone(), result),
                );
            }
            Message::Saved(epoch, snapshot, result) => match result {
                Ok(Some(path)) => {
                    if epoch != self.file_epoch {
                        self.status = "Previous document saved".into();
                        return Task::none();
                    }
                    self.saved = snapshot;
                    self.path = Some(path);
                    self.status = "Document saved".into();
                    self.error = false;
                    if !self.dirty()
                        && let Some(action) = self.pending.take()
                    {
                        return self.perform(action);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            },
            Message::Export(format) => {
                if format == "svg" {
                    return export_file(scene::svg(&self.doc), format);
                }
                let mut request = Request::molecule("export", self.doc.clone());
                request.format = Some(format.into());
                return self.run(request, Job::Export(format));
            }
            Message::Exported(result) => match result {
                Ok(Some(path)) => {
                    self.status = format!(
                        "Exported {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    self.error = false;
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            },
        }
        Task::none()
    }
    fn edit(&mut self, edit: Edit) {
        let before = self.doc.clone();
        match edit {
            Edit::Select(ids) => self.selected = ids,
            Edit::Move(ids, dx, dy) => {
                self.doc.translate(&ids, dx, dy);
                self.selected = ids;
            }
            Edit::Pan(dx, dy) => self.camera.center = self.camera.center.offset(-dx, -dy),
            Edit::Zoom(f, at) => {
                let old = self.camera.zoom;
                self.camera.zoom = (old * f).clamp(0.25, 5.0);
                let ratio = old / self.camera.zoom;
                self.camera.center = Point::new(
                    at.x + (self.camera.center.x - at.x) * ratio,
                    at.y + (self.camera.center.y - at.y) * ratio,
                );
            }
            Edit::Bond(start, end, a, b) => {
                if self.tool == Tool::Arrow {
                    let id = self.doc.next_id();
                    self.doc.arrows.push(Arrow { id, start, end });
                    self.selected = vec![id];
                } else {
                    let a = a.unwrap_or_else(|| self.doc.add_atom("C", start));
                    let b = b.unwrap_or_else(|| self.doc.add_atom(&self.element, end));
                    let (order, display) = self.bond_style();
                    self.doc.add_bond(a, b, order, display);
                    self.selected = vec![b];
                }
            }
            Edit::Click(p) => {
                let hit = canvas::hit_object(&self.doc, p, 10.0 / self.camera.zoom);
                match self.tool {
                    Tool::Atom => {
                        if let Some(id) = self.doc.nearest(p, 10.0 / self.camera.zoom) {
                            self.doc.invalidate_chemistry(&[id]);
                            if let Some(a) = self.doc.atom_mut(id) {
                                a.element = self.element.clone();
                                a.explicit_h = 0;
                                a.no_implicit = false;
                                a.charge = 0;
                                a.isotope = 0;
                            }
                            self.selected = vec![id];
                        } else {
                            let id = self.doc.add_atom(&self.element, p);
                            self.selected = vec![id];
                        }
                    }
                    Tool::Bond(_) | Tool::Wedge | Tool::Hash => {
                        let atom = self.doc.nearest(p, 10.0 / self.camera.zoom);
                        let bond =
                            self.doc
                                .bonds
                                .iter()
                                .find(|b| {
                                    self.doc.atom(b.a).zip(self.doc.atom(b.b)).is_some_and(
                                        |(a, z)| {
                                            canvas::distance_to_segment(p, a.position, z.position)
                                                < 7.0 / self.camera.zoom
                                        },
                                    )
                                })
                                .cloned();
                        if let Some(b) = bond.filter(|_| atom.is_none()) {
                            let (order, display) = self.bond_style();
                            self.doc.add_bond(b.a, b.b, order, display);
                        } else {
                            let a = atom.unwrap_or_else(|| self.doc.add_atom("C", p));
                            let start = self.doc.atom(a).unwrap().position;
                            let b = self.doc.add_atom(&self.element, start.offset(36.37, -21.0));
                            let (order, display) = self.bond_style();
                            self.doc.add_bond(a, b, order, display);
                        }
                    }
                    Tool::Ring => {
                        let anchor = self.doc.nearest(p, 10.0 / self.camera.zoom);
                        let center = anchor
                            .and_then(|id| self.doc.atom(id).map(|a| a.position.offset(-42.0, 0.0)))
                            .unwrap_or(p);
                        let mut ids = vec![];
                        for i in 0..6 {
                            let angle = i as f32 * std::f32::consts::PI / 3.0;
                            ids.push(if let Some(anchor) = anchor.filter(|_| i == 0) {
                                anchor
                            } else {
                                self.doc.add_atom(
                                    "C",
                                    center.offset(angle.cos() * 42.0, angle.sin() * 42.0),
                                )
                            });
                        }
                        for i in 0..6 {
                            self.doc.add_bond(ids[i], ids[(i + 1) % 6], 1, "plain");
                        }
                        self.selected = ids;
                    }
                    Tool::Text => {
                        if !self.caption.trim().is_empty() {
                            let id = self.doc.next_id();
                            self.doc.annotations.push(Annotation {
                                id,
                                position: p,
                                text: self.caption.clone(),
                            });
                            self.selected = vec![id];
                        }
                    }
                    Tool::Erase => {
                        if let Some(id) = hit {
                            self.doc.delete(&[id]);
                        } else {
                            let index = self.doc.bonds.iter().position(|b| {
                                self.doc
                                    .atom(b.a)
                                    .zip(self.doc.atom(b.b))
                                    .is_some_and(|(a, z)| {
                                        canvas::distance_to_segment(p, a.position, z.position)
                                            < 7.0 / self.camera.zoom
                                    })
                            });
                            if let Some(i) = index {
                                let b = self.doc.bonds.remove(i);
                                self.doc.invalidate_chemistry(&[b.a, b.b]);
                            }
                        }
                    }
                    _ => self.selected = hit.into_iter().collect(),
                }
            }
        }
        self.changed(before);
    }
    fn bond_style(&self) -> (u8, &'static str) {
        match self.tool {
            Tool::Bond(n) => (n, "plain"),
            Tool::Wedge => (1, "wedge"),
            Tool::Hash => (1, "hash"),
            _ => (1, "plain"),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = row![
            column![
                text("moruno").size(28),
                text("MOLECULAR WORKSPACE").size(9).color(muted())
            ]
            .spacing(1),
            Space::new().width(Length::Fill),
            button("New").on_press(Message::New).style(button::text),
            button("Open").on_press(Message::Open).style(button::text),
            button("Save").on_press(Message::Save).padding([9, 18])
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12);
        let history = row![
            button("Undo")
                .on_press_maybe(self.history.can_undo().then_some(Message::Undo))
                .style(button::secondary),
            button("Redo")
                .on_press_maybe(self.history.can_redo().then_some(Message::Redo))
                .style(button::secondary),
            Space::new().width(Length::Fill),
            button(if self.busy {
                "Working…"
            } else {
                "Check structure"
            })
            .on_press_maybe((!self.busy).then_some(Message::Analyze))
            .style(button::secondary),
            button("Clean up").on_press_maybe((!self.busy).then_some(Message::Clean))
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let tools = [
            ("Select / move", Tool::Select),
            ("Single bond", Tool::Bond(1)),
            ("Double bond", Tool::Bond(2)),
            ("Triple bond", Tool::Bond(3)),
            ("Solid wedge", Tool::Wedge),
            ("Hashed wedge", Tool::Hash),
            ("Six-member ring", Tool::Ring),
            ("Reaction arrow", Tool::Arrow),
            ("Text label", Tool::Text),
            ("Eraser", Tool::Erase),
        ];
        let mut tool_list = column![section("DRAW")].spacing(5);
        for (name, tool) in tools {
            tool_list = tool_list.push(
                button(text(name).size(13))
                    .width(Length::Fill)
                    .padding([8, 12])
                    .style(if self.tool == tool {
                        button::primary
                    } else {
                        button::text
                    })
                    .on_press(Message::Tool(tool)),
            );
        }
        let mut elements = column![section("ELEMENT")].spacing(5);
        for group in [["C", "N", "O"], ["S", "P", "F"], ["Cl", "Br", "I"]] {
            let mut r = row![].spacing(4);
            for e in group {
                r = r.push(
                    button(text(e).size(14))
                        .width(Length::Fill)
                        .on_press(Message::Element(e.into()))
                        .style(if self.element == e && self.tool == Tool::Atom {
                            button::primary
                        } else {
                            button::secondary
                        }),
                );
            }
            elements = elements.push(r);
        }
        let left = column![
            tool_list,
            Space::new().height(12),
            elements,
            Space::new().height(12),
            section("ANNOTATION"),
            text_input("Text label", &self.caption)
                .on_input(Message::Caption)
                .size(12),
            Space::new().height(12),
            section("SELECTION"),
            row![
                button("Charge −")
                    .on_press(Message::Charge(-1))
                    .style(button::secondary),
                button("+")
                    .on_press(Message::Charge(1))
                    .style(button::secondary)
            ]
            .spacing(5),
            row![
                text_input("Isotope", &self.isotope)
                    .on_input(Message::Isotope)
                    .size(12),
                button("Set")
                    .on_press(Message::ApplyIsotope)
                    .style(button::secondary)
            ]
            .spacing(5),
            button("Delete selected")
                .on_press_maybe((!self.selected.is_empty()).then_some(Message::Delete))
                .style(button::text)
        ]
        .spacing(8);
        let mut properties = column![
            section("STRUCTURE"),
            text(format!(
                "{} atoms  ·  {} bonds",
                self.doc.atoms.len(),
                self.doc.bonds.len()
            ))
            .size(13),
            Space::new().height(12)
        ]
        .spacing(8);
        if let Some(a) = &self.analysis {
            properties = properties
                .push(text(&a.formula).size(29))
                .push(text("Molecular formula").size(11).color(muted()))
                .push(Space::new().height(8));
            for (label, value) in [
                ("Molecular weight", format!("{:.3} g/mol", a.mass)),
                ("Exact mass", format!("{:.5} Da", a.exact_mass)),
                ("cLogP", format!("{:.2}", a.logp)),
                ("Polar surface", format!("{:.2} Å²", a.tpsa)),
                ("H-bond donors", a.donors.to_string()),
                ("H-bond acceptors", a.acceptors.to_string()),
                ("Rings", a.rings.to_string()),
            ] {
                properties = properties.push(
                    column![text(label).size(11).color(muted()), text(value).size(15)].spacing(2),
                );
            }
            properties = properties
                .push(Space::new().height(10))
                .push(section("CANONICAL SMILES"))
                .push(text(&a.smiles).size(11))
                .push(
                    button("Copy SMILES")
                        .on_press(Message::CopySmiles)
                        .style(button::text),
                );
        } else {
            properties = properties.push(
                text("Check your structure to calculate molecular properties.")
                    .size(13)
                    .color(muted()),
            );
        }
        properties = properties
            .push(Space::new().height(20))
            .push(section("EXPORT"));
        for (label, format) in [
            ("SVG drawing", "svg"),
            ("MOL structure", "mol"),
            ("CDXML drawing", "cdxml"),
            ("SMILES text", "smiles"),
            ("InChI text", "inchi"),
        ] {
            properties = properties.push(
                button(text(label).size(12))
                    .on_press_maybe((!self.busy).then_some(Message::Export(format)))
                    .style(button::secondary)
                    .width(Length::Fill),
            );
        }
        let input = row![
            text_input("Paste a SMILES structure…", &self.smiles)
                .on_input(Message::Smiles)
                .on_submit(Message::Import)
                .size(13)
                .padding(10),
            button("Import")
                .on_press_maybe((!self.busy).then_some(Message::Import))
                .padding([10, 16])
        ]
        .spacing(8);
        let examples = row![
            text("EXAMPLES").size(10).color(muted()),
            button("Ethanol")
                .on_press(Message::Example("CCO"))
                .style(button::text),
            button("Benzene")
                .on_press(Message::Example("c1ccccc1"))
                .style(button::text),
            button("Aspirin")
                .on_press(Message::Example("CC(=O)Oc1ccccc1C(=O)O"))
                .style(button::text),
            button("Caffeine")
                .on_press(Message::Example("Cn1c(=O)c2c(ncn2C)n(C)c1=O"))
                .style(button::text)
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        let canvas = drawing(MoleculeCanvas {
            doc: &self.doc,
            selected: &self.selected,
            tool: self.tool,
            camera: self.camera,
            grid: self.grid,
        })
        .width(Length::Fill)
        .height(Length::Fill);
        let canvas: Element<'_, Edit> = canvas.into();
        let footer = row![
            text(self.tool.hint()).size(11).color(muted()),
            Space::new().width(Length::Fill),
            button(if self.grid { "Grid on" } else { "Grid off" })
                .on_press(Message::Grid)
                .style(button::text),
            button("−").on_press(Message::Zoom(0.8)).style(button::text),
            text(format!("{:.0}%", self.camera.zoom * 100.0)).size(11),
            button("+")
                .on_press(Message::Zoom(1.25))
                .style(button::text),
            button("Fit").on_press(Message::Fit).style(button::text)
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        let center = column![
            history,
            input,
            examples,
            container(canvas.map(Message::Canvas))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(sheet),
            footer
        ]
        .spacing(10);
        let body = row![
            container(scrollable(left))
                .padding(16)
                .width(194)
                .height(Length::Fill)
                .style(panel),
            container(center)
                .padding([16, 20])
                .width(Length::Fill)
                .height(Length::Fill),
            container(scrollable(properties))
                .padding(18)
                .width(224)
                .height(Length::Fill)
                .style(panel)
        ]
        .height(Length::Fill);
        let mut content = column![container(header).padding([16, 22]).style(panel)];
        if self.pending.is_some() {
            content = content.push(
                container(
                    row![
                        text("This document has unsaved changes.").size(13),
                        Space::new().width(Length::Fill),
                        button("Save").on_press(Message::Save),
                        button("Discard & continue")
                            .on_press(Message::Discard)
                            .style(button::danger),
                        button("Cancel")
                            .on_press(Message::Cancel)
                            .style(button::secondary)
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                )
                .padding([10, 20]),
            );
        }
        content
            .push(body)
            .push(
                container(text(&self.status).size(12).color(if self.error {
                    Color::from_rgb8(169, 48, 42)
                } else {
                    muted()
                }))
                .padding([10, 22])
                .width(Length::Fill),
            )
            .into()
    }
}
fn export_file(contents: String, format: &'static str) -> Task<Message> {
    Task::perform(
        async move {
            let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("Export", &[format])
                .set_file_name(format!("Molecule.{format}"))
                .save_file()
                .await
            else {
                return Ok(None);
            };
            let path = file.path().to_path_buf();
            moruno::storage::write_atomic(&path, contents.as_bytes())?;
            Ok(Some(path))
        },
        Message::Exported,
    )
}
fn muted() -> Color {
    Color::from_rgb8(102, 121, 117)
}
fn section(label: &str) -> iced::widget::Text<'_> {
    text(label).size(10).color(muted())
}
fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(249, 250, 247).into()),
        border: iced::Border {
            color: Color::from_rgb8(225, 233, 227),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
fn sheet(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.into()),
        border: iced::Border {
            color: Color::from_rgb8(215, 224, 217),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_save_does_not_mark_newer_edits_as_saved() {
        let (mut app, _) = App::new();
        let snapshot = app.doc.clone();
        app.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Saved(
            0,
            snapshot,
            Ok(Some("example.moruno".into())),
        ));
        assert!(app.dirty());
    }

    #[test]
    fn late_save_does_not_retarget_another_document() {
        let (mut app, _) = App::new();
        let snapshot = app.doc.clone();
        let _ = app.perform(Pending::New);
        let _ = app.update(Message::Saved(
            0,
            snapshot,
            Ok(Some("previous.moruno".into())),
        ));
        assert!(app.path.is_none());
    }

    #[test]
    fn new_document_invalidates_inflight_startup_even_when_empty() {
        let (mut app, _) = App::new();
        let revision = app.revision;
        let _ = app.perform(Pending::New);
        let mut old = Document::default();
        old.add_atom("O", Point::default());
        let _ = app.update(Message::EngineDone {
            revision,
            kind: Job::Startup,
            result: Box::new(Ok(Response {
                document: Some(old),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            })),
        });
        assert!(app.doc.atoms.is_empty());
    }
}
