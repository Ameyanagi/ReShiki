//! The Icons tool: a searchable browser of line icons, the reaction
//! condition icons first and then the Tabler Icons outline library.
//! Choosing an icon arms it, and each click on the drawing places one copy.
use super::workspace::{command, control, hover_hint, muted_text, section};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use iced::widget::{Space, canvas, column, container, row, scrollable, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::{Document, Point};
use reshiki::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand};
use reshiki::line_icons::{GRID, Icon};
use reshiki::scientific::SymbolKind;
use std::sync::LazyLock;

/// The Tabler Icons release the library comes from; see licenses/tabler-icons.
pub const TABLER_VERSION: &str = "3.50.0";
/// The grid scrolls on its own, so the search stays in view.
const GRID_ID: &str = "icon-grid";
const COLUMNS: usize = 6;
const TILE: f32 = 40.;
const GAP: f32 = 6.;
const HEADING: f32 = 28.;
/// Rows drawn beyond the visible ones, so a quick scroll shows no gap.
const MARGIN: f32 = 400.;
const RECENT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Choice {
    Condition(SymbolKind),
    /// An icon of the library, by its index there.
    Library(usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Filter {
    #[default]
    All,
    Conditions,
    Category(usize),
}
impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All icons",
            Self::Conditions => CONDITIONS,
            Self::Category(i) => &library().categories[*i],
        })
    }
}
const CONDITIONS: &str = "Reaction conditions";

#[derive(Debug, Clone)]
pub enum Action {
    Search(String),
    Filter(Filter),
    /// Arm the icon; an icon already armed is placed in the middle of the view.
    Pick(Choice),
    /// Open the browser with the icon armed.
    Arm(Choice),
    Scrolled {
        offset: f32,
        height: f32,
    },
    Close,
}

#[derive(Default)]
pub struct State {
    pub query: String,
    pub filter: Filter,
    pub armed: Option<Choice>,
    /// Newest first.
    pub recent: Vec<Choice>,
    scroll: f32,
    height: f32,
}

pub struct Entry {
    /// Tabler's name, as "flask-2".
    pub name: String,
    category: Option<usize>,
    /// Name, category and tags in English and Japanese, lowercased.
    words: String,
    paths: String,
}
pub struct Library {
    pub entries: Vec<Entry>,
    categories: Vec<String>,
}

/// `assets/icons/tabler-outline.tsv`, deflated by `build.rs`.
const PACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tabler-outline.tsv.deflate"));
const JAPANESE: &str = include_str!("../../assets/icons/tabler-ja.tsv");
static LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let pack = miniz_oxide::inflate::decompress_to_vec(PACK).expect("the icon library inflates");
    let pack = String::from_utf8(pack).expect("the icon library is UTF-8");
    let rows = |text: &'static str| text.lines().filter(|l| !l.starts_with('#'));
    let japanese: std::collections::HashMap<_, _> = rows(JAPANESE)
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let lines: Vec<Vec<&str>> = pack
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect())
        .collect();
    let mut categories: Vec<String> = (lines.iter())
        .filter(|f| !f[1].is_empty())
        .map(|f| f[1].to_owned())
        .collect();
    categories.sort();
    categories.dedup();
    let entries = lines
        .iter()
        .map(|f| Entry {
            name: f[0].to_owned(),
            category: categories.iter().position(|c| c == f[1]),
            words: [
                f[0].replace('-', " ").as_str(),
                f[1],
                &f[2].replace(',', " "),
                &japanese.get(f[0]).unwrap_or(&"").replace(',', " "),
            ]
            .join(" ")
            .to_lowercase(),
            paths: f[3].to_owned(),
        })
        .collect();
    Library {
        entries,
        categories,
    }
});
pub fn library() -> &'static Library {
    &LIBRARY
}

/// English and Japanese words that find a condition icon.
pub(super) fn condition_words(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Light => "lamp bulb hv photochemistry irradiation 光 照射 光反応 ランプ 電球",
        SymbolKind::Electrode => "electrochemistry electrolysis 電極 電気化学 電解",
        SymbolKind::Heat => "flame reflux heating Δ 加熱 熱 還流 炎",
        SymbolKind::Microwave => "oven µw マイクロ波 電子レンジ",
        SymbolKind::Ultrasound => "sonication ultrasonic 超音波 ソニケーション",
        SymbolKind::Cold => "snowflake cooling ice 冷却 低温 氷 雪",
        SymbolKind::Enzyme => "biocatalysis 酵素 生体触媒",
        _ => "",
    }
}

impl Choice {
    /// "Flask 2", "Light".
    pub fn name(self) -> String {
        match self {
            Self::Condition(kind) => kind.to_string(),
            Self::Library(i) => {
                let name = library().entries[i].name.replace('-', " ");
                let mut chars = name.chars();
                chars
                    .next()
                    .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
            }
        }
    }
    fn words(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Condition(kind) => format!(
                "{} {} reaction condition",
                kind.to_string().to_lowercase(),
                condition_words(kind)
            )
            .into(),
            Self::Library(i) => library().entries[i].words.as_str().into(),
        }
    }
    fn icon(self) -> Option<Icon> {
        let Self::Library(i) = self else {
            return None;
        };
        let e = &library().entries[i];
        Icon::parse(&e.name, &e.paths).ok()
    }
    /// The graphic one click places at `at`: one bond length across, in the
    /// document's line width.
    fn graphic(
        self,
        id: u64,
        at: Point,
        doc: &Document,
        stroke: reshiki::palette::Color,
    ) -> Graphic {
        let size = doc.drawing_style.bond_length_world;
        let style = GraphicStyle {
            stroke,
            width_pt: doc.drawing_style.line_width_pt,
            ..GraphicStyle::default()
        };
        match (self, self.icon()) {
            (_, Some(icon)) => Graphic::line_icon(id, icon, at, size, style),
            (Self::Condition(kind), _) => {
                let mut g = Graphic::dragged(
                    id,
                    GraphicKind::Symbol(kind),
                    at,
                    at,
                    style,
                    BracketSides::Both,
                    false,
                );
                g.axis_x = Point::new(size, 0.);
                g.axis_y = Point::new(0., size);
                g
            }
            (Self::Library(_), None) => unreachable!("the bundled icons all read"),
        }
    }
}

/// Every word of `query` in the icon's words, best first: the name itself,
/// then names that start with or hold the first word, then tags.
fn ranked(query: &str, choices: Vec<Choice>) -> Vec<Choice> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let Some(first) = words.first() else {
        return choices;
    };
    let mut found: Vec<(u8, Choice)> = choices
        .into_iter()
        .filter_map(|c| {
            let all = c.words();
            words.iter().all(|w| all.contains(w)).then(|| {
                let name = c.name().to_lowercase();
                let rank = if name == *first {
                    0
                } else if name.starts_with(first) {
                    1
                } else if name.contains(first) {
                    2
                } else {
                    3
                };
                (rank, c)
            })
        })
        .collect();
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, c)| c).collect()
}

enum Row {
    Heading(String),
    Tiles(Vec<Choice>),
}
impl Row {
    fn height(&self) -> f32 {
        match self {
            Self::Heading(_) => HEADING,
            Self::Tiles(_) => TILE + GAP,
        }
    }
}

fn tiles(rows: &mut Vec<Row>, heading: Option<&str>, choices: Vec<Choice>) {
    if choices.is_empty() {
        return;
    }
    if let Some(h) = heading {
        rows.push(Row::Heading(h.to_owned()));
    }
    rows.extend(choices.chunks(COLUMNS).map(|c| Row::Tiles(c.to_vec())));
}

impl State {
    /// The icons the filter lets through, conditions first.
    fn pool(&self) -> Vec<Choice> {
        let lib = library();
        let conditions = SymbolKind::CONDITIONS.iter().map(|k| Choice::Condition(*k));
        let library = (0..lib.entries.len()).map(Choice::Library);
        match self.filter {
            Filter::All => conditions.chain(library).collect(),
            Filter::Conditions => conditions.collect(),
            Filter::Category(c) => library
                .filter(|l| matches!(l, Choice::Library(i) if lib.entries[*i].category == Some(c)))
                .collect(),
        }
    }

    /// What the grid shows: recent icons and the conditions first, then each
    /// category; a search lists its matches best first.
    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if !self.query.trim().is_empty() {
            tiles(&mut rows, None, ranked(&self.query, self.pool()));
            return rows;
        }
        if self.filter != Filter::All {
            tiles(&mut rows, None, self.pool());
            return rows;
        }
        let lib = library();
        let category = |c: Option<usize>| {
            (0..lib.entries.len())
                .filter(|i| lib.entries[*i].category == c)
                .map(Choice::Library)
                .collect()
        };
        tiles(&mut rows, Some("Recent"), self.recent.clone());
        tiles(
            &mut rows,
            Some(CONDITIONS),
            SymbolKind::CONDITIONS
                .iter()
                .map(|k| Choice::Condition(*k))
                .collect(),
        );
        for (c, name) in lib.categories.iter().enumerate() {
            tiles(&mut rows, Some(name), category(Some(c)));
        }
        tiles(&mut rows, Some("Other"), category(None));
        rows
    }
}

/// A tile's drawing: the icon on its 24-unit grid, scaled to the tile.
struct Tile(Choice);
impl<Message> canvas::Program<Message> for Tile {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: iced::Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut f = crate::canvas::layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        if let Choice::Condition(kind) = self.0 {
            super::icons::Glyph(
                super::icons::Icon::Tool(Tool::Graphic(GraphicKind::Symbol(kind))),
                true,
            )
            .paint(&mut f);
        } else if let Some(icon) = self.0.icon() {
            paint(
                &mut f,
                iced::Color::from_rgb8(51, 62, 72),
                &icon,
                bounds.width / GRID,
            );
        }
        f.finish()
    }
}

/// Tabler's "icons" icon, for the tool's button.
pub(super) const TOOL_GLYPH: &str = "M3 6.5a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0|M2.5 21h8l-4 -7l-4 7|M14 3l7 7|M14 10l7 -7|M14 14h7v7h-7l0 -7";

/// Draws `icon` in `ink`, `scale` pixels to a grid unit.
pub(super) fn paint(
    f: &mut crate::canvas::layered::Frame<'_>,
    ink: iced::Color,
    icon: &Icon,
    scale: f32,
) {
    use iced::widget::canvas::{LineCap, LineJoin, Path, Stroke};
    let at = |p: Point| iced::Point::new(p.x * scale, p.y * scale);
    for path in &icon.paths {
        let shape = Path::new(|b| {
            for c in &path.commands {
                match *c {
                    PathCommand::Move(p) => b.move_to(at(p)),
                    PathCommand::Line(p) => b.line_to(at(p)),
                    PathCommand::Cubic(a, z, p) => b.bezier_curve_to(at(a), at(z), at(p)),
                    PathCommand::Close => b.close(),
                }
            }
        });
        if path.filled {
            f.fill(&shape, ink);
        }
        f.stroke(
            &shape,
            Stroke::default()
                .with_color(ink)
                .with_width(1.5)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
    }
}

impl App {
    pub(super) fn icon_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Search(query) => {
                self.icons.query = query;
                return self.icon_grid_to_top();
            }
            Action::Filter(filter) => {
                self.icons.filter = filter;
                return self.icon_grid_to_top();
            }
            Action::Scrolled { offset, height } => {
                self.icons.scroll = offset;
                self.icons.height = height;
            }
            Action::Pick(choice)
                if self.icons.armed == Some(choice) && self.tool == Tool::Icons =>
            {
                let before = self.tab.doc.clone();
                self.place_icon(self.tab.camera.center, before);
                return super::focus_drawing();
            }
            Action::Pick(choice) | Action::Arm(choice) => {
                if self.tool != Tool::Icons {
                    self.select_tool(Tool::Icons);
                }
                self.icons.armed = Some(choice);
                self.status = format!("{} · Click to place · Esc closes", choice.name());
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

    fn icon_grid_to_top(&mut self) -> Task<Message> {
        self.icons.scroll = 0.;
        iced::widget::operation::snap_to(GRID_ID, iced::widget::operation::RelativeOffset::START)
    }

    /// One click places one icon, centred on the pointer, as one Undo step.
    pub(super) fn place_icon(&mut self, at: Point, before: Document) {
        let Some(choice) = self.icons.armed.filter(|_| self.tool == Tool::Icons) else {
            self.status = "Choose an icon in the Icons panel".into();
            self.error = false;
            return;
        };
        let id = self.tab.doc.next_id();
        let g = choice.graphic(id, at, &self.tab.doc, self.tab.graphic_style.stroke);
        self.tab.doc.graphics.push(g);
        self.tab.selected = vec![id];
        self.changed(before);
        if self.error {
            return;
        }
        let recent = &mut self.icons.recent;
        recent.retain(|c| *c != choice);
        recent.insert(0, choice);
        recent.truncate(RECENT);
        self.status = format!(
            "Placed {} · Click for another · Esc closes",
            choice.name().to_lowercase()
        );
    }

    /// The graphic a click would place, for the ghost under the pointer.
    pub(super) fn armed_icon(&self) -> Option<Graphic> {
        let choice = self.icons.armed.filter(|_| self.tool == Tool::Icons)?;
        Some(choice.graphic(
            0,
            Point::default(),
            &self.tab.doc,
            self.tab.graphic_style.stroke,
        ))
    }

    pub(super) fn icons_panel(&self) -> Element<'_, Message> {
        let state = &self.icons;
        let lib = library();
        let mut filters = vec![Filter::All, Filter::Conditions];
        filters.extend((0..lib.categories.len()).map(Filter::Category));
        let rows = state.rows();
        let count: usize = (rows.iter())
            .map(|r| match r {
                Row::Tiles(c) => c.len(),
                Row::Heading(_) => 0,
            })
            .sum::<usize>()
            - if state.query.trim().is_empty() && state.filter == Filter::All {
                state.recent.len()
            } else {
                0
            };
        // Only the rows near the view are built; spaces stand in for the rest.
        let (top, bottom) = (
            state.scroll - MARGIN,
            state.scroll + state.height.max(800.) + MARGIN,
        );
        let (mut above, mut below, mut y) = (0., 0., 0.);
        let mut shown = column![];
        for r in &rows {
            let h = r.height();
            if y + h < top {
                above += h;
            } else if y > bottom {
                below += h;
            } else {
                shown = shown.push(self.icon_row(r));
            }
            y += h;
        }
        let grid = scrollable(column![
            Space::new().height(above),
            shown,
            Space::new().height(below)
        ])
        .id(GRID_ID)
        .on_scroll(|v| {
            Message::Icons(Action::Scrolled {
                offset: v.absolute_offset().y,
                height: v.bounds().height,
            })
        })
        .height(Length::Fill);
        let hint = match state.armed.filter(|_| self.tool == Tool::Icons) {
            Some(c) => format!(
                "{}: click the drawing to place it, or the icon again for the middle of the view.",
                c.name()
            ),
            None => "Click an icon, then click the drawing.".into(),
        };
        let found = match count {
            0 => "No matching icons. Try another word or category.".to_owned(),
            1 => "1 icon".to_owned(),
            n => format!("{n} icons"),
        };
        column![
            column![
                row![
                    section("Icons").width(Length::Fill),
                    command("Done", Message::Icons(Action::Close)),
                ]
                .align_y(Alignment::Center),
                crate::appearance::text_input("Search icons · 検索…", &state.query)
                    .id("icon-search")
                    .on_input(|s| Message::Icons(Action::Search(s)))
                    .size(12)
                    .padding(8),
                crate::appearance::pick_list(filters, Some(state.filter), |f| {
                    Message::Icons(Action::Filter(f))
                })
                .text_size(12)
                .padding(6)
                .width(Length::Fill),
                text(hint).size(11),
                text(found).size(11).style(muted_text),
            ]
            .spacing(8)
            .padding([12, 16]),
            container(grid).padding(iced::Padding::ZERO.left(16).right(6)),
            container(
                text(format!("Tabler Icons {TABLER_VERSION} · MIT License"))
                    .size(10)
                    .style(muted_text)
            )
            .padding([6, 16]),
        ]
        .into()
    }

    fn icon_row<'a>(&self, r: &Row) -> Element<'a, Message> {
        match r {
            Row::Heading(h) => container(text(h.clone()).size(12))
                .height(HEADING)
                .align_y(Alignment::End)
                .padding(iced::Padding::ZERO.bottom(4))
                .into(),
            Row::Tiles(choices) => {
                let armed = |c: &Choice| self.tool == Tool::Icons && self.icons.armed == Some(*c);
                let mut line = row![].spacing(GAP);
                for c in choices {
                    let key = match c {
                        Choice::Condition(k) => format!("condition-{k:?}"),
                        Choice::Library(i) => library().entries[*i].name.clone(),
                    };
                    line = line.push(hover_hint(
                        reshiki::accessibility::button(
                            format!("icon-{key}"),
                            c.name(),
                            canvas(Tile(*c)).width(24).height(24),
                        )
                        .checked(armed(c))
                        .padding(8)
                        .width(TILE)
                        .height(TILE)
                        .style(control(armed(c)))
                        .on_press(Message::Icons(Action::Pick(*c))),
                        c.name(),
                        tooltip::Position::Bottom,
                    ));
                }
                container(line).height(TILE + GAP).into()
            }
        }
    }
}

#[cfg(test)]
mod tests;
