//! Figure panels (Wrap in panel, titles, dividers) and lasting row, column
//! and grid layouts.
use super::workspace::{control, hover_hint, muted_text, panel_action, section};
use super::{App, Message};
use iced::widget::{button, checkbox, column, row, text, tooltip};
use iced::{Alignment, Element, Length};
use reshiki::{
    document::Point,
    graphics::{Divider, Graphic, GraphicChange, GraphicKind, GraphicStyle, Panel, TitlePlace},
    layouts::{Align, Flow},
    palette::{Color, Hue, Row},
};

#[derive(Debug, Clone)]
pub enum Action {
    Wrap,
    Title(usize, String),
    ApplyTitles,
    Place(TitlePlace),
    Rounded(bool),
    Border(bool),
    AddDivider(bool),
    DividerAt(usize, String),
    ApplyDivider(usize),
    RemoveDivider(usize),
    Fit,
    Arrange(Flow),
    Columns(String),
    ApplyColumns,
    Gutter(String),
    ApplyGutter,
    Align(Align),
}

impl App {
    pub(super) fn figure_action(&mut self, action: Action) {
        match action {
            Action::Wrap => self.wrap_in_panel(),
            Action::Title(i, value) => {
                let titles = &mut self.tab.panel_titles;
                if titles.len() <= i {
                    titles.resize(i + 1, String::new());
                }
                if let Some(title) = titles.get_mut(i) {
                    *title = value;
                }
            }
            Action::ApplyTitles => {
                let titles = self.tab.panel_titles.clone();
                self.edit_panels(|panel| {
                    let mut titles = titles.clone();
                    titles.truncate(panel.columns().len());
                    while titles.last().is_some_and(|t| t.trim().is_empty()) {
                        titles.pop();
                    }
                    panel.titles = titles;
                });
            }
            Action::Place(place) => self.edit_panels(|panel| panel.place = place),
            Action::Rounded(rounded) => {
                // Corners are the shape's own: a rounded rectangle's radius.
                let before = self.tab.doc.clone();
                let selected = self.tab.selected.clone();
                for g in &mut self.tab.doc.graphics {
                    if g.panel.is_some() && selected.contains(&g.id) {
                        g.kind = if rounded {
                            GraphicKind::RoundedRectangle
                        } else {
                            GraphicKind::Rectangle
                        };
                        g.style.corner_pt = rounded.then_some(Panel::ROUNDED_PT);
                    }
                }
                self.changed(before);
                self.sync_graphics();
            }
            Action::Border(border) => self.edit_panels(|panel| panel.border = border),
            Action::AddDivider(vertical) => {
                let doc = self.tab.doc.clone();
                let positions: Vec<(u64, f32)> = self
                    .selected_panels()
                    .map(|g| {
                        let at = reshiki::graphics::panel::divider_position(&doc, g.id, vertical);
                        (g.id, at)
                    })
                    .collect();
                let before = self.tab.doc.clone();
                for (id, at) in positions {
                    if let Some(panel) = self.panel_mut(id)
                        && !panel
                            .dividers
                            .iter()
                            .any(|d| d.vertical == vertical && (d.at - at).abs() < 0.05)
                    {
                        // Titles stay with their columns: the split column's
                        // new right half starts untitled.
                        if vertical
                            && let Some(column) =
                                panel.columns().iter().position(|c| c.0 < at && at < c.1)
                            && column + 1 < panel.titles.len()
                        {
                            panel.titles.insert(column + 1, String::new());
                        }
                        panel.dividers.push(Divider { vertical, at });
                    }
                }
                self.changed(before);
                self.sync_figure();
            }
            Action::DividerAt(i, value) => {
                if let Some(input) = self.tab.panel_dividers.get_mut(i) {
                    *input = value;
                }
            }
            Action::ApplyDivider(i) => {
                let parsed = self
                    .tab
                    .panel_dividers
                    .get(i)
                    .and_then(|s| s.trim().trim_end_matches('%').trim().parse::<f32>().ok())
                    .filter(|v| (1. ..=99.).contains(v));
                let crowded = |percent: f32| {
                    self.selected_panels().any(|g| {
                        g.panel.as_ref().is_some_and(|panel| {
                            let mine = panel.dividers.get(i);
                            panel.dividers.iter().enumerate().any(|(j, d)| {
                                j != i
                                    && mine.is_some_and(|m| m.vertical == d.vertical)
                                    && (d.at - percent / 100.).abs() < 0.05
                            })
                        })
                    })
                };
                match parsed {
                    Some(percent) if crowded(percent) => {
                        self.error = true;
                        self.status = "Keep dividers at least 5% apart".into();
                    }
                    Some(percent) => self.edit_panels(|panel| {
                        if let Some(d) = panel.dividers.get_mut(i) {
                            d.at = percent / 100.;
                        }
                    }),
                    None => {
                        self.error = true;
                        self.status = "Enter a divider position from 1 to 99%".into();
                    }
                }
            }
            Action::RemoveDivider(i) => self.edit_panels(|panel| {
                let Some(divider) = panel.dividers.get(i).copied() else {
                    return;
                };
                // The columns on both sides merge; the left one's title stays.
                if divider.vertical
                    && let Some(column) = panel.columns().iter().position(|c| c.1 == divider.at)
                    && column + 1 < panel.titles.len()
                {
                    let right = panel.titles.remove(column + 1);
                    if let Some(left) = panel.titles.get_mut(column)
                        && left.trim().is_empty()
                    {
                        *left = right;
                    }
                }
                panel.dividers.remove(i);
                panel.titles.truncate(panel.columns().len());
            }),
            Action::Fit => self.fit_panels(),
            Action::Arrange(flow) => self.arrange_layout(flow),
            Action::Columns(value) => self.tab.layout_columns = value,
            Action::ApplyColumns => match self.tab.layout_columns.trim().parse::<u16>() {
                Ok(columns) if (1..=64).contains(&columns) => {
                    self.arrange_layout(Flow::Grid(columns))
                }
                _ => {
                    self.error = true;
                    self.status = "Enter 1–64 columns".into();
                }
            },
            Action::Gutter(value) => self.tab.layout_gutter = value,
            Action::ApplyGutter => match self.tab.layout_gutter.trim().parse::<f32>() {
                Ok(gutter) if gutter.is_finite() && (0. ..=288.).contains(&gutter) => {
                    self.edit_layout(|layout| layout.gutter_pt = gutter)
                }
                _ => {
                    self.error = true;
                    self.status = "Enter a gutter of 0–288 pt".into();
                }
            },
            Action::Align(align) => self.edit_layout(|layout| layout.align = align),
        }
    }

    fn selected_panels(&self) -> impl Iterator<Item = &Graphic> {
        self.tab
            .doc
            .graphics
            .iter()
            .filter(|g| g.panel.is_some() && self.tab.selected.contains(&g.id))
    }

    fn panel_mut(&mut self, id: u64) -> Option<&mut Panel> {
        self.tab
            .doc
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.panel.as_mut())
    }

    /// Change every selected panel in one Undo step. A title band that
    /// appears or goes grows or shrinks the panel upward, so it never covers
    /// what the panel holds.
    fn edit_panels(&mut self, edit: impl Fn(&mut Panel)) {
        let before = self.tab.doc.clone();
        let text = self.tab.doc.drawing_style.text_style();
        let selected = self.tab.selected.clone();
        for g in &mut self.tab.doc.graphics {
            if !selected.contains(&g.id) {
                continue;
            }
            let length = g.axis_y.distance(Point::default());
            // A band never takes more than the panel's height.
            let band = g.panel_bar_height(&text).min(length);
            let Some(panel) = &mut g.panel else {
                continue;
            };
            edit(panel);
            let grow = g.panel_bar_height(&text) - band;
            if grow != 0. && length > 0. {
                let (ux, uy) = (g.axis_y.x / length, g.axis_y.y / length);
                g.origin = g.origin.offset(-ux * grow, -uy * grow);
                g.axis_y = Point::new(g.axis_y.x + ux * grow, g.axis_y.y + uy * grow);
            }
        }
        self.changed(before);
        self.sync_figure();
    }

    /// Fit the selection in a tinted, rounded panel behind it, grouped with
    /// it so they move together.
    fn wrap_in_panel(&mut self) {
        let ids = self.tab.doc.complete_selection(&self.tab.selected);
        let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.tab.doc, &ids) else {
            self.error = true;
            self.status = "Select what the panel should hold first".into();
            return;
        };
        let before = self.tab.doc.clone();
        let pad = reshiki::style::DEFAULT.world(reshiki::graphics::panel::PADDING_PT);
        let id = self.tab.doc.next_id();
        let mut panel = Graphic::dragged(
            id,
            GraphicKind::RoundedRectangle,
            lo.offset(-pad, -pad),
            hi.offset(pad, pad),
            GraphicStyle {
                stroke: Color::Palette(Hue::Blue, Row::Strong),
                fill: Some(Color::Palette(Hue::Blue, Row::Tint)),
                width_pt: self.tab.doc.drawing_style.line_width_pt,
                corner_pt: Some(Panel::ROUNDED_PT),
                ..Default::default()
            },
            reshiki::graphics::BracketSides::Both,
            false,
        );
        panel.panel = Some(Panel::default());
        // Behind everything it holds: the lowest layer, drawn first.
        panel.layer = self
            .tab
            .doc
            .graphics
            .iter()
            .filter(|g| ids.contains(&g.id))
            .map(|g| g.layer)
            .chain([-1])
            .min()
            .unwrap_or(-1);
        self.tab.doc.graphics.insert(0, panel);
        let mut members = ids;
        members.push(id);
        match self.tab.doc.group_selection(&members) {
            Ok(members) => {
                self.changed(before);
                self.set_selection(members);
                self.tool = crate::canvas::Tool::Select;
                self.status = format!(
                    "Wrapped in a panel · {}-click edits what it holds",
                    crate::platform::alt_key()
                );
            }
            Err(error) => {
                self.tab.doc = before;
                self.error = true;
                self.status = error;
            }
        }
    }

    /// Refit each selected panel around what it is grouped with, or else
    /// around what lies inside it.
    fn fit_panels(&mut self) {
        let before = self.tab.doc.clone();
        let panels: Vec<u64> = self.selected_panels().map(|g| g.id).collect();
        for id in reshiki::graphics::panel::inner_first(&self.tab.doc, panels) {
            reshiki::graphics::panel::fit(&mut self.tab.doc, id);
        }
        self.changed(before);
    }

    /// The largest layout group inside the selection.
    pub(super) fn selected_layout(&self) -> Option<&reshiki::grouping::Group> {
        self.tab
            .doc
            .groups
            .iter()
            .filter(|g| {
                g.layout.is_some() && g.members.iter().all(|id| self.tab.selected.contains(id))
            })
            .max_by_key(|g| g.members.len())
    }

    fn arrange_layout(&mut self, flow: Flow) {
        let before = self.tab.doc.clone();
        let ids = self.tab.selected.clone();
        let gutter = self
            .selected_layout()
            .and_then(|g| g.layout.as_ref())
            .map_or(self.tab.doc.drawing_style.bond_length_pt, |l| l.gutter_pt);
        match reshiki::layouts::arrange(&mut self.tab.doc, &ids, flow, gutter) {
            Ok(group) => {
                let members = self
                    .tab
                    .doc
                    .groups
                    .iter()
                    .find(|g| g.id == group)
                    .map(|g| g.members.clone())
                    .unwrap_or_default();
                let selected = if self.tab.selected.len() > members.len() {
                    self.tab.selected.clone()
                } else {
                    members
                };
                self.changed(before);
                if self.error {
                    return;
                }
                self.set_selection(selected);
                self.status = format!(
                    "Lasting {} · {}-click edits one item",
                    match flow {
                        Flow::Row => "row",
                        Flow::Column => "column",
                        Flow::Grid(_) => "grid",
                    },
                    crate::platform::alt_key()
                );
            }
            Err(error) => {
                self.tab.doc = before;
                self.error = true;
                self.status = error;
            }
        }
    }

    fn edit_layout(&mut self, edit: impl Fn(&mut reshiki::layouts::Layout)) {
        let Some(id) = self.selected_layout().map(|g| g.id) else {
            return;
        };
        let before = self.tab.doc.clone();
        if let Some(layout) = self
            .tab
            .doc
            .groups
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.layout.as_mut())
        {
            edit(layout);
        }
        reshiki::layouts::reflow(&mut self.tab.doc, id, None, None);
        self.changed(before);
        self.sync_figure();
    }

    /// Refresh the panel and layout inputs from the selection.
    pub(super) fn sync_figure(&mut self) {
        let panel = self.selected_panels().next().and_then(|g| g.panel.clone());
        if let Some(panel) = panel {
            let mut titles = panel.titles.clone();
            titles.resize(panel.columns().len(), String::new());
            self.tab.panel_titles = titles;
            self.tab.panel_dividers = panel
                .dividers
                .iter()
                .map(|d| format!("{}", (d.at * 100.).round()))
                .collect();
        }
        let layout = self.selected_layout().and_then(|g| g.layout.clone());
        if let Some(layout) = layout {
            self.tab.layout_gutter = format!("{}", layout.gutter_pt);
            if let Flow::Grid(columns) = layout.flow {
                self.tab.layout_columns = columns.to_string();
            }
        }
    }

    /// Properties for the selected panels.
    pub(super) fn figure_panel_card(&self) -> Option<Element<'_, Message>> {
        let shape = self.selected_panels().next()?;
        let panel = shape.panel.as_ref()?;
        let rounded = shape.kind == GraphicKind::RoundedRectangle;
        let send = |action| Message::Figure(action);
        let toggle = |label, active: bool, action| {
            button(text(label).size(12).width(Length::Fill).center())
                .padding([6, 8])
                .width(Length::Fill)
                .style(control(active))
                .on_press(send(action))
        };
        let columns = panel.columns().len();
        let mut body = column![section("Panel")].spacing(8);
        for i in 0..columns {
            let value = self.tab.panel_titles.get(i).map_or("", String::as_str);
            body = body.push(
                reshiki::accessibility::text_input(
                    format!("panel-title-{i}"),
                    if columns == 1 {
                        "Panel title".to_owned()
                    } else {
                        format!("Title of column {}", i + 1)
                    },
                    if columns == 1 {
                        "Title"
                    } else {
                        "Column title"
                    },
                    value,
                )
                .style(crate::appearance::input_style)
                .on_input(move |v| send(Action::Title(i, v)))
                .on_submit(send(Action::ApplyTitles))
                .size(12)
                .padding(6),
            );
        }
        // Fill and shadow sit with the title: they are what a panel is for.
        let style = &self.tab.graphic_style;
        let restyle = |change| Some(Message::Graphics(super::graphics::Action::Style(change)));
        let palette = reshiki::palette::Palette::of(&self.tab.doc);
        body = body
            .push(
                super::color_popover::paper(
                    super::color_popover::palette_row(
                        &palette,
                        Row::Tint,
                        reshiki::palette::Hues::of(&self.tab.doc),
                        20.,
                        (style.fill.is_none(), restyle(GraphicChange::Fill(None))),
                        |hue| {
                            let color = Color::Palette(hue, Row::Tint);
                            (
                                style.fill == Some(color),
                                restyle(GraphicChange::Fill(Some(color))),
                            )
                        },
                    ),
                    self.tab.doc.canvas_theme,
                )
                .padding([6, 6]),
            )
            .push(
                row![
                    checkbox(panel.border)
                        .label("Outline")
                        .on_toggle(move |on| send(Action::Border(on)))
                        .size(14)
                        .text_size(12),
                    hover_hint(
                        checkbox(style.effects.shadow_pt.is_some())
                            .label("Shadow")
                            .on_toggle(|on| {
                                Message::Graphics(super::graphics::Action::Style(
                                    GraphicChange::Shadow(
                                        on.then_some(reshiki::graphics::Effects::SHADOW_PT),
                                    ),
                                ))
                            })
                            .size(14)
                            .text_size(12),
                        "A grey drop shadow below and to the right",
                        tooltip::Position::Top,
                    ),
                ]
                .spacing(14),
            )
            .push(
                row![
                    toggle(
                        "Title bar",
                        panel.place == TitlePlace::Bar,
                        Action::Place(TitlePlace::Bar)
                    ),
                    toggle(
                        "In border",
                        panel.place == TitlePlace::Border,
                        Action::Place(TitlePlace::Border)
                    ),
                ]
                .spacing(4),
            )
            .push(
                row![
                    toggle("Rounded", rounded, Action::Rounded(true)),
                    toggle("Square", !rounded, Action::Rounded(false)),
                ]
                .spacing(4),
            )
            .push(
                row![
                    hover_hint(
                        panel_action("Split columns", send(Action::AddDivider(true))),
                        "Add a vertical divider in the widest gap between what the panel holds",
                        tooltip::Position::Top,
                    ),
                    hover_hint(
                        panel_action("Split rows", send(Action::AddDivider(false))),
                        "Add a horizontal divider in the widest gap between what the panel holds",
                        tooltip::Position::Top,
                    ),
                ]
                .spacing(6),
            );
        for (i, divider) in panel.dividers.iter().enumerate() {
            let value = self.tab.panel_dividers.get(i).map_or("", String::as_str);
            body = body.push(
                row![
                    text(if divider.vertical {
                        "Column divider at"
                    } else {
                        "Row divider at"
                    })
                    .size(11)
                    .width(Length::Fill),
                    reshiki::accessibility::text_input(
                        format!("panel-divider-{i}"),
                        format!("Divider {} position (%)", i + 1),
                        "50",
                        value,
                    )
                    .style(crate::appearance::input_style)
                    .on_input(move |v| send(Action::DividerAt(i, v)))
                    .on_submit(send(Action::ApplyDivider(i)))
                    .size(12)
                    .padding(5)
                    .width(44),
                    text("%").size(11),
                    button(text("×").size(12))
                        .padding([3, 7])
                        .style(crate::appearance::secondary)
                        .on_press(send(Action::RemoveDivider(i))),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        Some(
            body.push(hover_hint(
                panel_action("Fit to contents", send(Action::Fit)),
                "Fit the panel around what it is grouped with",
                tooltip::Position::Top,
            ))
            .push(
                text("Enter applies titles and positions. Line and gradient are under Graphic style.")
                    .size(11)
                    .style(muted_text),
            )
            .into(),
        )
    }

    /// Properties for the selected lasting layout.
    pub(super) fn layout_card(&self) -> Option<Element<'_, Message>> {
        let layout = self.selected_layout()?.layout.clone()?;
        let send = |action| Message::Figure(action);
        let toggle = |label, active: bool, action| {
            button(text(label).size(12).width(Length::Fill).center())
                .padding([6, 8])
                .width(Length::Fill)
                .style(control(active))
                .on_press(send(action))
        };
        let grid = matches!(layout.flow, Flow::Grid(_));
        let columns = match layout.flow {
            Flow::Grid(c) => c,
            _ => 2,
        };
        let mut body = column![
            section("Layout"),
            row![
                toggle("Row", layout.flow == Flow::Row, Action::Arrange(Flow::Row)),
                toggle(
                    "Column",
                    layout.flow == Flow::Column,
                    Action::Arrange(Flow::Column)
                ),
                toggle("Grid", grid, Action::Arrange(Flow::Grid(columns))),
            ]
            .spacing(4),
        ]
        .spacing(8);
        let input = |id: &'static str,
                     name: &'static str,
                     value: &str,
                     edit: fn(String) -> Action,
                     apply| {
            reshiki::accessibility::text_input(id, name, "", value)
                .style(crate::appearance::input_style)
                .on_input(move |v| send(edit(v)))
                .on_submit(send(apply))
                .size(12)
                .padding(5)
                .width(52)
        };
        let mut numbers = row![
            text("Gutter (pt)").size(11),
            input(
                "layout-gutter",
                "Layout gutter (pt)",
                &self.tab.layout_gutter,
                Action::Gutter,
                Action::ApplyGutter
            )
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if grid {
            numbers = numbers.push(text("Columns").size(11)).push(input(
                "layout-columns",
                "Layout columns",
                &self.tab.layout_columns,
                Action::Columns,
                Action::ApplyColumns,
            ));
        }
        body = body.push(numbers).push(
            row(Align::ALL.into_iter().map(|align| {
                toggle(
                    match align {
                        Align::Start => "Start",
                        Align::Center => "Center",
                        Align::End => "End",
                    },
                    layout.align == align,
                    Action::Align(align),
                )
                .into()
            }))
            .spacing(4),
        );
        Some(
            body.push(
                text(
                    "Items keep these gutters when one changes size. Start and End align top or left, and bottom or right.",
                )
                .size(11)
                .style(muted_text),
            )
            .into(),
        )
    }
}

#[cfg(test)]
mod tests;
