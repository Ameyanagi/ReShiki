use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message};
use iced::widget::{button, column, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    document::Point,
    pages::{Layout, Preset},
};

fn print() -> Message {
    Message::Printing(super::printing::Action::Start(
        reshiki::printing::Scope::Document,
    ))
}

fn command(label: &'static str) -> iced::widget::Button<'static, Message> {
    iced::widget::button(text(label).size(12))
        .padding([8, 10])
        .style(super::workspace::control(false))
}

#[derive(Debug, Clone, Copy)]
pub enum Field {
    Width,
    Height,
    Top,
    Right,
    Bottom,
    Left,
    Columns,
    Rows,
}
#[derive(Debug, Clone)]
pub enum Action {
    Show,
    Open,
    Cancel,
    Apply,
    Remove,
    Preset(Preset),
    Landscape(bool),
    Input(Field, String),
    Fit(Option<usize>),
    Navigate(usize),
    Center(bool),
    Export,
}
#[derive(Default)]
pub struct State {
    pub editor: Option<Editor>,
    pub active: usize,
    pub fit: Option<Option<usize>>,
}
pub struct Editor {
    original: Option<Layout>,
    layout: Layout,
    epoch: u64,
    width: String,
    height: String,
    top: String,
    right: String,
    bottom: String,
    left: String,
    columns: String,
    rows: String,
}
impl Editor {
    pub(super) fn new(doc: &reshiki::document::Document, epoch: u64) -> Self {
        let layout = doc
            .page_layout
            .clone()
            .unwrap_or_else(|| Layout::around(doc));
        let mut editor = Self {
            original: doc.page_layout.clone(),
            layout,
            epoch,
            width: String::new(),
            height: String::new(),
            top: String::new(),
            right: String::new(),
            bottom: String::new(),
            left: String::new(),
            columns: String::new(),
            rows: String::new(),
        };
        editor.reset_inputs();
        editor
    }
    fn reset_inputs(&mut self) {
        let mm = |pt: f32| format!("{:.2}", pt * 25.4 / 72.);
        self.width = mm(self.layout.width_pt);
        self.height = mm(self.layout.height_pt);
        self.top = mm(self.layout.margins.top);
        self.right = mm(self.layout.margins.right);
        self.bottom = mm(self.layout.margins.bottom);
        self.left = mm(self.layout.margins.left);
        self.columns = self.layout.columns.to_string();
        self.rows = self.layout.rows.to_string();
    }
    fn candidate(&self) -> Result<Layout, String> {
        let number = |s: &str| {
            s.trim()
                .parse::<f32>()
                .map(|n| n * 72. / 25.4)
                .map_err(|_| "Enter paper dimensions and margins in millimetres.".to_string())
        };
        let mut layout = self.layout.clone();
        // Keep standard paper dimensions exact despite rounded display values.
        let dimension = |s: &str, old: f32| -> Result<f32, String> {
            if s == format!("{:.2}", old * 25.4 / 72.) {
                Ok(old)
            } else {
                number(s)
            }
        };
        layout.width_pt = dimension(&self.width, layout.width_pt)?;
        layout.height_pt = dimension(&self.height, layout.height_pt)?;
        layout.margins.top = dimension(&self.top, layout.margins.top)?;
        layout.margins.right = dimension(&self.right, layout.margins.right)?;
        layout.margins.bottom = dimension(&self.bottom, layout.margins.bottom)?;
        layout.margins.left = dimension(&self.left, layout.margins.left)?;
        layout.columns = self
            .columns
            .trim()
            .parse()
            .map_err(|_| "Enter a whole number of page columns.".to_string())?;
        layout.rows = self
            .rows
            .trim()
            .parse()
            .map_err(|_| "Enter a whole number of page rows.".to_string())?;
        layout.validate()?;
        Ok(layout)
    }
}
impl App {
    pub(super) fn print_document(&self) -> Result<reshiki::document::Document, String> {
        let mut snapshot = self.tab.doc.clone();
        if let Some(editor) = &self.tab.pages.editor {
            if editor.epoch != self.tab.file_epoch || editor.original != self.tab.doc.page_layout {
                return Err("Reopen Page setup before printing changed page settings.".into());
            }
            snapshot.page_layout = Some(editor.candidate()?);
        }
        Ok(snapshot)
    }
    pub(super) fn fit_pages(&mut self, index: Option<usize>) {
        let Some(layout) = &self.tab.doc.page_layout else {
            return;
        };
        let region = match index {
            Some(i) => layout.bounds(i),
            None => layout.spread_bounds(),
        };
        let Some((lo, hi)) = region else {
            return;
        };
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        self.tab.camera.center = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
        self.tab.camera.zoom = ((paper.width - 50.).max(100.) / (hi.x - lo.x))
            .min((paper.height - 50.).max(100.) / (hi.y - lo.y))
            .clamp(0.005, 5.);
        self.tab.fit_to_view = false;
        self.tab.pages.fit = Some(index);
        if let Some(i) = index {
            self.tab.pages.active = i;
        }
    }
    pub(super) fn page_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show => {
                if self.tab.doc.page_layout.is_none() {
                    return self.page_action(Action::Open);
                }
                self.cancel_join();
                if self.tab.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.tab.pages.editor = None;
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Pages;
                self.palette = None;
            }
            Action::Open => {
                self.cancel_join();
                if self.tab.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.tab.pages.editor = Some(Editor::new(&self.tab.doc, self.tab.file_epoch));
                self.tab.pages.active = self.tab.pages.active.min(
                    self.tab
                        .doc
                        .page_layout
                        .as_ref()
                        .map(|l| l.count().saturating_sub(1))
                        .unwrap_or(0),
                );
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Pages;
                self.palette = None;
            }
            Action::Cancel => {
                self.tab.pages.editor = None;
                self.inspector_tab = InspectorTab::Export;
            }
            Action::Input(field, value) => {
                if let Some(e) = &mut self.tab.pages.editor {
                    *match field {
                        Field::Width => &mut e.width,
                        Field::Height => &mut e.height,
                        Field::Top => &mut e.top,
                        Field::Right => &mut e.right,
                        Field::Bottom => &mut e.bottom,
                        Field::Left => &mut e.left,
                        Field::Columns => &mut e.columns,
                        Field::Rows => &mut e.rows,
                    } = value;
                }
            }
            Action::Preset(preset) => {
                if let Some(e) = &mut self.tab.pages.editor
                    && let Some((w, h)) = preset.size()
                {
                    if let Ok(candidate) = e.candidate() {
                        e.layout = candidate;
                    }
                    let landscape = e.layout.width_pt > e.layout.height_pt;
                    e.layout.width_pt = if landscape { h } else { w };
                    e.layout.height_pt = if landscape { w } else { h };
                    e.reset_inputs();
                }
            }
            Action::Landscape(landscape) => {
                if let Some(e) = &mut self.tab.pages.editor {
                    if let Ok(candidate) = e.candidate() {
                        e.layout = candidate;
                    }
                    if landscape != (e.layout.width_pt > e.layout.height_pt) {
                        std::mem::swap(&mut e.layout.width_pt, &mut e.layout.height_pt);
                    }
                    e.reset_inputs();
                }
            }
            Action::Apply => {
                let Some(editor) = &self.tab.pages.editor else {
                    return Task::none();
                };
                if editor.epoch != self.tab.file_epoch
                    || editor.original != self.tab.doc.page_layout
                {
                    self.error = true;
                    self.status = "Page settings changed. Reopen Page setup to continue.".into();
                    return Task::none();
                }
                match editor.candidate() {
                    Ok(layout) => {
                        let before = self.tab.doc.clone();
                        self.tab.doc.page_layout = Some(layout);
                        self.tab.doc.version = self.tab.doc.version.max(15);
                        self.changed(before);
                        self.tab.pages.editor = None;
                        self.tab.pages.active = self.tab.pages.active.min(
                            self.tab
                                .doc
                                .page_layout
                                .as_ref()
                                .map(|l| l.count().saturating_sub(1))
                                .unwrap_or(0),
                        );
                        self.fit_pages(Some(self.tab.pages.active));
                        self.status =
                            "Page setup applied · Drawing scale and positions preserved".into();
                    }
                    Err(error) => {
                        self.error = true;
                        self.status = error;
                    }
                }
            }
            Action::Remove => {
                let before = self.tab.doc.clone();
                self.tab.doc.page_layout = None;
                self.changed(before);
                self.tab.pages = State::default();
                self.inspector_tab = InspectorTab::Export;
                self.fit();
                self.status =
                    "Returned to an unbounded canvas · Undo restores the page layout".into();
            }
            Action::Fit(index) => self.fit_pages(index),
            Action::Navigate(index) => self.fit_pages(Some(index)),
            Action::Center(selection) => {
                let Some(layout) = self.tab.doc.page_layout.clone() else {
                    return Task::none();
                };
                let ids = if selection {
                    self.tab.selected.clone()
                } else {
                    self.tab.doc.all_ids()
                };
                let before = self.tab.doc.clone();
                match layout.center(&mut self.tab.doc, &ids, self.tab.pages.active) {
                    Ok(()) => {
                        self.changed(before);
                        self.fit_pages(Some(self.tab.pages.active));
                        self.status =
                            "Centered on page · Drawing size preserved · Undo is available".into();
                    }
                    Err(error) => {
                        self.error = true;
                        self.status = error;
                    }
                }
            }
            Action::Export => {
                return self.export_figure("pdf", true);
            }
        }
        Task::none()
    }
    fn pages_overview(&self) -> Element<'_, Message> {
        let mut body = column![text("Publication pages").size(17)].spacing(14);
        if let Some(layout) = &self.tab.doc.page_layout {
            let active = self.tab.pages.active.min(layout.count().saturating_sub(1));
            body = body
                .push(
                    text(format!(
                        "{} · {:.1} × {:.1} mm",
                        layout.preset(),
                        layout.width_pt * 25.4 / 72.,
                        layout.height_pt * 25.4 / 72.
                    ))
                    .size(12),
                )
                .push(
                    row![
                        command("‹").on_press_maybe(
                            active
                                .checked_sub(1)
                                .map(|i| Message::Pages(Action::Navigate(i)))
                        ),
                        text(format!("Page {} of {}", active + 1, layout.count())).size(13),
                        command("›").on_press_maybe(
                            (active + 1 < layout.count())
                                .then_some(Message::Pages(Action::Navigate(active + 1)))
                        )
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                )
                .push(
                    row![
                        command("Fit page").on_press(Message::Pages(Action::Fit(Some(active)))),
                        command("Fit all pages").on_press(Message::Pages(Action::Fit(None)))
                    ]
                    .spacing(6),
                )
                .push(
                    command("Edit page setup…")
                        .on_press(Message::Pages(Action::Open))
                        .style(button::text),
                )
                .push(horizontal_line())
                .push(
                    command(if self.tab.selected.is_empty() {
                        "Center drawing on page"
                    } else {
                        "Center selection on page"
                    })
                    .on_press(Message::Pages(Action::Center(
                        !self.tab.selected.is_empty(),
                    )))
                    .style(button::text),
                )
                .push(
                    text(
                        "Centering keeps whole molecules together and preserves the drawing scale.",
                    )
                    .size(11)
                    .style(super::workspace::muted_text),
                )
                .push(
                    command("Export pages as PDF…")
                        .style(crate::appearance::primary)
                        .on_press_maybe(
                            (!self.figure_exporting).then_some(Message::Pages(Action::Export)),
                        )
                        .width(Length::Fill),
                );
            if reshiki::printing::available() {
                body = body.push(
                    super::workspace::keyed_command("Print…", print())
                        .padding([8, 10])
                        .on_press_maybe(self.printing.active.is_none().then(print))
                        .width(Length::Fill),
                );
            }
            let overflow = layout.overflow(&self.tab.doc);
            if overflow > 0 {
                body = body.push(text(format!("{overflow} items cross a page edge or sit outside the pages. PDF pages clip at paper edges.")).size(11).style(crate::appearance::text_color(iced::Color::from_rgb8(168,91,36))));
            }
            body = body.push(horizontal_line())
                .push(text("Margins are guides and do not print. Use cropped drawing exports for figures that will be placed in another document.").size(11).style(super::workspace::muted_text))
                .push(command("Remove page layout").on_press(Message::Pages(Action::Remove)).style(button::text));
        } else {
            body = body.push(text("Choose a paper size and margins to prepare a publication or a multipage PDF.").size(12))
                .push(command("Set up pages…").on_press(Message::Pages(Action::Open)));
        }
        body.into()
    }
    pub(super) fn pages_panel(&self) -> Element<'_, Message> {
        let Some(editor) = self
            .tab
            .pages
            .editor
            .as_ref()
            .filter(|e| e.epoch == self.tab.file_epoch)
        else {
            return self.pages_overview();
        };
        let input = |label: &'static str, value: &str, field| {
            column![
                text(label).size(11).style(super::workspace::muted_text),
                crate::appearance::text_input("", value)
                    .on_input(move |v| Message::Pages(Action::Input(field, v)))
                    .on_submit(Message::Pages(Action::Apply))
                    .size(12)
                    .padding(7)
            ]
            .spacing(4)
        };
        let candidate = editor.candidate();
        let preset = candidate
            .as_ref()
            .map(|l| l.preset())
            .unwrap_or(Preset::Custom);
        let landscape = candidate.as_ref().unwrap_or(&editor.layout).width_pt
            > candidate.as_ref().unwrap_or(&editor.layout).height_pt;
        let mut body = column![
            row![
                text("Publication pages").size(17),
                command("×")
                    .style(button::text)
                    .on_press(Message::Pages(Action::Cancel))
            ]
            .spacing(5),
            text("Physical size · Document drawing scale")
                .size(11)
                .style(super::workspace::muted_text),
            text("Canvas theme").size(11),
            crate::appearance::pick_list(
                self.theme_choices().0,
                Some(self.theme_choices().1),
                |choice| Message::ThemeFile(super::theme_files::Action::Choose(choice)),
            )
            .width(Length::Fill)
            .text_size(12),
            text("Canvas brightness · copies have transparent backgrounds").size(11),
            crate::appearance::pick_list(
                reshiki::canvas_theme::CanvasTheme::ALL,
                Some(self.tab.doc.canvas_theme),
                Message::CanvasTheme
            )
            .width(Length::Fill)
            .text_size(12),
            crate::appearance::pick_list(Preset::ALL, Some(preset), |p| Message::Pages(
                Action::Preset(p)
            ))
            .width(Length::Fill)
            .text_size(12),
            row![
                command("Portrait")
                    .on_press(Message::Pages(Action::Landscape(false)))
                    .style(super::workspace::control(!landscape)),
                command("Landscape")
                    .on_press(Message::Pages(Action::Landscape(true)))
                    .style(super::workspace::control(landscape))
            ]
            .spacing(4),
            row![
                input("Width (mm)", &editor.width, Field::Width),
                input("Height (mm)", &editor.height, Field::Height)
            ]
            .spacing(8),
            horizontal_line(),
            text("Margins (mm)").size(12),
            row![
                input("Top", &editor.top, Field::Top),
                input("Bottom", &editor.bottom, Field::Bottom)
            ]
            .spacing(8),
            row![
                input("Left", &editor.left, Field::Left),
                input("Right", &editor.right, Field::Right)
            ]
            .spacing(8),
            horizontal_line(),
            text("Page grid · read left to right, then down").size(11),
            row![
                input("Columns", &editor.columns, Field::Columns),
                input("Rows", &editor.rows, Field::Rows)
            ]
            .spacing(8),
            command("Apply page setup")
                .style(crate::appearance::primary)
                .on_press_maybe(candidate.is_ok().then_some(Message::Pages(Action::Apply)))
                .width(Length::Fill),
        ]
        .spacing(12);
        if let Err(error) = candidate {
            body = body.push(text(error).size(11).style(crate::appearance::text_color(
                iced::Color::from_rgb8(168, 52, 47),
            )));
        }
        if let Some(layout) = &self.tab.doc.page_layout {
            let active = self.tab.pages.active.min(layout.count().saturating_sub(1));
            body = body
                .push(horizontal_line())
                .push(
                    row![
                        command("‹").on_press_maybe(
                            active
                                .checked_sub(1)
                                .map(|i| Message::Pages(Action::Navigate(i)))
                        ),
                        text(format!("Page {} / {}", active + 1, layout.count())).size(12),
                        command("›").on_press_maybe(
                            (active + 1 < layout.count())
                                .then_some(Message::Pages(Action::Navigate(active + 1)))
                        ),
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                )
                .push(
                    row![
                        command("Fit page").on_press(Message::Pages(Action::Fit(Some(active)))),
                        command("Fit all").on_press(Message::Pages(Action::Fit(None)))
                    ]
                    .spacing(5),
                )
                .push(
                    command(if self.tab.selected.is_empty() {
                        "Center drawing on page"
                    } else {
                        "Center selection on page"
                    })
                    .on_press(Message::Pages(Action::Center(
                        !self.tab.selected.is_empty(),
                    )))
                    .style(button::text),
                )
                .push(
                    command("Export pages as PDF…")
                        .style(crate::appearance::primary)
                        .on_press_maybe(
                            (!self.figure_exporting).then_some(Message::Pages(Action::Export)),
                        )
                        .width(Length::Fill),
                );
            if reshiki::printing::available() {
                body = body.push(
                    super::workspace::keyed_command("Print…", print())
                        .padding([8, 10])
                        .on_press_maybe(self.printing.active.is_none().then(print))
                        .width(Length::Fill),
                );
            }
            let overflow = layout.overflow(&self.tab.doc);
            if overflow > 0 {
                body=body.push(text(format!("{overflow} objects cross a page edge or sit outside the pages. PDF pages clip at paper edges.")).size(11).style(crate::appearance::text_color(iced::Color::from_rgb8(168,91,36))));
            }
            body = body.push(
                command("Remove page layout")
                    .on_press(Message::Pages(Action::Remove))
                    .style(button::text),
            );
        }
        body.push(text("Page setup does not move or resize objects. Margin guides do not appear in exports. Drawing exports remain cropped to the artwork.").size(11).style(super::workspace::muted_text)).into()
    }
}

#[cfg(test)]
mod tests;
