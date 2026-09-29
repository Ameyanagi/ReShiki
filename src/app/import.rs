//! The Import tab, and structure or picture files chosen or dropped on the window.
use super::workspace::{caret, hover_hint, muted_text};
use super::{App, Message, Pending};
use crate::canvas::Tool;
use iced::widget::canvas::{self, Geometry, Path as Outline, Stroke};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, stack, text, text_editor, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length, Rectangle, Renderer, Task, Theme, mouse};
use reshiki::document::{Document, Point};
use reshiki::engine::{ChemistryEngine, LocalEngine, Request};
use reshiki::pictures::Picture;
use std::path::{Path, PathBuf};

/// Widget id of the text box, focused whenever the tab opens.
pub(super) const INPUT: &str = "import-input";
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
    Edit(text_editor::Action),
    /// Opens or closes the Insert ▾ menu.
    Menu(bool),
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
    pub input: text_editor::Content,
    /// Import format of the text in the box, or None while it is blank.
    format: Option<&'static str>,
    pub menu: bool,
    hovered: Vec<PathBuf>,
    dropped: Vec<PathBuf>,
}
impl State {
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
    Open(PathBuf),
    Reject(String),
}
fn plan(paths: &[PathBuf]) -> Plan {
    if let Some(path) = paths.iter().find(|p| kind(p) == Kind::Unsupported) {
        return Plan::Reject(match extension(path) {
            e if e.is_empty() => "Can't import this file".into(),
            e => format!("Can't import .{e}"),
        });
    }
    match paths {
        [path] if kind(path) == Kind::Document => Plan::Open(path.clone()),
        _ if paths.iter().any(|p| kind(p) == Kind::Document) => {
            Plan::Reject("Open one drawing at a time".into())
        }
        _ => Plan::Insert(paths.to_vec()),
    }
}
impl Plan {
    fn label(&self) -> String {
        match self {
            Plan::Insert(paths) => format!("Drop to insert · {}", describe(paths)),
            Plan::Open(path) => format!("Drop to open · {}", name(path)),
            Plan::Reject(reason) => reason.clone(),
        }
    }
}

/// Structure file contents as the engine reads them: binary CDX as base64.
pub(super) fn contents(format: &str, bytes: Vec<u8>) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if format == "cdx" {
        STANDARD.encode(bytes)
    } else {
        String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
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
                .execute(Request::import(format, &contents(format, bytes)))
                .await?;
            let drawing = response.document.ok_or("The file contains no drawing")?;
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
            Action::Edit(action) => {
                let edit = action.is_edit();
                self.imports.input.perform(action);
                if edit {
                    self.imports.format = detect(&self.imports.input.text());
                }
            }
            Action::Menu(open) => self.imports.menu = open,
            Action::Choose => {
                return Task::perform(choose(), |paths| Message::Imports(Action::Files(paths)));
            }
            Action::Files(paths) if paths.is_empty() => {}
            Action::Files(paths) => match plan(&paths) {
                Plan::Reject(reason) => {
                    self.error = true;
                    self.status = reason;
                }
                Plan::Open(path) => return self.pending(Pending::Open(Some(path))),
                Plan::Insert(paths) => {
                    if self.busy {
                        self.error = true;
                        self.status = "Wait for the current operation, then import again".into();
                        return Task::none();
                    }
                    self.busy = true;
                    self.error = false;
                    self.status = "Importing…".into();
                    let ticket = Ticket {
                        epoch: self.file_epoch,
                        revision: self.revision,
                    };
                    return Task::perform(load(self.engine.clone(), paths), move |result| {
                        Message::Imports(Action::Loaded(ticket, Box::new(result)))
                    });
                }
            },
            Action::Loaded(ticket, result) => {
                self.busy = false;
                if ticket.epoch != self.file_epoch {
                    return Task::none();
                }
                if ticket.revision != self.revision
                    || self.inline_text.is_some()
                    || self.atom_text.is_some()
                    || self.joining.is_some()
                    || self.cleanup.is_some()
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
        let parts: Vec<_> = batch
            .drawings
            .iter()
            .filter_map(|d| reshiki::scene::selection_bounds(d, &d.all_ids()).map(|b| (d, b)))
            .collect();
        if parts.is_empty() {
            self.error = true;
            self.status = format!("{} contains no drawing", batch.label);
            return;
        }
        let gap = self.doc.drawing_style.bond_length_world;
        let width = parts.iter().map(|(_, (lo, hi))| hi.x - lo.x).sum::<f32>()
            + gap * (parts.len() - 1) as f32;
        let height = parts
            .iter()
            .map(|(_, (lo, hi))| hi.y - lo.y)
            .fold(0., f32::max);
        let at = self.drop_point(width, height);
        let before = self.doc.clone();
        let mut x = at.x - width / 2.;
        let mut selected = vec![];
        for (drawing, (lo, hi)) in parts {
            let offset = Point::new(x - lo.x, at.y - (lo.y + hi.y) / 2.);
            let ids = reshiki::editing::append(&mut self.doc, drawing, offset);
            if ids.is_empty() {
                self.doc = before;
                self.error = true;
                self.status = format!("Could not insert {}", batch.label);
                return;
            }
            selected.extend(ids);
            x += hi.x - lo.x + gap;
        }
        self.selected = selected;
        self.error = false;
        self.changed(before);
        if self.error {
            self.selected.clear();
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
            .hover
            .filter(|&(_, epoch)| epoch == self.file_epoch)
            .map_or(self.camera.center, |(at, _)| at);
        let canvas = Rectangle::with_size(self.viewport);
        let paper = self.guides.paper(canvas);
        let camera = self.camera;
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
        let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.doc, &self.selected) else {
            return;
        };
        let canvas = Rectangle::with_size(self.viewport);
        let paper = self.guides.paper(canvas);
        if !(paper.contains(self.camera.screen(lo, canvas))
            && paper.contains(self.camera.screen(hi, canvas)))
        {
            self.reveal_bounds(lo, hi);
        }
    }

    pub(super) fn import_panel(&self) -> Element<'_, Message> {
        let state = &self.imports;
        let ready = state.format.is_some() && !self.busy;
        let editor = text_editor(&state.input)
            .id(INPUT)
            .placeholder(super::platform_shortcut(
                "Paste SMILES, reaction SMILES, InChI, MOL, RXN or CDXML\n⌘↩ inserts",
                "Paste SMILES, reaction SMILES, InChI, MOL, RXN or CDXML\nCtrl+Enter inserts",
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
        let split = |content: Element<'static, Message>, message, left: bool| {
            button(content)
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
                    super::platform_shortcut(
                        "Insert into the drawing · ⌘↩",
                        "Insert into the drawing · Ctrl+Enter"
                    ),
                    tooltip::Position::Top,
                ),
                hover_hint(
                    split(
                        // As tall as the Insert label beside it.
                        caret(9.)
                            .line_height(text::LineHeight::Absolute(15.6.into()))
                            .into(),
                        Message::Imports(Action::Menu(!state.menu)),
                        false
                    ),
                    "More ways to insert",
                    tooltip::Position::Top,
                ),
            ]
            .spacing(1),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        // 11 px keeps the three controls on one line in the 300 px inspector.
        let field = |label| {
            button(text(label).size(11))
                .padding([6, 7])
                .style(crate::appearance::secondary)
        };
        let files = row![
            hover_hint(
                field("Choose file…").on_press(Message::Imports(Action::Choose)),
                "MOL, RXN, CDXML, CDX, SMILES or pictures · Several files go side by side",
                tooltip::Position::Top,
            ),
            field("Paste picture").on_press_maybe(
                (reshiki::clipboard::available() && !self.clipboard_busy)
                    .then_some(Message::PastePicture)
            ),
            crate::appearance::pick_list(EXAMPLES, None::<Example>, |e| Message::Example(e.1))
                .placeholder("Examples")
                .text_size(11)
                .padding([6, 7])
                .width(Length::Fill)
                // A command menu, not an empty field.
                .style(|theme, status| {
                    let mut style = crate::appearance::dropdown(theme, status);
                    style.placeholder_color = style.text_color;
                    style
                }),
        ]
        .spacing(5);
        let hint =
            text("Or drop MOL, RXN, CDXML, CDX, SMILES or picture files anywhere on the drawing.")
                .size(11)
                .style(muted_text);
        // The Insert ▾ menu floats over the rows below it.
        let mut below = stack![column![files, hint].spacing(10)];
        if state.menu {
            let replace = button(text("Replace drawing").size(12))
                .padding([6, 10])
                .width(Length::Fill)
                .style(button::text)
                .on_press_maybe(ready.then_some(Message::Import));
            below = below
                .push(
                    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                        .on_press(Message::Imports(Action::Menu(false))),
                )
                .push(
                    container(opaque(
                        container(replace).padding(5).width(170).style(menu_style),
                    ))
                    .width(Length::Fill)
                    .align_x(Alignment::End),
                );
        }
        column![editor, actions, below].spacing(10).into()
    }

    /// Outlines the canvas while files are dragged over the window.
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
            dark: self.doc.canvas_theme.is_dark(),
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

/// Matches the pick list menus beside it.
fn menu_style(theme: &Theme) -> container::Style {
    let menu = crate::appearance::dropdown_menu(theme);
    container::Style {
        background: Some(menu.background),
        border: menu.border,
        shadow: menu.shadow,
        text_color: Some(menu.text_color),
        ..Default::default()
    }
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
mod tests {
    use super::*;
    use crate::app::InspectorTab;
    use crate::canvas::Edit;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }
    fn ready() -> App {
        let (mut app, _) = App::new();
        app.busy = false;
        app
    }

    #[test]
    fn files_route_by_extension_and_label_the_drop() {
        let insert = paths(&[
            "caffeine.MOL",
            "b.rxn",
            "c.cdxml",
            "d.cdx",
            "e.smi",
            "f.smiles",
        ]);
        assert_eq!(plan(&insert), Plan::Insert(insert.clone()));
        assert_eq!(plan(&insert).label(), "Drop to insert · 6 files");
        let one = paths(&["caffeine.mol"]);
        assert_eq!(plan(&one).label(), "Drop to insert · caffeine.mol");
        let pictures = paths(&["a.png", "b.JPG", "c.jpeg", "d.tif", "e.tiff", "f.webp"]);
        assert_eq!(plan(&pictures), Plan::Insert(pictures.clone()));
        for native in ["x.rsk", "x.reshiki", "x.moruno"] {
            let path = PathBuf::from(native);
            assert_eq!(plan(std::slice::from_ref(&path)), Plan::Open(path));
        }
        assert_eq!(plan(&paths(&["x.rsk"])).label(), "Drop to open · x.rsk");
        for (names, reason) in [
            (&["notes.docx"][..], "Can't import .docx"),
            (&["a.mol", "notes.docx"][..], "Can't import .docx"),
            (&["README"][..], "Can't import this file"),
            (&["a.rsk", "b.rsk"][..], "Open one drawing at a time"),
            (&["a.rsk", "b.mol"][..], "Open one drawing at a time"),
        ] {
            assert_eq!(plan(&paths(names)).label(), reason);
        }
    }

    #[test]
    fn typed_text_reports_its_detected_format() {
        let mut app = ready();
        assert!(app.imports.is_blank());
        let paste = |text: &str| {
            Message::Imports(Action::Edit(text_editor::Action::Edit(
                text_editor::Edit::Paste(text.to_owned().into()),
            )))
        };
        let _ = app.update(paste("InChI=1S/CH4/h1H4"));
        assert_eq!(app.imports.format.map(format_name), Some("InChI"));
        let _ = app.update(Message::Imports(Action::Edit(
            text_editor::Action::SelectAll,
        )));
        let _ = app.update(paste("  "));
        assert!(app.imports.is_blank());
        app.imports.set_text("CCO>>CC=O");
        assert_eq!(app.imports.format.map(format_name), Some("Reaction SMILES"));
    }

    #[test]
    fn the_import_tab_focuses_its_box_and_its_menu_closes_like_other_menus() {
        use iced::keyboard::{Key, Modifiers};
        let mut app = ready();
        app.inspector_open = false;
        app.help_open = true;
        let command = Key::Character("i".into());
        let Some(show) =
            super::super::shortcuts::key_message(&command, &command, Modifiers::COMMAND)
        else {
            panic!("Cmd/Ctrl+I opens Import");
        };
        assert!(matches!(show, Message::Inspector(InspectorTab::Import)));
        assert!(app.update(show).units() > 0, "The text box takes focus");
        assert!(app.inspector_open && !app.help_open);
        assert_eq!(app.inspector_tab, InspectorTab::Import);
        let tool = app.tool;
        let _ = app.update(Message::Imports(Action::Menu(true)));
        let _ = app.update(Message::Escape);
        assert!(!app.imports.menu);
        assert_eq!(app.tool, tool, "Escape only closes the menu");
        let _ = app.update(Message::Imports(Action::Menu(true)));
        let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::default()))));
        assert!(app.imports.menu, "Passive events keep it open");
        let _ = app.update(Message::Imports(Action::Edit(
            text_editor::Action::SelectAll,
        )));
        assert!(!app.imports.menu);
    }

    #[test]
    fn hovered_files_drop_together_and_unsupported_files_change_nothing() {
        let mut app = ready();
        for name in ["a.mol", "b.png", "a.mol"] {
            let _ = app.update(Message::Imports(Action::Hovered(name.into())));
        }
        assert_eq!(app.imports.hovered, paths(&["a.mol", "b.png"]));
        let _ = app.update(Message::Imports(Action::Dropped("a.mol".into())));
        assert!(!app.busy, "Waits for the second file");
        assert_eq!(app.imports.hovered.len(), 2);
        assert!(
            app.update(Message::Imports(Action::Dropped("b.png".into())))
                .units()
                > 0
        );
        assert!(app.busy && app.imports.hovered.is_empty() && app.imports.dropped.is_empty());
        assert_eq!(app.status, "Importing…");

        let mut app = ready();
        app.doc.add_atom("C", Point::default());
        let before = app.doc.clone();
        let _ = app.update(Message::Imports(Action::Hovered("notes.docx".into())));
        let _ = app.update(Message::Imports(Action::Left));
        assert!(app.imports.hovered.is_empty());
        let _ = app.update(Message::Imports(Action::Hovered("notes.docx".into())));
        let _ = app.update(Message::Imports(Action::Dropped("notes.docx".into())));
        assert_eq!(app.doc, before);
        assert!(app.error && !app.busy);
        assert_eq!(app.status, "Can't import .docx");
    }

    #[test]
    fn a_dropped_drawing_opens_after_the_save_dialog() {
        let path = PathBuf::from("Esterification.rsk");
        let mut app = ready();
        assert!(
            app.update(Message::Imports(Action::Files(vec![path.clone()])))
                .units()
                > 0
        );
        assert!(app.pending.is_none(), "A saved drawing opens at once");
        app.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Imports(Action::Files(vec![path.clone()])));
        assert!(matches!(&app.pending, Some(Pending::Open(Some(p))) if *p == path));
    }

    #[tokio::test]
    async fn files_insert_side_by_side_at_the_pointer_as_one_undo_step() -> Result<(), String> {
        let folder = tempfile::tempdir().map_err(|e| e.to_string())?;
        let png = folder.path().join("blot.png");
        image::save_buffer(&png, &[0; 4 * 30 * 20], 30, 20, image::ColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let files = vec![
            fixtures.join("ethanol.mol"),
            fixtures.join("native-ethyl-clipboard.cdx"),
            png,
        ];
        let batch = load(LocalEngine::default(), files).await?;
        assert_eq!(batch.label, "3 files");
        assert_eq!(batch.drawings.len(), 3);
        assert_eq!(batch.drawings[0].atoms.len(), 3);
        assert!(!batch.drawings[1].atoms.is_empty(), "Binary CDX");
        assert_eq!(batch.drawings[2].graphics.len(), 1);

        let mut app = ready();
        app.viewport = iced::Size::new(900., 600.);
        app.doc.add_atom("N", Point::new(-300., 0.));
        let at = Point::new(40., 25.);
        let _ = app.update(Message::Canvas(Edit::Hover(Some(at))));
        let before = app.doc.clone();
        let ticket = Ticket {
            epoch: app.file_epoch,
            revision: app.revision,
        };
        app.busy = true;
        let loaded = |ticket, batch: &Batch| {
            Message::Imports(Action::Loaded(ticket, Box::new(Ok(batch.clone()))))
        };
        let _ = app.update(loaded(ticket, &batch));
        assert!(!app.busy && !app.error, "{}", app.status);
        let after = app.doc.clone();
        assert!(before.atoms.iter().all(|a| after.atom(a.id) == Some(a)));
        assert_eq!(after.graphics.len(), 1);
        let (lo, hi) = reshiki::scene::selection_bounds(&after, &app.selected).ok_or("bounds")?;
        assert!(((lo.x + hi.x) / 2. - at.x).abs() < 1. && ((lo.y + hi.y) / 2. - at.y).abs() < 1.);
        // Left to right in file order.
        let x = |ids: &[u64]| reshiki::scene::selection_bounds(&after, ids).map(|(lo, _)| lo.x);
        let mol: Vec<_> = app.selected.iter().copied().take(3).collect();
        let picture = [after.graphics[0].id];
        assert!(x(&mol) < x(&picture));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);

        // A newer drawing or file wins over a late result.
        for new_file in [false, true] {
            let ticket = Ticket {
                epoch: app.file_epoch,
                revision: app.revision,
            };
            if new_file {
                app.file_epoch += 1;
            } else {
                app.revision += 1;
            }
            let _ = app.update(loaded(ticket, &batch));
            assert_eq!(app.doc, after);
        }
        // A pointer that left the canvas, as it does to reach Finder or the
        // Import tab, leaves files at the view center.
        let mut app = ready();
        let _ = app.update(Message::Canvas(Edit::Hover(Some(at))));
        assert_eq!(app.drop_point(0., 0.), at);
        let _ = app.update(Message::Canvas(Edit::Hover(None)));
        assert_eq!(app.drop_point(0., 0.), app.camera.center);
        // Drawings dropped near an edge move inward rather than scroll the view.
        let edge = app.camera.center.x + app.viewport.width / 2. / app.camera.zoom;
        let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::new(
            edge - 1.,
            at.y,
        )))));
        let x = app.drop_point(200., 0.).x;
        assert!(x + 100. < edge && x > app.camera.center.x, "{x}");
        Ok(())
    }

    #[tokio::test]
    async fn open_reads_binary_cdx() -> Result<(), String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-ethyl-clipboard.cdx");
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let mut app = ready();
        assert!(
            app.update(Message::Opened(Some((path, Ok(bytes.clone())))))
                .units()
                > 0
        );
        assert!(app.busy, "{}", app.status);
        let response = LocalEngine::default()
            .request(Request::import("cdx", &contents("cdx", bytes)))
            .await?;
        assert!(!response.document.ok_or("drawing")?.atoms.is_empty());
        Ok(())
    }
}
