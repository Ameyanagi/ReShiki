//! Native table creation and explicit cell editing. Cell ownership changes
//! never infer identity from coordinates or move molecular geometry.
use super::{App, Message};
use crate::canvas::Tool;
use iced::widget::{column, row, text};
use iced::{Element, Length};
use reshiki::{
    document::{Annotation, Point},
    graphics::{
        GraphicKind,
        table::{self, Border, CellRef, Membership, Side},
    },
    palette::Color,
};

#[derive(Debug, Clone)]
pub enum Action {
    Rows(String),
    Columns(String),
    ApplyGrid,
    Cell(u32),
    SelectTable,
    SelectContents,
    Done,
    Text(String),
    AddText,
    Width(String),
    ApplyWidth,
    AssignSelection,
    ReleaseSelection,
}

#[derive(Debug, Clone)]
pub(super) struct Inputs {
    pub grid: [usize; 2],
    rows: String,
    columns: String,
    active: Option<CellRef>,
    text: String,
    width: String,
    width_source: Option<(CellRef, f32)>,
    selection: Vec<u64>,
}
impl Default for Inputs {
    fn default() -> Self {
        Self {
            grid: table::editor::DEFAULT_GRID,
            rows: "2".into(),
            columns: "3".into(),
            active: None,
            text: String::new(),
            width: "0.6".into(),
            width_source: None,
            selection: Vec::new(),
        }
    }
}

impl App {
    pub(super) fn active_table_cell(&self) -> Option<CellRef> {
        let target = self.tab.table_inputs.active?;
        (self.tool != Tool::Graphic(GraphicKind::Table)).then_some(target)
    }

    pub(super) fn sync_tables(&mut self) {
        if !self.tab.doc.graphics.iter().any(|g| g.table.is_some()) {
            self.tab.table_inputs.active = None;
            self.tab.table_inputs.width_source = None;
            return;
        }
        let selected = &self.tab.selected;
        let selection_changed = *selected != self.tab.table_inputs.selection;
        self.tab.table_inputs.selection = selected.clone();
        let direct = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| g.table.is_some() && selected.contains(&g.id));
        let owner = Membership::new(&self.tab.doc).ok().and_then(|m| {
            let mut owners = selected.iter().filter_map(|id| m.cell_of(*id));
            let first = owners.next()?;
            owners.all(|owner| owner == first).then_some(first)
        });
        let from_selection = direct
            .and_then(|g| {
                let cells = &g.table.as_ref()?.cells;
                self.tab
                    .table_inputs
                    .active
                    .filter(|a| a.table == g.id && cells.iter().any(|c| c.id == a.cell))
                    .or_else(|| {
                        cells.first().map(|c| CellRef {
                            table: g.id,
                            cell: c.id,
                        })
                    })
            })
            .or(owner);
        let target = if selection_changed {
            from_selection.or(self.tab.table_inputs.active)
        } else {
            self.tab.table_inputs.active.or(from_selection)
        };
        let target = target.filter(|target| {
            self.tab.doc.graphics.iter().any(|g| {
                g.id == target.table
                    && g.table
                        .as_ref()
                        .is_some_and(|t| t.cell(target.cell).is_some())
            })
        });
        if target != self.tab.table_inputs.active {
            self.tab.table_inputs.active = target;
        }
        self.sync_table_width();
    }

    fn sync_table_width(&mut self) {
        if let Some(target) = self.tab.table_inputs.active
            && let Some(border) = self
                .tab
                .doc
                .graphics
                .iter()
                .find(|g| g.id == target.table)
                .and_then(|g| g.table.as_ref())
                .and_then(|t| t.cell(target.cell))
                .and_then(|cell| cell.borders.first())
        {
            let source = Some((target, border.width_pt));
            if source != self.tab.table_inputs.width_source {
                self.tab.table_inputs.width = border.width_pt.to_string();
                self.tab.table_inputs.width_source = source;
            }
        }
    }

    pub(super) fn table_action(&mut self, action: Action) {
        match action {
            Action::Rows(value) => self.tab.table_inputs.rows = value,
            Action::Columns(value) => self.tab.table_inputs.columns = value,
            Action::Text(value) => self.tab.table_inputs.text = value,
            Action::Width(value) => self.tab.table_inputs.width = value,
            Action::ApplyGrid => {
                let counts = self
                    .tab
                    .table_inputs
                    .rows
                    .parse::<usize>()
                    .ok()
                    .zip(self.tab.table_inputs.columns.parse::<usize>().ok())
                    .map(|(r, c)| [r, c])
                    .filter(|grid| table::editor::valid_grid(*grid));
                if let Some(grid) = counts {
                    self.tab.table_inputs.grid = grid;
                    self.status = format!("New tables: {} rows × {} columns", grid[0], grid[1]);
                    self.error = false;
                } else {
                    self.status =
                        "Use positive row and column counts with at most 4096 cells.".into();
                    self.error = true;
                }
            }
            Action::Done => {
                self.tab.table_inputs.active = None;
                self.set_selection(vec![]);
            }
            other => self.edit_table_action(other),
        }
    }

    fn edit_table_action(&mut self, action: Action) {
        self.sync_tables();
        let Some(mut target) = self.tab.table_inputs.active else {
            return;
        };
        if let Action::Cell(cell) = action {
            target.cell = cell;
            if self
                .tab
                .doc
                .graphics
                .iter()
                .find(|g| g.id == target.table)
                .and_then(|g| g.table.as_ref())
                .and_then(|t| t.cell(cell))
                .is_some()
            {
                self.tab.table_inputs.active = Some(target);
                self.sync_table_width();
            }
            return;
        }
        if matches!(action, Action::SelectTable) {
            self.set_selection(vec![target.table]);
            return;
        }
        if matches!(action, Action::SelectContents) {
            let ids = self
                .tab
                .doc
                .graphics
                .iter()
                .find(|g| g.id == target.table)
                .and_then(|g| g.table.as_ref())
                .and_then(|t| t.cell(target.cell))
                .map(|c| c.members.clone())
                .unwrap_or_default();
            self.set_selection(ids);
            return;
        }
        let before = self.tab.doc.clone();
        let result: Result<Option<u64>, String> = (|| {
            match action {
                Action::AssignSelection | Action::ReleaseSelection => {
                    let ids: Vec<_> = self
                        .tab
                        .selected
                        .iter()
                        .copied()
                        .filter(|id| *id != target.table)
                        .collect();
                    if ids.is_empty() {
                        return Err("Select the content to assign or release.".into());
                    }
                    table::relink(
                        &mut self.tab.doc,
                        matches!(action, Action::AssignSelection).then_some(target),
                        &ids,
                    )?;
                }
                Action::AddText => {
                    let value = self.tab.table_inputs.text.clone();
                    if value.trim().is_empty() {
                        return Err("Enter text for this cell.".into());
                    }
                    let graphic = self
                        .tab
                        .doc
                        .graphics
                        .iter()
                        .find(|g| g.id == target.table)
                        .ok_or("Table was removed")?;
                    let cell = graphic
                        .table
                        .as_ref()
                        .and_then(|t| t.cell(target.cell))
                        .ok_or("Cell was removed")?;
                    let frame = cell.frame(graphic).ok_or("Invalid cell frame")?;
                    let id = self.tab.doc.next_id();
                    let format = reshiki::typography::TextFormat {
                        style: self.tab.doc.drawing_style.text_style(),
                        ..Default::default()
                    };
                    self.tab.doc.annotations.push(Annotation {
                        id,
                        position: frame.point(Point::new(0.05, 0.08)),
                        text: value,
                        format,
                        angle: 0.,
                    });
                    table::relink(&mut self.tab.doc, Some(target), &[id])?;
                    return Ok(Some(id));
                }
                Action::ApplyWidth => {
                    let width = self
                        .tab
                        .table_inputs
                        .width
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite() && (0.0..=12.0).contains(v))
                        .ok_or("Use a border width from 0 to 12 pt.")?;
                    let cell = self
                        .tab
                        .doc
                        .graphics
                        .iter_mut()
                        .find(|g| g.id == target.table)
                        .and_then(|g| g.table.as_mut())
                        .and_then(|t| t.cell_mut(target.cell))
                        .ok_or("Cell was removed")?;
                    for side in [Side::Left, Side::Top, Side::Bottom, Side::Right] {
                        if let Some(border) = cell.borders.iter_mut().find(|b| b.side == side) {
                            border.width_pt = width;
                        } else {
                            cell.borders.push(Border {
                                side,
                                width_pt: width,
                                color: Color::Ink,
                                pattern: Default::default(),
                            });
                        }
                    }
                }
                _ => return Ok(None),
            }
            Ok(None)
        })();
        match result {
            Ok(selected) => {
                self.error = false;
                self.changed(before);
                if self.error {
                    self.sync_tables();
                    return;
                }
                if let Some(id) = selected {
                    self.set_selection(vec![id]);
                    self.tab.table_inputs.text.clear();
                }
                self.sync_tables();
            }
            Err(error) => {
                self.tab.doc = before;
                self.error = true;
                self.status = error;
            }
        }
    }

    pub(super) fn table_card(&self) -> Option<Element<'_, Message>> {
        use super::workspace::{accessible_command, muted_text, section};
        let send = Message::Table;
        let input = |id, label, value: &str| {
            reshiki::accessibility::text_input(id, label, "", value)
                .style(crate::appearance::input_style)
                .size(12)
                .padding(5)
        };
        if self.tool == Tool::Graphic(GraphicKind::Table) {
            let inputs = &self.tab.table_inputs;
            return Some(
                column![
                    section("Table"),
                    text("Drag on the canvas to draw the grid.")
                        .size(12)
                        .style(muted_text),
                    row![
                        text("Rows").size(12),
                        input("table-rows", "New table rows", &inputs.rows)
                            .on_input(move |v| send(Action::Rows(v)))
                            .on_submit(send(Action::ApplyGrid))
                            .width(60),
                        text("Columns").size(12),
                        input("table-columns", "New table columns", &inputs.columns)
                            .on_input(move |v| send(Action::Columns(v)))
                            .on_submit(send(Action::ApplyGrid))
                            .width(60)
                    ]
                    .spacing(6),
                    accessible_command(
                        "table-grid-apply",
                        "Use these dimensions for new tables",
                        "Apply grid"
                    )
                    .on_press(send(Action::ApplyGrid)),
                ]
                .spacing(8)
                .into(),
            );
        }
        let target = self.tab.table_inputs.active?;
        let graphic = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| g.id == target.table)?;
        let table = graphic.table.as_ref()?;
        let cell = table.cell(target.cell)?;
        let inputs = &self.tab.table_inputs;
        let choices = table.cells.iter().map(|c| c.id).collect::<Vec<_>>();
        let chooser = reshiki::accessibility::select(
            "table-active-cell",
            "Active table cell",
            choices.clone(),
            cell.id.to_string(),
            move |id| send(Action::Cell(id)),
            iced::widget::pick_list(choices, Some(cell.id), move |id| send(Action::Cell(id)))
                .text_size(12),
        );
        Some(
            column![
                section("Table cell"),
                row![
                    text("Cell").size(12),
                    chooser,
                    text(format!("{} objects", cell.members.len())).size(12)
                ]
                .spacing(6),
                row![
                    accessible_command("table-select", "Select the table", "Select table")
                        .on_press(send(Action::SelectTable)),
                    accessible_command(
                        "table-cell-select",
                        "Select this cell's contents",
                        "Select contents"
                    )
                    .on_press(send(Action::SelectContents))
                ]
                .spacing(6),
                input("table-cell-text", "New cell text", &inputs.text)
                    .on_input(move |v| send(Action::Text(v)))
                    .on_submit(send(Action::AddText))
                    .width(Length::Fill),
                accessible_command(
                    "table-cell-add-text",
                    "Add editable text to this cell",
                    "Add text"
                )
                .on_press(send(Action::AddText)),
                row![
                    text("Border (pt)").size(12),
                    input(
                        "table-cell-width",
                        "Cell border width in points",
                        &inputs.width
                    )
                    .on_input(move |v| send(Action::Width(v)))
                    .on_submit(send(Action::ApplyWidth))
                    .width(65),
                    accessible_command(
                        "table-cell-width-apply",
                        "Set all four cell border widths",
                        "Apply"
                    )
                    .on_press(send(Action::ApplyWidth))
                ]
                .spacing(6),
                text("Assigning content keeps its position and clips it to this cell.")
                    .size(12)
                    .style(muted_text),
                row![
                    accessible_command(
                        "table-cell-assign",
                        "Assign selected content to this cell",
                        "Assign selection"
                    )
                    .on_press(send(Action::AssignSelection)),
                    accessible_command(
                        "table-cell-release",
                        "Release selected content from its cell",
                        "Release selection"
                    )
                    .on_press(send(Action::ReleaseSelection))
                ]
                .spacing(6),
                accessible_command("table-cell-done", "Finish editing this table cell", "Done")
                    .on_press(send(Action::Done)),
            ]
            .spacing(8)
            .into(),
        )
    }
}

#[cfg(test)]
mod tests;
