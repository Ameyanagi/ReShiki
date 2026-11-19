//! The Equipment browser: lab apparatus to place on the drawing. Choosing a
//! size arms it, and each click on the drawing places one copy.
use super::workspace::{command, control, horizontal_line, hover_hint, muted_text, section};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use crate::canvas::layered::canvas;
use iced::widget::{checkbox, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::{Document, Point};
use reshiki::equipment::{
    builtin,
    format::{Category, Definition},
    placed,
};
use std::sync::Arc;

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
    /// Arm the built-in piece with this ID.
    Arm(String),
    Repeat(bool),
    Close,
}

pub struct State {
    pub query: String,
    pub category: Filter,
    pub armed: Option<Arc<Definition>>,
    pub repeat: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: Filter::default(),
            armed: None,
            repeat: true,
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
fn thumbnail(d: &Arc<Definition>, theme: reshiki::canvas_theme::CanvasTheme) -> Document {
    let mut doc = Document {
        canvas_theme: theme,
        ..Document::default()
    };
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics
        .push(placed::graphic(1, d.clone(), Point::default(), scale));
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
            Action::Arm(id) => {
                let Some(definition) = builtin::find(&id) else {
                    return Task::none();
                };
                self.status = format!("{} · Click to place · Esc cancels", name(&definition));
                self.error = false;
                self.equipment.armed = Some(definition);
                self.tool = Tool::Equipment;
                return super::focus_drawing();
            }
            Action::Close => {
                self.select_tool(Tool::Select);
                self.inspector_tab = InspectorTab::Properties;
                return super::focus_drawing();
            }
        }
        Task::none()
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
        if !self.equipment.repeat {
            self.tool = Tool::Select;
        }
        self.changed(before);
    }

    pub(super) fn equipment_panel(&self) -> Element<'_, Message> {
        let state = &self.equipment;
        let arm = |d: &Definition| Message::Equipment(Action::Arm(d.id.clone()));
        let armed = |d: &Definition| self.armed_piece().is_some_and(|a| a.id == d.id);
        let mut categories = vec![Filter(None)];
        for d in builtin::catalogue() {
            if !categories.contains(&Filter(Some(d.category))) {
                categories.push(Filter(Some(d.category)));
            }
        }
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
            crate::appearance::pick_list(categories, Some(state.category), |f| {
                Message::Equipment(Action::Category(f))
            })
            .text_size(12)
            .padding(6)
            .width(Length::Fill),
        ]
        .spacing(8);
        if let Some(d) = self.armed_piece() {
            body = body.push(super::inspector::card(
                container(
                    column![
                        text(name(d)).size(14),
                        checkbox(state.repeat)
                            .label("Keep placing")
                            .text_size(12)
                            .size(14)
                            .on_toggle(|v| Message::Equipment(Action::Repeat(v))),
                        text(if state.repeat {
                            "Click again to add another copy. Esc finishes placement."
                        } else {
                            "Returns to Select after one placement."
                        })
                        .size(11)
                        .style(muted_text),
                    ]
                    .spacing(6),
                )
                .padding(12),
            ));
        }
        // Each family once, with its sizes as chips.
        let mut families: Vec<(&'static Arc<Definition>, Vec<&'static Arc<Definition>>)> =
            Vec::new();
        for d in builtin::catalogue().iter().filter(|d| {
            state.category.0.is_none_or(|c| c == d.category) && matches(d, &state.query)
        }) {
            match families
                .iter_mut()
                .find(|(first, _)| first.family == d.family)
            {
                Some((_, sizes)) => sizes.push(d),
                None => families.push((d, vec![d])),
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
        for &(first, ref sizes) in &families {
            let shown = sizes.iter().copied().find(|d| armed(d)).unwrap_or(first);
            let preview: Element<'_, Message> =
                canvas(crate::canvas::DrawingThumbnail(thumbnail(shown, theme)))
                    .width(44)
                    .height(44)
                    .into();
            let tile = hover_hint(
                reshiki::accessibility::button(
                    format!("equipment-{}", shown.id),
                    name(shown).to_owned(),
                    preview,
                )
                .checked(armed(shown))
                .padding(4)
                .width(52)
                .height(52)
                .style(control(armed(shown)))
                .on_press(arm(shown)),
                name(shown),
                tooltip::Position::Left,
            );
            let mut chips = row![].spacing(4);
            for &d in sizes {
                chips = chips.push(
                    reshiki::accessibility::button(
                        format!("equipment-size-{}", d.id),
                        format!("{} · {}", title(d), size(d)),
                        text(size(d)).size(11),
                    )
                    .checked(armed(d))
                    .padding([3, 7])
                    .style(control(armed(d)))
                    .on_press(arm(d)),
                );
            }
            body = body.push(
                row![
                    tile,
                    column![
                        text(title(shown)).size(12),
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
                text("No matching equipment. Try another search or category.")
                    .size(12)
                    .style(muted_text),
            );
        }
        body.into()
    }

    /// The selected pieces' name and size. Pieces are always illustration.
    pub(super) fn piece_card(&self) -> Option<Element<'_, Message>> {
        let pieces: Vec<_> = (self.tab.doc.graphics.iter())
            .filter(|g| self.tab.selected.contains(&g.id))
            .filter_map(|g| g.piece.as_ref()?.linked.as_ref())
            .collect();
        let (heading, detail) = match pieces.as_slice() {
            [] => return None,
            [d] => (title(d).to_owned(), size(d)),
            many => (format!("{} equipment pieces", many.len()), String::new()),
        };
        Some(super::inspector::card(
            container(
                column![
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
                .spacing(6),
            )
            .padding(12),
        ))
    }
}

#[cfg(test)]
mod tests;
