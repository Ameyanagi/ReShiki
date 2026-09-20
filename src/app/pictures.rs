//! Asynchronous picture insertion and selection-aware size controls.
use super::{App, InspectorTab, Message, Point, Tool};
use iced::{Element, Task};
use moruno::{graphics::Graphic, pictures::Picture};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    serial: u64,
    epoch: u64,
    revision: u64,
    replace: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum Action {
    Import,
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
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Insert a picture — PNG, JPEG, TIFF or WebP")
        .add_filter("Pictures", &["png", "jpg", "jpeg", "tif", "tiff", "webp"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path: PathBuf = file.path().to_path_buf();
    tokio::task::spawn_blocking(move || Picture::open(&path).map(Some))
        .await
        .map_err(|e| format!("Could not load picture: {e}"))?
}
fn millimetres(world: f32) -> f32 {
    world * moruno::style::DEFAULT.points_per_world() * 25.4 / 72.
}

impl App {
    fn selected_picture(&self) -> Option<&Graphic> {
        let mut pictures = self
            .doc
            .graphics
            .iter()
            .filter(|g| g.picture.is_some() && self.selected.contains(&g.id));
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
            self.pictures.width = format!("{:.2}", millimetres(width));
            self.pictures.height = format!("{:.2}", millimetres(height));
        }
    }
    fn picture_loaded(&mut self, ticket: Ticket, result: Result<Option<Picture>, String>) {
        if self.pictures.active != Some(ticket.serial) {
            return;
        }
        self.pictures.active = None;
        if ticket.epoch != self.file_epoch {
            return;
        }
        if ticket.revision != self.revision
            || self.inline_text.is_some()
            || self.joining.is_some()
            || self.cleanup.is_some()
        {
            self.status =
                "Drawing changed while loading the picture · Import again when ready".into();
            return;
        }
        let picture = match result {
            Ok(Some(picture)) => picture,
            Ok(None) => {
                self.status = "Picture import cancelled".into();
                return;
            }
            Err(error) => {
                self.error = true;
                self.status = format!("Could not import picture: {error}");
                return;
            }
        };
        let before = self.doc.clone();
        let id = if let Some(id) = ticket.replace {
            let Some(g) = self
                .doc
                .graphics
                .iter_mut()
                .find(|g| g.id == id && g.picture.is_some())
            else {
                return;
            };
            let width = g.axis_x.distance(Point::default());
            let height = g.axis_y.distance(Point::default());
            let scale = (width / picture.width() as f32).min(height / picture.height() as f32);
            if let Err(error) = moruno::pictures::resize(
                g,
                picture.width() as f32 * scale,
                picture.height() as f32 * scale,
            ) {
                self.error = true;
                self.status = error;
                return;
            }
            g.picture = Some(picture);
            id
        } else {
            let id = self.doc.next_id();
            self.doc
                .graphics
                .push(picture.graphic(id, self.camera.center));
            id
        };
        if let Err(error) = self.doc.validate() {
            self.doc = before;
            self.error = true;
            self.status = format!("Could not insert picture: {error}");
            return;
        }
        self.selected = vec![id];
        self.changed(before);
        self.tool = Tool::Select;
        self.import_open = false;
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Properties;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        self.status = if ticket.replace.is_some() {
            "Picture replaced · Undo restores the original"
        } else {
            "Picture inserted · Drag the corner handles to resize"
        }
        .into();
        if ticket.replace.is_none() {
            self.reveal_picture(id);
        }
    }
    pub(super) fn reveal_picture(&mut self, id: u64) {
        if let Some(g) = self
            .doc
            .graphics
            .iter()
            .find(|g| g.id == id && g.picture.is_some())
        {
            let (lo, hi) = g.bounds();
            let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
            if paper.width > 100. && paper.height > 100. {
                self.camera.zoom = self
                    .camera
                    .zoom
                    .min((paper.width - 80.) / (hi.x - lo.x))
                    .min((paper.height - 80.) / (hi.y - lo.y))
                    .max(0.001);
            }
            self.camera.center = lo.offset((hi.x - lo.x) / 2., (hi.y - lo.y) / 2.);
            self.fit_to_view = false;
            self.pages.fit = None;
        }
    }
    pub(super) fn picture_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Import | Action::Replace => {
                if self.pictures.active.is_some() {
                    return Task::none();
                }
                let replace = if matches!(action, Action::Replace) {
                    let Some(g) = self.selected_picture() else {
                        return Task::none();
                    };
                    Some(g.id)
                } else {
                    None
                };
                self.pictures.next = self.pictures.next.wrapping_add(1);
                let ticket = Ticket {
                    serial: self.pictures.next,
                    epoch: self.file_epoch,
                    revision: self.revision,
                    replace,
                };
                self.pictures.active = Some(ticket.serial);
                self.error = false;
                self.status = "Choose a picture…".into();
                return Task::perform(choose_picture(), move |result| {
                    Message::Pictures(Action::Loaded(ticket, result))
                });
            }
            Action::Loaded(ticket, result) => self.picture_loaded(ticket, result),
            Action::Width(value) => self.pictures.width = value,
            Action::Height(value) => self.pictures.height = value,
            Action::Lock(value) => self.pictures.locked = value,
            Action::Resize(width_changed) => {
                let text = if width_changed {
                    &self.pictures.width
                } else {
                    &self.pictures.height
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
                let size = moruno::style::DEFAULT.world(size * 72. / 25.4);
                let dimensions = if width_changed {
                    (
                        size,
                        if self.pictures.locked {
                            height * size / width
                        } else {
                            height
                        },
                    )
                } else {
                    (
                        if self.pictures.locked {
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
        let before = self.doc.clone();
        if let Some(g) = self.doc.graphics.iter_mut().find(|g| g.id == id)
            && let Err(error) = moruno::pictures::resize(g, width, height)
        {
            self.error = true;
            self.status = error;
            return;
        }
        self.changed(before);
        self.sync_pictures();
    }
    pub(super) fn picture_panel(&self) -> Element<'_, Message> {
        use super::workspace::{command, muted, section};
        use iced::Alignment;
        use iced::widget::{checkbox, column, row, text, text_input};
        let message = Message::Pictures;
        let mut panel = column![section("PICTURE")].spacing(9);
        if let Some(g) = self.selected_picture() {
            if let Some(p) = &g.picture {
                panel = panel.push(
                    text(format!("{} × {} pixels", p.width(), p.height()))
                        .size(12)
                        .color(muted()),
                );
            }
            panel = panel
                .push(
                    row![
                        text("Width").size(12).width(45),
                        text_input("mm", &self.pictures.width)
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
                        text_input("mm", &self.pictures.height)
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
                    checkbox(self.pictures.locked)
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
                        self.pictures
                            .active
                            .is_none()
                            .then_some(message(Action::Replace)),
                    ),
                );
        } else {
            panel = panel.push(text("Multiple pictures selected").size(12));
        }
        panel.push(row![command("Send to back", Message::GraphicLayer(false)), command("Bring to front", Message::GraphicLayer(true))].spacing(4))
            .push(text("Drag the corner handles to resize; use the handle above to rotate. Enter applies dimensions. Pictures are saved inside your drawing.").size(11).color(muted())).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moruno::document::Document;

    fn picture(width: u32, height: u32) -> Picture {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(width, height)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        Picture::import(&bytes.into_inner()).unwrap()
    }
    fn ready() -> App {
        let (mut app, _) = App::new();
        app.busy = false;
        app.doc = Document::default();
        app.doc.add_atom("O", Point::new(200., 0.));
        app
    }
    fn ticket(app: &mut App, replace: Option<u64>) -> Ticket {
        app.pictures.next += 1;
        app.pictures.active = Some(app.pictures.next);
        Ticket {
            serial: app.pictures.next,
            epoch: app.file_epoch,
            revision: app.revision,
            replace,
        }
    }
    fn finish(app: &mut App, ticket: Ticket, picture: Picture) {
        let _ = app.update(Message::Pictures(Action::Loaded(ticket, Ok(Some(picture)))));
    }
    #[test]
    fn asynchronous_import_is_one_undo_step_and_never_replaces_a_newer_drawing() {
        let mut app = ready();
        let before = app.doc.clone();
        let job = ticket(&mut app, None);
        finish(&mut app, job, picture(120, 80));
        let after = app.doc.clone();
        assert_eq!(after.atoms, before.atoms);
        assert_eq!(after.graphics.len(), 1);
        assert_eq!(app.selected, vec![2]);
        assert!(app.pictures.active.is_none());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
        for new_file in [false, true] {
            let job = ticket(&mut app, None);
            if new_file {
                app.file_epoch += 1;
            } else {
                app.revision += 1;
            }
            finish(&mut app, job, picture(10, 20));
            assert_eq!(app.doc, after);
            assert!(app.pictures.active.is_none());
        }
        let job = ticket(&mut app, None);
        let _ = app.update(Message::InlineText(
            super::super::inline_text::Action::Begin(None, Point::default()),
        ));
        finish(&mut app, job, picture(10, 20));
        assert!(app.inline_text.is_some());
        assert_eq!(app.doc, after);
    }
    #[test]
    fn replacement_retains_center_rotation_and_layer_and_undo_restores_the_pixels() {
        let mut app = ready();
        let job = ticket(&mut app, None);
        finish(&mut app, job, picture(120, 80));
        let _ = app.update(Message::Transform(moruno::editing::Transform::Rotate(37.)));
        let _ = app.update(Message::Transform(
            moruno::editing::Transform::FlipHorizontal,
        ));
        let before = app.doc.clone();
        let old = &before.graphics[0];
        let center = old.origin.offset(
            (old.axis_x.x + old.axis_y.x) / 2.,
            (old.axis_x.y + old.axis_y.y) / 2.,
        );
        let job = ticket(&mut app, Some(old.id));
        finish(&mut app, job, picture(60, 100));
        let g = &app.doc.graphics[0];
        assert_eq!((g.id, g.layer), (old.id, old.layer));
        assert!(
            center.distance(g.origin.offset(
                (g.axis_x.x + g.axis_y.x) / 2.,
                (g.axis_x.y + g.axis_y.y) / 2.
            )) < 0.001
        );
        assert!((g.axis_x.y.atan2(g.axis_x.x) - old.axis_x.y.atan2(old.axis_x.x)).abs() < 0.001);
        assert!(
            (g.axis_x.distance(Point::default()) / g.axis_y.distance(Point::default()) - 0.6).abs()
                < 0.001
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }
    #[test]
    fn numeric_sizes_keep_proportions_and_reject_invalid_input_without_edits() {
        let mut app = ready();
        let job = ticket(&mut app, None);
        finish(&mut app, job, picture(120, 80));
        let before = app.doc.clone();
        for invalid in ["NaN", "-1", "inf", "0", "wrong"] {
            let _ = app.update(Message::Pictures(Action::Width(invalid.into())));
            let _ = app.update(Message::Pictures(Action::Resize(true)));
            assert_eq!(app.doc, before);
        }
        let _ = app.update(Message::Pictures(Action::Width("60".into())));
        let _ = app.update(Message::Pictures(Action::Resize(true)));
        let g = &app.doc.graphics[0];
        assert!((millimetres(g.axis_x.distance(Point::default())) - 60.).abs() < 0.001);
        assert!((millimetres(g.axis_y.distance(Point::default())) - 40.).abs() < 0.001);
        let _ = app.update(Message::Pictures(Action::Lock(false)));
        let _ = app.update(Message::Pictures(Action::Height("70".into())));
        let _ = app.update(Message::Pictures(Action::Resize(false)));
        assert_eq!(app.pictures.width, "60.00");
        assert_eq!(app.pictures.height, "70.00");
        let _ = app.update(Message::Pictures(Action::RestoreAspect));
        assert_eq!(app.pictures.height, "40.00");
        let restored = app.doc.clone();
        app.apply_graphic_style(moruno::graphics::GraphicChange::Stroke([255, 0, 0]));
        assert_eq!(app.doc, restored);
        assert_eq!(app.doc.atoms, before.atoms);
    }
    #[test]
    fn cancellation_failure_and_old_job_completion_leave_the_drawing_untouched() {
        let mut app = ready();
        let before = app.doc.clone();
        for result in [Ok(None), Err("Broken image".into())] {
            let job = ticket(&mut app, None);
            let _ = app.update(Message::Pictures(Action::Loaded(job, result)));
            assert_eq!(app.doc, before);
            assert!(app.pictures.active.is_none());
        }
        let old = ticket(&mut app, None);
        let current = ticket(&mut app, None);
        finish(&mut app, old, picture(20, 20));
        assert_eq!(app.pictures.active, Some(current.serial));
        assert_eq!(app.doc, before);
    }

    #[test]
    fn canvas_handle_resizing_refreshes_the_picture_dimensions() {
        let mut app = ready();
        let job = ticket(&mut app, None);
        finish(&mut app, job, picture(600, 360));
        assert_eq!(app.pictures.width, "50.80");
        app.edit(crate::canvas::Edit::Transform {
            ids: app.selected.clone(),
            pivot: Point::default(),
            scale: 0.5,
            rotation: 32.,
        });
        assert_eq!(app.pictures.width, "25.40");
        assert_eq!(app.pictures.height, "15.24");
        let _ = app.update(Message::Undo);
        assert_eq!(app.pictures.width, "50.80");
    }
}
