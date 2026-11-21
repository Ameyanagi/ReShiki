//! The empty drawing's import field, actions and recent files.
#[cfg(not(target_arch = "wasm32"))]
use super::workspace::hover_hint;
use super::workspace::{control, muted_text};
use super::{App, InspectorTab, Message};
use iced::widget::{column, container, row, scrollable, text};
#[cfg(not(target_arch = "wasm32"))]
use iced::widget::{image, tooltip};
use iced::{Alignment, Element, Length, Task};
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
mod recent;

pub(super) fn compact(value: String, size: f32, muted: bool) -> Element<'static, Message> {
    iced::widget::responsive(move |bounds| {
        let glyphs = iced::widget::text_input::Value::new(&value);
        let mut end = glyphs.len();
        let mut shown = value.clone();
        while end > 0 && super::workspace::text_width(&shown, size) > bounds.width {
            end -= 1;
            shown = glyphs.until(end).to_string() + "…";
        }
        let label = text(shown)
            .size(size)
            .height(28)
            .align_y(Alignment::Center)
            .wrapping(text::Wrapping::None);
        if muted {
            label.style(muted_text).into()
        } else {
            label.into()
        }
    })
    .width(Length::Fill)
    .height(28)
    .into()
}

#[derive(Debug, Clone)]
pub enum Action {
    Input(String),
    Insert,
    #[cfg(any(target_arch = "wasm32", test))]
    BrowserOpen(super::document_tab::TabId, u64),
    #[cfg(not(target_arch = "wasm32"))]
    Open(PathBuf),
    #[cfg(not(target_arch = "wasm32"))]
    Preview(PathBuf, u64, Option<image::Handle>),
}

#[derive(Default)]
pub(super) struct State {
    input: String,
    #[cfg(any(target_arch = "wasm32", test))]
    browser_recent: Vec<(super::document_tab::TabId, u64)>,
    #[cfg(not(target_arch = "wasm32"))]
    recent: recent::Recent,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) recent_suppressed: bool,
}

impl State {
    pub(super) fn load() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            recent: recent::Recent::load(),
            ..Self::default()
        }
    }
}

impl App {
    pub(super) fn start_visible(&self) -> bool {
        self.tab.doc.object_ids().next().is_none()
            && !self.tab.drawing_edit_active()
            && self.tab.mapping.review_pair.is_none()
    }

    pub(super) fn start_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Input(input) => {
                if self.start_visible() {
                    self.start.input = input.clone();
                    // Editing the field cancels its previous name request,
                    // exactly as editing the ordinary Import name field does.
                    let _ = self.naming_action(super::naming::Action::Name(input));
                }
            }
            #[cfg(any(target_arch = "wasm32", test))]
            Action::BrowserOpen(id, epoch) => {
                if self.pending.is_none()
                    && self
                        .strip()
                        .any(|tab| tab.id == id && tab.file_epoch == epoch)
                {
                    return self.tab_action(super::tabs::Action::Select(id));
                }
            }
            Action::Insert => {
                if !self.start_visible() || self.tab.busy || self.tab.naming.is_pending() {
                    return Task::none();
                }
                let input = self.start.input.trim().to_owned();
                if input.is_empty() {
                    return Task::none();
                }
                if input.len() > 8192 {
                    self.status = "Name or SMILES is too long · Use Import for larger text".into();
                    self.error = true;
                    return Task::none();
                }
                // SMILES wins when the input is valid as both a name and SMILES.
                // Use the same chemistry parser and insertion jobs as Import.
                if reshiki::chemistry::smiles::parse_for_import(&input).is_ok() {
                    self.imports.kind = super::import::InputKind::Structure;
                    self.imports.set_text(&input);
                    return self.insert_input();
                }
                self.imports.kind = super::import::InputKind::Name;
                let _ = self.naming_action(super::naming::Action::Name(input));
                return self.naming_action(super::naming::Action::InsertName);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::Open(path) => {
                if self.pending.is_none() && self.start.recent.contains(&path) {
                    return super::files::open_paths(vec![path]);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::Preview(path, generation, preview) => {
                self.start.recent.previewed(&path, generation, preview)
            }
        }
        Task::none()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn remember_start_file(&mut self, path: &std::path::Path) {
        if self.start.recent_suppressed
            || self.strip().any(|tab| {
                tab.office
                    .as_ref()
                    .is_some_and(|binding| binding.same_path(path))
            })
        {
            return;
        }
        self.start.recent.remember(path);
    }

    pub(super) fn start_previews(&mut self) -> Task<Message> {
        #[cfg(not(target_arch = "wasm32"))]
        return self.start.recent.previews(self.engine.clone());
        #[cfg(target_arch = "wasm32")]
        Task::none()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(super) fn remember_start_browser_tab(&mut self) {
        let key = (self.tab.id, self.tab.file_epoch);
        self.start.browser_recent.retain(|(id, _)| *id != key.0);
        self.start.browser_recent.insert(0, key);
        self.start.browser_recent.truncate(5);
    }

    pub(super) fn with_start<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let layers = iced::widget::stack![base];
        if !self.start_visible() {
            return layers.into();
        }
        let ready = !self.tab.busy && !self.tab.naming.is_pending();
        let insert = ready && !self.start.input.trim().is_empty();
        let field = reshiki::accessibility::text_input(
            "start-input",
            "Name or SMILES",
            "Name or SMILES",
            &self.start.input,
        )
        .on_input(|value| Message::Start(Action::Input(value)))
        .on_submit_maybe(insert.then_some(Message::Start(Action::Insert)))
        .padding([11, 12])
        .size(15)
        .style(crate::appearance::input_style);
        let action = |id, name: &'static str, key: &str, message| {
            reshiki::accessibility::button(
                id,
                name,
                row![
                    text(name).size(12),
                    (!key.is_empty()).then(|| text(key.to_owned()).size(10).style(muted_text))
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([7, 10])
            .style(crate::appearance::secondary)
            .on_press_maybe(ready.then_some(message))
        };
        let open_key = super::shortcuts::label(&Message::Open).unwrap_or_default();
        let paste_key = super::shortcuts::label(&Message::Paste).unwrap_or_default();
        let mut block = column![
            field,
            row![
                action("start-open", "Open…", &open_key, Message::Open),
                action("start-paste", "Paste", &paste_key, Message::Paste),
                action(
                    "start-templates",
                    "Templates",
                    "",
                    Message::Inspector(InspectorTab::Templates)
                ),
            ]
            .spacing(7)
            .wrap(),
        ]
        .spacing(12);
        if let Some(notice) = self.start_name_notice(&self.start.input) {
            block = block.push(
                column![
                    text(notice).size(11).style(muted_text),
                    reshiki::accessibility::button(
                        "start-import-details",
                        "Open Import",
                        "Open Import"
                    )
                    .style(iced::widget::button::text)
                    .on_press(Message::Inspector(InspectorTab::Import)),
                ]
                .spacing(4),
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.start.recent.entries.is_empty() {
            block = block.push(text("Recent").size(11).style(muted_text));
            for (index, entry) in self.start.recent.entries.iter().enumerate() {
                let name = super::import::name(&entry.path);
                let preview: Element<'_, Message> = match &entry.preview {
                    Some(handle) => image(handle.clone())
                        .width(40)
                        .height(28)
                        .content_fit(iced::ContentFit::Contain)
                        .into(),
                    None => container(
                        iced::widget::canvas(super::icons::Glyph(super::icons::Icon::Open, true))
                            .width(20)
                            .height(20),
                    )
                    .center_x(40)
                    .center_y(28)
                    .into(),
                };
                let place = self.start_place(&entry.path);
                let contents = row![preview, compact(name.clone(), 12., false), place]
                    .spacing(8)
                    .align_y(Alignment::Center);
                block = block.push(hover_hint(
                    reshiki::accessibility::button(
                        format!("start-recent-{index}"),
                        format!("Open {name}"),
                        contents,
                    )
                    .value(entry.path.display().to_string())
                    .width(Length::Fill)
                    .padding([4, 0])
                    .style(control(false))
                    .on_press_maybe(
                        ready.then(|| Message::Start(Action::Open(entry.path.clone()))),
                    ),
                    entry.path.display().to_string(),
                    tooltip::Position::Top,
                ));
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let recent: Vec<_> = self
                .start
                .browser_recent
                .iter()
                .filter_map(|(id, epoch)| {
                    self.strip()
                        .find(|tab| tab.id == *id && tab.file_epoch == *epoch)
                })
                .collect();
            if !recent.is_empty() {
                block = block.push(text("Recent · This session").size(11).style(muted_text));
            }
            for tab in recent.into_iter().take(5) {
                let name = tab.name();
                block = block.push(
                    reshiki::accessibility::button(
                        format!("start-recent-tab-{}-{}", tab.id.0, tab.file_epoch),
                        format!("Open {name}"),
                        row![
                            iced::widget::canvas(crate::canvas::DrawingPreview(&tab.doc))
                                .width(40)
                                .height(28),
                            compact(name, 12., false),
                            text("Browser").size(10).style(muted_text)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([7, 0])
                    .style(control(false))
                    .on_press_maybe(
                        ready
                            .then_some(Message::Start(Action::BrowserOpen(tab.id, tab.file_epoch))),
                    ),
                );
            }
        }
        layers
            .push(
                container(
                    container(
                        scrollable(container(block).padding(20).style(container::rounded_box))
                            .height(Length::Shrink),
                    )
                    .width(Length::Fill)
                    .max_width(400),
                )
                .center(Length::Fill)
                .padding(20),
            )
            .into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_place(&self, path: &std::path::Path) -> Element<'_, Message> {
        if let Some(place) = self.github_recent_place(path) {
            return container(place).width(90).into();
        }
        container(compact(
            path.parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            10.,
            true,
        ))
        .width(90)
        .into()
    }
}

#[cfg(test)]
mod tests;
