use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message};
use iced::widget::{button, column, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    document::Point,
    pages::{Layout, Preset},
};

/// The page width field, focused when Custom is chosen.
const WIDTH_INPUT: &str = "page-width";

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
    /// Width and height as typed, or the applied size while either does not
    /// read as millimetres.
    fn typed_size(&self) -> (f32, f32) {
        let typed = |s: &str, old: f32| {
            if s == format!("{:.2}", old * 25.4 / 72.) {
                return Some(old);
            }
            s.trim()
                .parse::<f32>()
                .ok()
                .filter(|mm| *mm > 0.)
                .map(|mm| mm * 72. / 25.4)
        };
        typed(&self.width, self.layout.width_pt)
            .zip(typed(&self.height, self.layout.height_pt))
            .unwrap_or((self.layout.width_pt, self.layout.height_pt))
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
    /// Applies the edited page setup as one Undo step and closes the editor.
    fn apply_pages(&mut self) -> Task<Message> {
        let Some(editor) = &self.tab.pages.editor else {
            return Task::none();
        };
        if editor.epoch != self.tab.file_epoch || editor.original != self.tab.doc.page_layout {
            self.error = true;
            self.status = "Page settings changed. Reopen Page setup to continue.".into();
            return Task::none();
        }
        match editor.candidate() {
            Ok(layout) => {
                let before = self.tab.doc.clone();
                self.tab.doc.page_layout = Some(layout);
                self.tab
                    .doc
                    .require_version(reshiki::document::EDITOR_BASELINE);
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
                self.status = "Page setup applied · Drawing scale and positions preserved".into();
            }
            Err(error) => {
                self.error = true;
                self.status = error;
            }
        }
        Task::none()
    }
    /// Applies a size or orientation choice at once as one Undo step. Only
    /// the size changes: margins and grid typed but not applied stay typed.
    /// Margins too wide for the new size fall back to the defaults.
    fn apply_size(
        &mut self,
        graphic: bool,
        change: impl FnOnce(&mut Layout, &reshiki::document::Document),
    ) -> Task<Message> {
        // An open caption edit is committed first, so the sheet frames it.
        if !self.finish_inline(true) {
            return Task::none();
        }
        let Some(editor) = &self.tab.pages.editor else {
            return Task::none();
        };
        if editor.epoch != self.tab.file_epoch || editor.original != self.tab.doc.page_layout {
            self.error = true;
            self.status = "Page settings changed. Reopen Page setup to continue.".into();
            return Task::none();
        }
        let mut layout = editor
            .original
            .clone()
            .unwrap_or_else(|| editor.layout.clone());
        let drafts = [
            editor.top.clone(),
            editor.right.clone(),
            editor.bottom.clone(),
            editor.left.clone(),
            editor.columns.clone(),
            editor.rows.clone(),
        ];
        change(&mut layout, &self.tab.doc);
        let mut status = "Page size applied · Drawing scale and positions preserved";
        if layout.validate().is_err() {
            layout.margins = Default::default();
            status = "Page size applied · Margins too wide, reset to defaults";
        }
        if let Err(error) = layout.validate() {
            self.error = true;
            self.status = error;
            return Task::none();
        }
        let before = self.tab.doc.clone();
        self.tab.doc.page_layout = Some(layout);
        self.tab
            .doc
            .require_version(reshiki::document::EDITOR_BASELINE);
        self.changed(before.clone());
        // Carry forward what was typed and not yet applied.
        let shown = Editor::new(&before, self.tab.file_epoch);
        let mut fresh = Editor::new(&self.tab.doc, self.tab.file_epoch);
        if !graphic {
            let shown = [
                &shown.top,
                &shown.right,
                &shown.bottom,
                &shown.left,
                &shown.columns,
                &shown.rows,
            ];
            let fields = [
                &mut fresh.top,
                &mut fresh.right,
                &mut fresh.bottom,
                &mut fresh.left,
                &mut fresh.columns,
                &mut fresh.rows,
            ];
            for ((draft, shown), field) in drafts.into_iter().zip(shown).zip(fields) {
                if draft != *shown {
                    *field = draft;
                }
            }
        }
        self.tab.pages.editor = Some(fresh);
        let last = self
            .tab
            .doc
            .page_layout
            .as_ref()
            .map_or(0, |l| l.count().saturating_sub(1));
        // A single-sheet graphic fits its one sheet.
        self.tab.pages.active = if graphic {
            0
        } else {
            self.tab.pages.active.min(last)
        };
        self.fit_pages(Some(self.tab.pages.active));
        self.status = status.into();
        Task::none()
    }
    pub(super) fn page_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show => {
                if self.tab.doc.page_layout.is_none() {
                    return self.page_action(Action::Open);
                }
                self.cancel_join();
                if !self.finish_inline(true) {
                    return Task::none();
                }
                self.tab.pages.editor = None;
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Pages;
                self.palette = None;
                // Page setup lives in the sidebar now; the View strip's Done
                // would be a second way to finish it.
                self.view_open = false;
            }
            Action::Open => {
                self.cancel_join();
                if !self.finish_inline(true) {
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
                self.view_open = false;
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
                let Some((w, h)) = preset.size() else {
                    self.status = "Type the width and height, then Apply page setup".into();
                    return iced::widget::operation::focus(WIDTH_INPUT);
                };
                let graphic = preset.graphic();
                return self.apply_size(graphic, move |layout, doc| {
                    if graphic {
                        // A TOC graphic is one wide sheet that fills its box:
                        // the page edge is the final crop, centered on the
                        // drawing.
                        layout.width_pt = w;
                        layout.height_pt = h;
                        layout.margins = reshiki::pages::Margins {
                            top: 0.,
                            right: 0.,
                            bottom: 0.,
                            left: 0.,
                        };
                        layout.columns = 1;
                        layout.rows = 1;
                        if let Some((lo, hi)) =
                            reshiki::scene::selection_bounds(doc, &doc.all_ids())
                        {
                            let style = &reshiki::style::DEFAULT;
                            layout.origin = Point::new(
                                (lo.x + hi.x - style.world(w)) / 2.,
                                (lo.y + hi.y - style.world(h)) / 2.,
                            );
                        }
                    } else if layout.width_pt > layout.height_pt {
                        (layout.width_pt, layout.height_pt) = (h, w);
                    } else {
                        (layout.width_pt, layout.height_pt) = (w, h);
                    }
                });
            }
            Action::Landscape(landscape) => {
                // Turns the size as typed, so a typed custom size is kept.
                let typed = self.tab.pages.editor.as_ref().map(Editor::typed_size);
                return self.apply_size(false, move |layout, _| {
                    if let Some((w, h)) = typed {
                        (layout.width_pt, layout.height_pt) = (w, h);
                    }
                    if landscape != (layout.width_pt > layout.height_pt) {
                        std::mem::swap(&mut layout.width_pt, &mut layout.height_pt);
                    }
                });
            }
            Action::Apply => return self.apply_pages(),
            Action::Remove => {
                let before = self.tab.doc.clone();
                self.tab.doc.page_layout = None;
                self.changed(before);
                self.tab.pages = State::default();
                self.inspector_tab = InspectorTab::Export;
                self.fit();
                self.status = "Page layout removed".into();
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
                        self.status = "Centered on page · Drawing size kept".into();
                    }
                    Err(error) => {
                        self.error = true;
                        self.status = error;
                    }
                }
            }
            Action::Export => {
                return self.export_figure("pdf", true, false);
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
                    command("Page setup…")
                        .on_press(Message::Pages(Action::Open))
                        .style(button::text),
                )
                .push(horizontal_line())
                .push(super::workspace::hover_hint(
                    command(if self.tab.selected.is_empty() {
                        "Center drawing on page"
                    } else {
                        "Center selection on page"
                    })
                    .on_press(Message::Pages(Action::Center(
                        !self.tab.selected.is_empty(),
                    )))
                    .style(button::text),
                    "Keeps whole molecules together and the drawing scale",
                    iced::widget::tooltip::Position::Top,
                ))
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
                body = body.push(overflow_warning(overflow));
            }
            body = body
                .push(horizontal_line())
                .push(super::workspace::hover_hint(
                    text("Margins do not print")
                        .size(11)
                        .style(super::workspace::muted_text),
                    "For a figure in another document, export the cropped drawing",
                    iced::widget::tooltip::Position::Top,
                ))
                .push(
                    command("Remove page layout")
                        .on_press(Message::Pages(Action::Remove))
                        .style(button::text),
                );
        } else {
            body = body.push(text("Choose a paper size and margins to prepare a publication or a multipage PDF.").size(12))
                .push(command("Page setup…").on_press(Message::Pages(Action::Open)));
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
            let mut input = crate::appearance::text_input("", value);
            if matches!(field, Field::Width) {
                input = input.id(WIDTH_INPUT);
            }
            column![
                text(label).size(11).style(super::workspace::muted_text),
                input
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
        // The typed size, or the applied one while a field holds a mistake.
        let (width, height) = editor.typed_size();
        let preset = match &candidate {
            Ok(_) => preset,
            Err(_) => Layout {
                width_pt: width,
                height_pt: height,
                ..editor.layout.clone()
            }
            .preset(),
        };
        let landscape = width > height;
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
            text("Size").size(11),
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
            // A typed mistake shows beside Apply, which it blocks.
            text(candidate.as_ref().err().cloned().unwrap_or_default())
                .size(11)
                .style(crate::appearance::text_color(iced::Color::from_rgb8(
                    168, 52, 47
                ))),
            horizontal_line(),
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
        ]
        .spacing(12);
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
                body = body.push(overflow_warning(overflow));
            }
            body = body.push(
                command("Remove page layout")
                    .on_press(Message::Pages(Action::Remove))
                    .style(button::text),
            );
        }
        body.push(super::workspace::hover_hint(
            text("Objects stay in place · Margins do not print")
                .size(11)
                .style(super::workspace::muted_text),
            "Drawing exports stay cropped to the artwork",
            iced::widget::tooltip::Position::Top,
        ))
        .into()
    }
}

/// Objects that cross a page edge or sit outside the pages.
fn overflow_warning<'a>(overflow: usize) -> Element<'a, Message> {
    super::workspace::hover_hint(
        text(match overflow {
            1 => "1 object is off the pages".into(),
            n => format!("{n} objects are off the pages"),
        })
        .size(11)
        .style(crate::appearance::text_color(iced::Color::from_rgb8(
            168, 91, 36,
        ))),
        "Crosses a page edge or sits outside the pages · PDF pages clip at paper edges",
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

#[cfg(test)]
mod tests;
