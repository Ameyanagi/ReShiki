//! Asynchronous picture replacement and selection-aware size controls.
use super::{App, InspectorTab, Message, Point, Tool};
use iced::{Element, Task};
use reshiki::{graphics::Graphic, pictures::Picture};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    serial: u64,
    epoch: u64,
    revision: u64,
    target: u64,
}

#[derive(Debug, Clone)]
pub enum Action {
    Replace,
    Loaded(Ticket, Result<Option<Picture>, String>),
    Width(String),
    Height(String),
    Lock(bool),
    Resize(bool),
    RestoreAspect,
}
pub struct State {
    next: u64,
    pub active: Option<u64>,
    width: String,
    height: String,
    locked: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            next: 0,
            active: None,
            width: String::new(),
            height: String::new(),
            locked: true,
        }
    }
}
async fn choose_picture() -> Result<Option<Picture>, String> {
    let mut extensions = vec!["png", "jpg", "jpeg", "tif", "tiff", "webp"];
    if cfg!(windows) {
        extensions.push("emf");
    }
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Replace the picture")
        .add_filter("Pictures", &extensions)
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path: PathBuf = file.path().to_path_buf();
    reshiki::metafile::open(&path).await.map(Some)
}
fn millimetres(world: f32) -> f32 {
    world * reshiki::style::DEFAULT.points_per_world() * 25.4 / 72.
}

impl App {
    fn selected_picture(&self) -> Option<&Graphic> {
        let mut pictures = self
            .tab
            .doc
            .graphics
            .iter()
            .filter(|g| g.picture.is_some() && self.tab.selected.contains(&g.id));
        let first = pictures.next()?;
        pictures.next().is_none().then_some(first)
    }
    pub(super) fn sync_pictures(&mut self) {
        let dimensions = self.selected_picture().map(|g| {
            (
                g.axis_x.distance(Point::default()),
                g.axis_y.distance(Point::default()),
            )
        });
        if let Some((width, height)) = dimensions {
            self.tab.pictures.width = format!("{:.2}", millimetres(width));
            self.tab.pictures.height = format!("{:.2}", millimetres(height));
        }
    }
    fn picture_loaded(&mut self, ticket: Ticket, result: Result<Option<Picture>, String>) {
        if self.tab.pictures.active != Some(ticket.serial) {
            return;
        }
        self.tab.pictures.active = None;
        if ticket.epoch != self.tab.file_epoch {
            return;
        }
        if ticket.revision != self.tab.revision
            || self.tab.inline_text.is_some()
            || self.tab.joining.is_some()
            || self.tab.cleanup.is_some()
        {
            self.status =
                "Drawing changed while loading the picture · Replace it again when ready".into();
            return;
        }
        let picture = match result {
            Ok(Some(picture)) => picture,
            Ok(None) => {
                self.status = "Picture replacement cancelled".into();
                return;
            }
            Err(error) => {
                self.error = true;
                self.status = format!("Could not load picture: {error}");
                return;
            }
        };
        let before = self.tab.doc.clone();
        let Some(g) = self
            .tab
            .doc
            .graphics
            .iter_mut()
            .find(|g| g.id == ticket.target && g.picture.is_some())
        else {
            return;
        };
        let width = g.axis_x.distance(Point::default());
        let height = g.axis_y.distance(Point::default());
        let scale = (width / picture.width() as f32).min(height / picture.height() as f32);
        if let Err(error) = reshiki::pictures::resize(
            g,
            picture.width() as f32 * scale,
            picture.height() as f32 * scale,
        ) {
            self.error = true;
            self.status = error;
            return;
        }
        g.picture = Some(picture);
        if let Err(error) = self.tab.doc.validate() {
            self.tab.doc = before;
            self.error = true;
            self.status = format!("Could not replace picture: {error}");
            return;
        }
        self.tab.selected = vec![ticket.target];
        self.changed(before);
        self.tool = Tool::Select;
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Properties;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        self.status = "Picture replaced · Undo restores the original".into();
    }
    pub(super) fn reveal_picture(&mut self, id: u64) {
        if let Some(g) = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| g.id == id && g.picture.is_some())
        {
            let (lo, hi) = g.bounds();
            self.reveal_bounds(lo, hi);
        }
    }
    /// Centers the view on these bounds, zooming out if they do not fit.
    pub(super) fn reveal_bounds(&mut self, lo: Point, hi: Point) {
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        if paper.width > 100. && paper.height > 100. {
            self.tab.camera.zoom = self
                .tab
                .camera
                .zoom
                .min((paper.width - 80.) / (hi.x - lo.x))
                .min((paper.height - 80.) / (hi.y - lo.y))
                .max(0.001);
        }
        self.tab.camera.center = lo.offset((hi.x - lo.x) / 2., (hi.y - lo.y) / 2.);
        self.tab.fit_to_view = false;
        self.tab.pages.fit = None;
    }
    pub(super) fn picture_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Replace => {
                if self.tab.pictures.active.is_some() {
                    return Task::none();
                }
                let Some(target) = self.selected_picture().map(|g| g.id) else {
                    return Task::none();
                };
                self.tab.pictures.next = self.tab.pictures.next.wrapping_add(1);
                let ticket = Ticket {
                    serial: self.tab.pictures.next,
                    epoch: self.tab.file_epoch,
                    revision: self.tab.revision,
                    target,
                };
                self.tab.pictures.active = Some(ticket.serial);
                self.error = false;
                self.status = "Choose a picture…".into();
                return Task::perform(choose_picture(), move |result| {
                    Message::Pictures(Action::Loaded(ticket, result))
                });
            }
            Action::Loaded(ticket, result) => self.picture_loaded(ticket, result),
            Action::Width(value) => self.tab.pictures.width = value,
            Action::Height(value) => self.tab.pictures.height = value,
            Action::Lock(value) => self.tab.pictures.locked = value,
            Action::Resize(width_changed) => {
                let text = if width_changed {
                    &self.tab.pictures.width
                } else {
                    &self.tab.pictures.height
                };
                let size = text
                    .parse::<f32>()
                    .ok()
                    .filter(|v| (0.01..=10_000.).contains(v));
                let Some(size) = size else {
                    self.error = true;
                    self.status = "Enter a picture dimension from 0.01 to 10,000 mm".into();
                    return Task::none();
                };
                let Some(g) = self.selected_picture() else {
                    return Task::none();
                };
                let (width, height) = (
                    g.axis_x.distance(Point::default()),
                    g.axis_y.distance(Point::default()),
                );
                let size = reshiki::style::DEFAULT.world(size * 72. / 25.4);
                let dimensions = if width_changed {
                    (
                        size,
                        if self.tab.pictures.locked {
                            height * size / width
                        } else {
                            height
                        },
                    )
                } else {
                    (
                        if self.tab.pictures.locked {
                            width * size / height
                        } else {
                            width
                        },
                        size,
                    )
                };
                self.resize_picture(dimensions);
            }
            Action::RestoreAspect => {
                let Some(g) = self.selected_picture() else {
                    return Task::none();
                };
                let Some(picture) = &g.picture else {
                    return Task::none();
                };
                let width = g.axis_x.distance(Point::default());
                let height = g.axis_y.distance(Point::default());
                let scale = (width / picture.width() as f32).min(height / picture.height() as f32);
                self.resize_picture((
                    picture.width() as f32 * scale,
                    picture.height() as f32 * scale,
                ));
            }
        }
        Task::none()
    }
    fn resize_picture(&mut self, (width, height): (f32, f32)) {
        let Some(id) = self.selected_picture().map(|g| g.id) else {
            return;
        };
        let before = self.tab.doc.clone();
        if let Some(g) = self.tab.doc.graphics.iter_mut().find(|g| g.id == id)
            && let Err(error) = reshiki::pictures::resize(g, width, height)
        {
            self.error = true;
            self.status = error;
            return;
        }
        self.changed(before);
        self.sync_pictures();
    }
    pub(super) fn picture_panel(&self) -> Element<'_, Message> {
        use super::workspace::{command, muted_text, section};
        use iced::Alignment;
        use iced::widget::{checkbox, column, row, text};
        let message = Message::Pictures;
        let mut panel = column![section("PICTURE")].spacing(9);
        if let Some(g) = self.selected_picture() {
            if let Some(p) = &g.picture {
                if p.emf().is_some() {
                    panel = panel.push(text("EMF vector picture").size(12));
                    panel = panel.push(text("Windows EMF export keeps the original vectors. PNG, SVG, PDF and print use a preview.").size(11).style(muted_text));
                }
                panel = panel.push(
                    text(format!("{} × {} pixels", p.width(), p.height()))
                        .size(12)
                        .style(muted_text),
                );
            }
            panel = panel
                .push(
                    row![
                        text("Width").size(12).width(45),
                        crate::appearance::text_input("mm", &self.tab.pictures.width)
                            .on_input(move |v| message(Action::Width(v)))
                            .on_submit(message(Action::Resize(true)))
                            .padding(6)
                            .size(12),
                        text("mm").size(11)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .push(
                    row![
                        text("Height").size(12).width(45),
                        crate::appearance::text_input("mm", &self.tab.pictures.height)
                            .on_input(move |v| message(Action::Height(v)))
                            .on_submit(message(Action::Resize(false)))
                            .padding(6)
                            .size(12),
                        text("mm").size(11)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .push(
                    checkbox(self.tab.pictures.locked)
                        .label("Link width and height")
                        .on_toggle(move |v| message(Action::Lock(v)))
                        .size(14)
                        .text_size(12),
                )
                .push(command(
                    "Restore original proportions",
                    message(Action::RestoreAspect),
                ))
                .push(
                    command("Replace picture…", message(Action::Replace)).on_press_maybe(
                        self.tab
                            .pictures
                            .active
                            .is_none()
                            .then_some(message(Action::Replace)),
                    ),
                );
        } else {
            panel = panel.push(text("Multiple pictures selected").size(12));
        }
        panel.push(text("Drag the corner handles to resize; use the handle above to rotate. Enter applies dimensions. Pictures are saved inside your drawing.").size(11).style(muted_text)).into()
    }
}

#[cfg(test)]
mod tests;
