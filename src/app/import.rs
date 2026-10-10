//! The Import tab, and structure or picture files chosen or dropped on the window.
use super::workspace::{caret, hover_hint, muted_text};
use super::{App, Message};
use crate::canvas::Tool;
use iced::widget::canvas::{self, Geometry, Path as Outline, Stroke};
use iced::widget::{column, container, row, stack, text, text_editor, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Rectangle, Renderer, Task, Theme, mouse};
use reshiki::document::{Document, Point};
use reshiki::engine::{ChemistryEngine, LocalEngine, Request};
use reshiki::pictures::Picture;
use std::path::{Path, PathBuf};

/// Widget id of the text box, focused whenever the tab opens.
pub(super) const INPUT: &str = "import-input";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputKind {
    #[default]
    Structure,
    Name,
}
/// File extensions inserted as structures, with their import format.
const STRUCTURES: [(&str, &str); 6] = [
    ("mol", "mol"),
    ("rxn", "rxn"),
    ("cdxml", "cdxml"),
    ("cdx", "cdx"),
    ("smi", "smiles"),
    ("smiles", "smiles"),
];
const PICTURES: [&str; 6] = ["png", "jpg", "jpeg", "tif", "tiff", "webp"];

#[derive(Debug, Clone, Copy, PartialEq)]
struct Example(&'static str, &'static str);
impl std::fmt::Display for Example {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
const EXAMPLES: [Example; 4] = [
    Example("Ethanol", "CCO"),
    Example("Benzene", "c1ccccc1"),
    Example("Aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
    Example("Caffeine", "Cn1c(=O)c2c(ncn2C)n(C)c1=O"),
];

#[derive(Debug, Clone)]
pub enum Action {
    InputKind(InputKind),
    Edit(text_editor::Action),
    ReplaceText(String),
    /// Opens or closes the Insert ▾ menu.
    Menu(bool),
    ExamplesMenu(bool),
    Choose,
    /// Chosen or dropped files, imported together.
    Files(Vec<PathBuf>),
    Hovered(PathBuf),
    Dropped(PathBuf),
    Left,
    Loaded(Ticket, Box<Result<Batch, String>>),
}

#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    epoch: u64,
    revision: u64,
}

/// Drawings read from files, in file order.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    label: String,
    drawings: Vec<Document>,
    warnings: Vec<String>,
}

#[derive(Default)]
pub struct State {
    pub kind: InputKind,
    pub input: text_editor::Content,
    /// Import format of the text in the box, or None while it is blank.
    format: Option<&'static str>,
    pub menu: bool,
    pub examples_menu: bool,
    hovered: Vec<PathBuf>,
    dropped: Vec<PathBuf>,
}
impl State {
    #[cfg(test)]
    pub fn is_blank(&self) -> bool {
        self.format.is_none()
    }
    pub fn set_text(&mut self, value: &str) {
        self.input = text_editor::Content::with_text(value);
        self.format = detect(value);
    }
}

fn detect(text: &str) -> Option<&'static str> {
    (!text.trim().is_empty()).then(|| reshiki::clipboard::text_format(text))
}
fn format_name(format: &str) -> &'static str {
    match format {
        "inchi" => "InChI",
        "rxn" => "RXN",
        "mol" => "MOL",
        "cdxml" => "CDXML",
        "rsmi" => "Reaction SMILES",
        _ => "SMILES",
    }
}

/// File drag events from the window.
pub(super) fn drag_event(event: &iced::window::Event) -> Option<Message> {
    use iced::window::Event as E;
    Some(Message::Imports(match event {
        E::FileHovered(path) => Action::Hovered(path.clone()),
        E::FileDropped(path) => Action::Dropped(path.clone()),
        E::FilesHoveredLeft => Action::Left,
        _ => return None,
    }))
}

#[derive(Debug, PartialEq)]
enum Kind {
    Structure(&'static str),
    Picture,
    Document,
    Unsupported,
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}
fn kind(path: &Path) -> Kind {
    let extension = extension(path);
    if let Some((_, format)) = STRUCTURES.iter().find(|(e, _)| *e == extension) {
        Kind::Structure(format)
    } else if PICTURES.contains(&extension.as_str()) {
        Kind::Picture
    } else if reshiki::compatibility::is_native_extension(&extension) {
        Kind::Document
    } else {
        Kind::Unsupported
    }
}
fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
fn describe(paths: &[PathBuf]) -> String {
    match paths {
        [path] => name(path),
        _ => format!("{} files", paths.len()),
    }
}

/// What dropping or choosing these files does.
#[derive(Debug, PartialEq)]
enum Plan {
    Insert(Vec<PathBuf>),
    /// Drawings, each opened in a tab.
    Open(Vec<PathBuf>),
    Reject(String),
}
fn plan(paths: &[PathBuf]) -> Plan {
    if let Some(path) = paths.iter().find(|p| kind(p) == Kind::Unsupported) {
        return Plan::Reject(match extension(path) {
            e if e.is_empty() => "Can't import this file".into(),
            e => format!("Can't import .{e}"),
        });
    }
    let documents = paths.iter().filter(|p| kind(p) == Kind::Document).count();
    if documents == paths.len() {
        Plan::Open(paths.to_vec())
    } else if documents > 0 {
        Plan::Reject("Drop drawings apart from structures and pictures".into())
    } else {
        Plan::Insert(paths.to_vec())
    }
}
impl Plan {
    fn label(&self) -> String {
        match self {
            Plan::Insert(paths) => format!("Drop to insert · {}", describe(paths)),
            Plan::Open(paths) => format!("Drop to open · {}", describe(paths)),
            Plan::Reject(reason) => reason.clone(),
        }
    }
}

/// Structure file contents as the engine reads them: binary CDX as base64.
pub(super) fn contents(format: &str, bytes: Vec<u8>) -> Result<String, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if format == "cdx" {
        Ok(STANDARD.encode(bytes))
    } else {
        String::from_utf8(bytes).map_err(|error| format!("The file is not valid UTF-8: {error}"))
    }
}

/// A drawing holding just this picture, sized as a newly inserted picture.
fn picture_drawing(picture: &Picture) -> Document {
    let mut drawing = Document::default();
    let id = drawing.next_id();
    drawing.graphics.push(picture.graphic(id, Point::default()));
    drawing
}

async fn read(engine: &LocalEngine, path: &Path) -> Result<(Document, Vec<String>), String> {
    match kind(path) {
        Kind::Picture => {
            let path = path.to_owned();
            let picture = tokio::task::spawn_blocking(move || Picture::open(&path))
                .await
                .map_err(|e| e.to_string())??;
            Ok((picture_drawing(&picture), vec![]))
        }
        Kind::Structure(format) => {
            let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
            let response = engine
                .execute(Request::import(format, &contents(format, bytes)?))
                .await?;
            let drawing = response.document.ok_or("The file contains no drawing")?;
            if reshiki::scene::selection_bounds(&drawing, &drawing.all_ids()).is_none() {
                return Err("The file contains no drawing".into());
            }
            Ok((drawing, response.warnings))
        }
        Kind::Document | Kind::Unsupported => Err("Unsupported file type".into()),
    }
}
async fn load(engine: LocalEngine, paths: Vec<PathBuf>) -> Result<Batch, String> {
    let mut batch = Batch {
        label: describe(&paths),
        ..Default::default()
    };
    for path in &paths {
        let (drawing, warnings) = read(&engine, path)
            .await
            .map_err(|e| format!("Could not import {}: {e}", name(path)))?;
        batch.drawings.push(drawing);
        for warning in warnings {
            if !batch.warnings.contains(&warning) {
                batch.warnings.push(warning);
            }
        }
    }
    Ok(batch)
}
async fn choose() -> Vec<PathBuf> {
    let extensions: Vec<_> = STRUCTURES.iter().map(|(e, _)| *e).chain(PICTURES).collect();
    rfd::AsyncFileDialog::new()
        .set_title("Insert structures or pictures")
        .add_filter("Structures and pictures", &extensions)
        .pick_files()
        .await
        .map(|files| files.iter().map(|f| f.path().to_path_buf()).collect())
        .unwrap_or_default()
}

impl App {
    /// Drag feedback, which never changes the drawing. The drop itself passes
    /// the same checks as other imports.
    pub(super) fn import_drag(&mut self, action: Action) -> Task<Message> {
        let state = &mut self.imports;
        match action {
            Action::Hovered(path) if !state.hovered.contains(&path) => state.hovered.push(path),
            Action::Left => {
                state.hovered.clear();
                state.dropped.clear();
            }
            Action::Dropped(path) => {
                state.dropped.push(path);
                // Each hovered file arrives as its own drop; import them together.
                if state.dropped.len() >= state.hovered.len() {
                    state.hovered.clear();
                    let paths = std::mem::take(&mut state.dropped);
                    return self.update(Message::Imports(Action::Files(paths)));
                }
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn import_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::InputKind(kind) => {
                self.imports.kind = kind;
                self.imports.menu = false;
                self.imports.examples_menu = false;
                if kind == InputKind::Structure {
                    self.cancel_name_import();
                }
                return iced::widget::operation::focus(self.import_input_id());
            }
            Action::Edit(action) => {
                let edit = action.is_edit();
                self.imports.input.perform(action);
                if edit {
                    self.imports.format = detect(&self.imports.input.text());
                }
            }
            Action::ReplaceText(value) => {
                // Use the editor's actual selection/paste path, preserving the
                // same detection and content semantics as keyboard editing.
                self.imports.input.perform(text_editor::Action::SelectAll);
                self.imports
                    .input
                    .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                        std::sync::Arc::new(value),
                    )));
                self.imports.format = detect(&self.imports.input.text());
            }
            Action::Menu(open) => {
                self.imports.menu = open;
                self.imports.examples_menu = false;
            }
            Action::ExamplesMenu(open) => {
                self.imports.examples_menu = open;
                self.imports.menu = false;
            }
            Action::Choose => {
                return Task::perform(choose(), |paths| Message::Imports(Action::Files(paths)));
            }
            Action::Files(paths) if paths.is_empty() => {}
            Action::Files(paths) => match plan(&paths) {
                Plan::Reject(reason) => {
                    self.error = true;
                    self.status = reason;
                }
                Plan::Open(paths) => return super::files::open_paths(paths),
                Plan::Insert(paths) => {
                    if self.tab.busy {
                        self.error = true;
                        self.status = "Wait for the current operation, then import again".into();
                        return Task::none();
                    }
                    self.tab.busy = true;
                    self.error = false;
                    self.status = "Importing…".into();
                    let ticket = Ticket {
                        epoch: self.tab.file_epoch,
                        revision: self.tab.revision,
                    };
                    return Task::perform(load(self.engine.clone(), paths), move |result| {
                        Message::Imports(Action::Loaded(ticket, Box::new(result)))
                    });
                }
            },
            Action::Loaded(ticket, result) => {
                if ticket.epoch != self.tab.file_epoch {
                    return Task::none();
                }
                self.tab.busy = false;
                if ticket.revision != self.tab.revision
                    || self.tab.inline_text.is_some()
                    || self.tab.atom_text.is_some()
                    || self.tab.joining.is_some()
                    || self.tab.cleanup.is_some()
                {
                    self.status =
                        "Drawing changed while importing · Import again when ready".into();
                    return Task::none();
                }
                match *result {
                    Ok(batch) => self.insert_batch(batch),
                    Err(error) => {
                        self.error = true;
                        self.status = error;
                    }
                }
            }
            Action::Hovered(_) | Action::Dropped(_) | Action::Left => {
                return self.import_drag(action);
            }
        }
        Task::none()
    }

    /// Inserts the drawings side by side around the drop point as one Undo step.
    fn insert_batch(&mut self, batch: Batch) {
        let parts: Option<Vec<_>> = batch
            .drawings
            .iter()
            .map(|d| reshiki::scene::selection_bounds(d, &d.all_ids()).map(|b| (d, b)))
            .collect();
        let Some(parts) = parts.filter(|parts| !parts.is_empty()) else {
            self.error = true;
            self.status = format!(
                "Could not insert {}: a file contains no drawing",
                batch.label
            );
            return;
        };
        let gap = self.tab.doc.drawing_style.bond_length_world;
        let width = parts.iter().map(|(_, (lo, hi))| hi.x - lo.x).sum::<f32>()
            + gap * (parts.len() - 1) as f32;
        let height = parts
            .iter()
            .map(|(_, (lo, hi))| hi.y - lo.y)
            .fold(0., f32::max);
        let at = self.drop_point(width, height);
        let before = self.tab.doc.clone();
        let mut x = at.x - width / 2.;
        let mut selected = vec![];
        for (drawing, (lo, hi)) in parts {
            let offset = Point::new(x - lo.x, at.y - (lo.y + hi.y) / 2.);
            let ids = reshiki::editing::append(&mut self.tab.doc, drawing, offset);
            if ids.is_empty() {
                self.tab.doc = before;
                self.error = true;
                self.status = format!("Could not insert {}", batch.label);
                return;
            }
            selected.extend(ids);
            x += hi.x - lo.x + gap;
        }
        self.tab.selected = selected;
        self.error = false;
        self.changed(before);
        if self.error {
            self.tab.selected.clear();
            return;
        }
        self.tool = Tool::Select;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        self.reveal_inserted();
        self.status = format!(
            "Inserted {} · Drag to position · Delete or Undo to remove",
            batch.label
        );
        for warning in batch.warnings {
            self.status.push_str(" · ");
            self.status.push_str(&warning);
        }
    }

    /// Center for inserted drawings of this size: the pointer while it is over
    /// the canvas, or else the view center, moved inward so they stay in view.
    /// Window drag events carry no position; the pointer is known only once the
    /// system reports it again after the drop.
    fn drop_point(&self, width: f32, height: f32) -> Point {
        let at = self
            .tab
            .hover
            .filter(|&(_, epoch)| epoch == self.tab.file_epoch)
            .map_or(self.tab.camera.center, |(at, _)| at);
        let canvas = Rectangle::with_size(self.viewport);
        let paper = self.guides.paper(canvas);
        let camera = self.tab.camera;
        let world = |x: f32, y: f32| {
            Point::new(
                (x - canvas.width / 2.) / camera.zoom + camera.center.x,
                (y - canvas.height / 2.) / camera.zoom + camera.center.y,
            )
        };
        let lo = world(paper.x, paper.y);
        let hi = world(paper.x + paper.width, paper.y + paper.height);
        // Room for the selection handles, including the rotation handle above.
        let margin = 56. / camera.zoom;
        let fit = |v: f32, size: f32, lo: f32, hi: f32| {
            let (min, max) = (lo + size / 2. + margin, hi - size / 2. - margin);
            if min <= max {
                v.clamp(min, max)
            } else {
                (lo + hi) / 2.
            }
        };
        Point::new(fit(at.x, width, lo.x, hi.x), fit(at.y, height, lo.y, hi.y))
    }

    /// Moves the view only when the new selection would be clipped.
    fn reveal_inserted(&mut self) {
        let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.tab.doc, &self.tab.selected)
        else {
            return;
        };
        let canvas = Rectangle::with_size(self.viewport);
        let paper = self.guides.paper(canvas);
        if !(paper.contains(self.tab.camera.screen(lo, canvas))
            && paper.contains(self.tab.camera.screen(hi, canvas)))
        {
            self.reveal_bounds(lo, hi);
        }
    }

    pub(super) fn import_panel(&self) -> Element<'_, Message> {
        let kind = self.imports.kind;
        let modes = row![
            reshiki::accessibility::button(
                "import-kind-structure",
                "Import structure",
                text("Structure").size(12),
            )
            .checked(kind == InputKind::Structure)
            .style(super::workspace::control(kind == InputKind::Structure))
            .on_press(Message::Imports(Action::InputKind(InputKind::Structure))),
            reshiki::accessibility::button(
                "import-kind-name",
                "Import chemical name",
                text("Name").size(12),
            )
            .checked(kind == InputKind::Name)
            .style(super::workspace::control(kind == InputKind::Name))
            .on_press(Message::Imports(Action::InputKind(InputKind::Name))),
        ]
        .spacing(4);
        let body = match kind {
            InputKind::Structure => self.structure_import_panel(),
            InputKind::Name => self.name_import_panel(),
        };
        column![modes, body].spacing(12).into()
    }

    pub(super) fn import_input_id(&self) -> &'static str {
        match self.imports.kind {
            InputKind::Structure => INPUT,
            InputKind::Name => "import-name",
        }
    }

    fn structure_import_panel(&self) -> Element<'_, Message> {
        let state = &self.imports;
        let ready = state.format.is_some() && !self.tab.busy;
        let editor = text_editor(&state.input)
            .id(INPUT)
            .placeholder(format!(
                "Paste SMILES, reaction SMILES, InChI, MOL, RXN or CDXML\n{} inserts",
                super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Enter")
            ))
            .on_action(|a| Message::Imports(Action::Edit(a)))
            .key_binding(|key| {
                use iced::keyboard::{Key, key::Named};
                if matches!(key.status, text_editor::Status::Focused { .. })
                    && key.modifiers.command()
                    && key.key == Key::Named(Named::Enter)
                {
                    Some(text_editor::Binding::Custom(Message::InsertInput))
                } else {
                    text_editor::Binding::from_key_press(key)
                }
            })
            .height(120)
            .size(12)
            .padding(8)
            .wrapping(text::Wrapping::WordOrGlyph)
            .style(editor_style);
        let editor = reshiki::accessibility::editor(
            INPUT,
            "Structure or reaction to import",
            state.input.text(),
            editor,
            |value| Message::Imports(Action::ReplaceText(value)),
        );
        let split = |content: Element<'static, Message>, message, left: bool| {
            reshiki::accessibility::button(
                if left {
                    "import-insert"
                } else {
                    "import-insert-menu"
                },
                if left {
                    "Insert into drawing"
                } else {
                    "More ways to insert"
                },
                content,
            )
            .padding(if left { [6, 12] } else { [6, 7] })
            .style(move |theme: &Theme, status| {
                let mut style = crate::appearance::primary(theme, status);
                style.border.radius = if left {
                    iced::border::Radius::new(6).right(0)
                } else {
                    iced::border::Radius::new(6).left(0)
                };
                style
            })
            .on_press_maybe(ready.then_some(message))
        };
        let actions = row![
            container(
                text(
                    state
                        .format
                        .map(|f| format!("Detected: {}", format_name(f)))
                        .unwrap_or_default()
                )
                .size(11)
                .style(muted_text)
            )
            .width(Length::Fill),
            row![
                hover_hint(
                    split(text("Insert").size(12).into(), Message::InsertInput, true),
                    format!(
                        "Insert into the drawing · {}",
                        super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Enter")
                    ),
                    tooltip::Position::Top,
                ),
                insert_menu(
                    split(
                        // As tall as the Insert label beside it.
                        caret(9.)
                            .line_height(text::LineHeight::Absolute(15.6.into()))
                            .into(),
                        Message::Imports(Action::Menu(!state.menu)),
                        false
                    ),
                    state.menu,
                    ready
                ),
            ]
            .spacing(1),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        // 11 px keeps the three controls on one line in the 300 px inspector.
        let field = |id, label| {
            reshiki::accessibility::button(id, label, text(label).size(11))
                .padding([6, 7])
                .style(crate::appearance::secondary)
        };
        let files = row![
            hover_hint(
                field("import-choose-file", "Choose file…")
                    .on_press(Message::Imports(Action::Choose)),
                "MOL, RXN, CDXML, CDX, SMILES or pictures · Several files go side by side",
                tooltip::Position::Top,
            ),
            field("import-paste-picture", "Paste picture").on_press_maybe(
                (reshiki::clipboard::available() && !self.tab.clipboard_busy)
                    .then_some(Message::PastePicture)
            ),
            self.import_examples_menu(),
        ]
        .spacing(5);
        let hint =
            text("Or drop MOL, RXN, CDXML, CDX, SMILES or picture files anywhere on the drawing.")
                .size(11)
                .style(muted_text);
        column![editor, actions, files, hint].spacing(10).into()
    }

    /// Outlines the canvas while files are dragged over the window.
    fn import_examples_menu(&self) -> Element<'_, Message> {
        let open = self.imports.examples_menu;
        let anchor = super::popover::choice_anchor(
            "import-examples",
            "Import examples".into(),
            "Examples".into(),
            11.,
            [6, 7],
            open,
            Message::Imports(Action::ExamplesMenu(!open)),
        );
        let popup = open.then(|| {
            let items = EXAMPLES.into_iter().enumerate().map(|(index, example)| {
                reshiki::accessibility::button(
                    format!("import-example-{index}"),
                    example.0,
                    text(example.0).size(12),
                )
                .padding([6, 10])
                .width(Length::Fill)
                .style(super::workspace::control(false))
                .on_press(Message::Example(example.1))
                .into()
            });
            container(column(items))
                .width(170)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        Element::new(
            super::popover::popover(anchor, popup, Message::Imports(Action::ExamplesMenu(false)))
                .align_end(),
        )
    }

    pub(super) fn with_drop_overlay<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let layers = stack![base];
        if self.imports.hovered.is_empty() {
            return layers.into();
        }
        let plan = plan(&self.imports.hovered);
        let zone = Zone {
            error: matches!(plan, Plan::Reject(_)),
            dark: self.tab.doc.canvas_theme.is_dark(),
        };
        let accent = zone.colors(false).0;
        let tag = container(text(plan.label()).size(12))
            .padding([6, 9])
            .style(move |theme| {
                crate::appearance::container(
                    theme,
                    container::Style {
                        background: Some(Color::WHITE.into()),
                        border: Border {
                            color: accent,
                            width: 1.,
                            radius: 6.0.into(),
                        },
                        shadow: crate::appearance::surface_shadow(iced::Shadow {
                            color: Color::from_rgba(0., 0., 0., 0.25),
                            offset: iced::Vector::new(0., 6.),
                            blur_radius: 18.,
                        }),
                        ..Default::default()
                    },
                )
            });
        layers
            .push(
                canvas::Canvas::new(zone)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .push(container(tag).center(Length::Fill))
            .into()
    }
}

fn editor_style(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    use iced::widget::text_input::Status as Input;
    let field = crate::appearance::input_style(
        theme,
        match status {
            text_editor::Status::Active => Input::Active,
            text_editor::Status::Hovered => Input::Hovered,
            text_editor::Status::Focused { is_hovered } => Input::Focused { is_hovered },
            text_editor::Status::Disabled => Input::Disabled,
        },
    );
    text_editor::Style {
        background: field.background,
        border: field.border,
        placeholder: field.placeholder,
        value: field.value,
        selection: field.selection,
    }
}

/// The Insert ▾ menu, floating under its button like the other menus.
fn insert_menu(
    anchor: reshiki::accessibility::Button<'_, Message>,
    open: bool,
    ready: bool,
) -> Element<'_, Message> {
    let popup = open.then(|| {
        container(
            reshiki::accessibility::button(
                "import-replace",
                "Replace drawing",
                text("Replace drawing").size(12),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .style(super::workspace::control(false))
            .on_press_maybe(ready.then_some(Message::Import)),
        )
        .width(170)
        .padding(5)
        .style(super::color_popover::surface)
        .into()
    });
    Element::new(
        super::popover::popover(
            hover_hint(anchor, "More ways to insert", tooltip::Position::Top),
            popup,
            Message::Imports(Action::Menu(false)),
        )
        .align_end(),
    )
}

/// The dashed drop outline, colored for the canvas it covers.
struct Zone {
    error: bool,
    dark: bool,
}
impl Zone {
    /// Outline and wash colors on the light canvas, or the dark one.
    fn colors(&self, dark: bool) -> (Color, Color) {
        let rgb = Color::from_rgb8;
        match (self.error, dark) {
            (false, false) => (rgb(39, 87, 75), rgb(221, 239, 232)),
            (false, true) => (rgb(124, 199, 176), rgb(29, 58, 49)),
            (true, false) => (rgb(168, 52, 47), rgb(248, 228, 225)),
            (true, true) => (rgb(238, 140, 132), rgb(58, 34, 32)),
        }
    }
}
impl canvas::Program<Message> for Zone {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (line, wash) = self.colors(self.dark);
        let inset = 10.;
        let zone = Outline::rounded_rectangle(
            iced::Point::new(inset, inset),
            iced::Size::new(
                (bounds.width - 2. * inset).max(0.),
                (bounds.height - 2. * inset).max(0.),
            ),
            8.0.into(),
        );
        frame.fill(&zone, Color { a: 0.55, ..wash });
        frame.stroke(
            &zone,
            Stroke {
                line_dash: canvas::LineDash {
                    segments: &[8., 5.],
                    offset: 0,
                },
                ..Stroke::default().with_width(2.).with_color(line)
            },
        );
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
