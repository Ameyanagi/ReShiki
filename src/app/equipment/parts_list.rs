//! Insert the equipment inventory as native table cells with editable text.
use super::{Action, App, Message};
use iced::Task;
use reshiki::{
    document::{Annotation, Document, Point},
    equipment::parts_list::{PartConnection, PartRow},
    graphics::table::{self, Frame},
    style,
    typography::{TextFormat, TextStyle},
};
use std::collections::BTreeSet;

const HEADERS: [&str; 7] = [
    "Item",
    "Equipment",
    "Tube specification",
    "Cut length",
    "From",
    "To",
    "Passes through",
];
const COLUMN_PT: [f32; HEADERS.len()] = [38., 145., 106., 96., 122., 122., 126.];
type Cells = [String; HEADERS.len()];

impl App {
    pub(in crate::app) fn has_equipment(&self) -> bool {
        self.tab.doc.graphics.iter().any(|g| g.piece.is_some())
    }

    fn equipment_parts_cells(&self) -> Result<Vec<Cells>, String> {
        let selection = if self.pieces_selected() {
            self.tab.selected.as_slice()
        } else {
            &[]
        };
        let rows = reshiki::equipment::parts_list(&self.tab.doc, selection)?;
        if rows.is_empty() {
            return Err("Add equipment to the drawing first".into());
        }
        let listed: BTreeSet<u64> = rows.iter().map(|row| row.id).collect();
        Ok(rows.iter().map(|row| cells(row, &listed)).collect())
    }

    pub(super) fn insert_equipment_parts_list(&mut self) {
        let result = self.equipment_parts_cells().and_then(|rows| {
            let mut doc = self.tab.doc.clone();
            let old_cells = doc
                .graphics
                .iter()
                .filter_map(|g| g.table.as_ref())
                .map(|table| table.cells.len())
                .sum::<usize>();
            let rows_with_header = rows.len() + 1;
            if rows_with_header > table::MAX_CELLS.saturating_sub(old_cells) / HEADERS.len() {
                return Err("This parts list exceeds the drawing's 4096-cell limit. Use Copy equipment parts list for the full list.".into());
            }
            let id = doc.next_id();
            let (lo, hi) = doc.bounds();
            insert_table(&mut doc, id, &rows, Point::new(lo.x + 30., hi.y + 24.))?;
            self.try_replace_document(doc, vec![id])?;
            Ok(())
        });
        match result {
            Ok(()) => {
                self.select_tool(crate::canvas::Tool::Select);
                if let Some(bounds) =
                    reshiki::scene::selection_bounds(&self.tab.doc, &self.tab.selected)
                {
                    self.fit_bounds(bounds);
                }
                self.status = "Inserted the equipment parts list".into();
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }

    pub(super) fn copy_equipment_parts_list(&mut self) -> Task<Message> {
        let rows = match self.equipment_parts_cells() {
            Ok(rows) => rows,
            Err(error) => {
                self.status = error;
                self.error = true;
                return Task::none();
            }
        };
        let mut text = HEADERS.join("\t");
        for row in rows {
            text.push('\n');
            text.push_str(
                &row.map(|cell| {
                    let mut cell = cell.replace(['\n', '\r', '\t'], " · ");
                    if cell.trim_start().starts_with(['=', '+', '-', '@']) {
                        cell.insert(0, '\'');
                    }
                    cell
                })
                .join("\t"),
            );
        }
        self.status = "Copying the equipment parts list…".into();
        self.error = false;
        let (tab, epoch) = (self.tab.id, self.tab.file_epoch);
        #[cfg(not(target_arch = "wasm32"))]
        let write = reshiki::clipboard::write_text(text);
        #[cfg(target_arch = "wasm32")]
        let write = crate::web::write_text(text);
        Task::perform(write, move |result| {
            Message::Equipment(Action::PartsListCopied { tab, epoch, result })
        })
    }
}

fn cells(row: &PartRow, listed: &BTreeSet<u64>) -> Cells {
    let link = |connection: &PartConnection| {
        if listed.contains(&connection.id) {
            format!("#{} · {}", connection.id, connection.port_name)
        } else {
            format!("{} · {}", connection.label, connection.port_name)
        }
    };
    let equipment = if row.tube.is_some() {
        let geometry = row.variant.get("geometry");
        geometry.map_or_else(|| row.name.clone(), |v| format!("{}\n{v}", row.name))
    } else {
        let variants: Vec<_> = super::variant_descriptions(&row.variant)
            .filter(|value| !row.name.contains(value.as_str()))
            .collect();
        if variants.is_empty() {
            row.name.clone()
        } else {
            format!("{}\n{}", row.name, variants.join(" · "))
        }
    };
    let mut values = [
        format!("#{}", row.id),
        equipment,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ];
    if let Some(tube) = &row.tube {
        values[2] = format!("{}\n{} mm OD", tube.material, tube.od_mm);
        if let Some(inner) = tube.id_mm {
            values[2].push_str(&format!(" · {inner} mm ID"));
        }
        // A bend contributes an arc length. Keep the rounded report readable
        // while leaving the calculated cut length at full precision in the model.
        let cut_length = format!("{:.3}", tube.cut_length_mm);
        values[3] = format!(
            "{} mm",
            cut_length.trim_end_matches('0').trim_end_matches('.')
        );
        for (at, end) in values[4..6].iter_mut().zip(&tube.ends) {
            *at = end
                .connection
                .as_ref()
                .map_or_else(|| "Free end".to_owned(), link);
        }
        values[6] = tube
            .through
            .iter()
            .map(|pass| link(&pass.fitting))
            .collect::<Vec<_>>()
            .join("\n");
    }
    values
}

fn insert_table(doc: &mut Document, id: u64, rows: &[Cells], start: Point) -> Result<(), String> {
    let widths = COLUMN_PT.map(|width| style::DEFAULT.world(width));
    let width: f32 = widths.iter().sum();
    let header = HEADERS.map(str::to_owned);
    let data: Vec<_> = std::iter::once(&header).chain(rows).collect();
    let heights: Vec<_> = data
        .iter()
        .enumerate()
        .map(|(row, cells)| {
            cells
                .iter()
                .zip(widths)
                .zip(COLUMN_PT)
                .map(|((value, width), pt)| cell_height(value, width, pt, row == 0))
                .fold(0., f32::max)
        })
        .collect();
    let height: f32 = heights.iter().sum();
    let mut graphic = table::editor::dragged(
        id,
        start,
        start.offset(width, height),
        [data.len(), HEADERS.len()],
        0.45,
    )?;
    let mut next_id = id;
    let mut top = 0.;
    let grid = graphic
        .table
        .as_mut()
        .ok_or("Could not create the parts table")?;
    for (row, (values, row_height)) in data.iter().zip(heights).enumerate() {
        let mut left = 0.;
        for (column, (value, cell_width)) in values.iter().zip(widths).enumerate() {
            let cell = &mut grid.cells[row * HEADERS.len() + column];
            cell.lo = Point::new(left / width, top / height);
            cell.hi = Point::new((left + cell_width) / width, (top + row_height) / height);
            if !value.is_empty() {
                let frame = Frame {
                    origin: start.offset(left, top),
                    axis_x: Point::new(cell_width, 0.),
                    axis_y: Point::new(0., row_height),
                };
                next_id = next_id
                    .checked_add(1)
                    .ok_or("The drawing has no free object IDs")?;
                let format = cell_format(COLUMN_PT[column], row == 0);
                let text = table::editor::new_text(next_id, frame, value.clone(), format)?;
                cell.members.push(next_id);
                doc.annotations.push(text);
            }
            left += cell_width;
        }
        top += row_height;
    }
    doc.graphics.push(graphic);
    Ok(())
}

fn cell_format(width_pt: f32, header: bool) -> TextFormat {
    TextFormat {
        style: TextStyle {
            size_pt: 9.5,
            bold: header,
            ..TextStyle::default()
        },
        width_pt: Some(0.9 * width_pt),
        ..TextFormat::default()
    }
}

fn cell_height(text: &str, width: f32, width_pt: f32, header: bool) -> f32 {
    // Measure with the same text wrapping used by a native table. The frame's
    // height does not affect wrapping in an upright cell.
    let annotation = Annotation {
        id: 0,
        position: Point::new(width * 0.05, 1.),
        text: text.to_owned(),
        format: cell_format(width_pt, header),
        angle: 0.,
    };
    let frame = Frame {
        origin: Point::default(),
        axis_x: Point::new(width, 0.),
        axis_y: Point::new(0., 100.),
    };
    let layout = reshiki::scene::table_annotation_layout(&annotation, frame);
    (layout.height + style::DEFAULT.world(4.)).max(style::DEFAULT.world(16.)) / 0.84
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::equipment::{
        builtin, placed,
        solve::{self, JointRef},
    };

    #[test]
    fn equipment_parts_command_creates_an_editable_table_from_the_whole_drawing() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let doc = &mut app.tab.doc;
        let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
        for (id, key) in [
            (1, "reshiki/compression-cap-1-4"),
            (2, "reshiki/compression-tube-60-1-4"),
        ] {
            let piece = builtin::find(key).unwrap();
            doc.graphics
                .push(placed::graphic(id, piece, Point::default(), scale));
        }
        let cap = JointRef {
            piece: 1,
            joint: "tube".into(),
        };
        let tube = JointRef {
            piece: 2,
            joint: "a".into(),
        };
        solve::connect(doc, &[2], &cap, &tube).unwrap();
        let before = doc.clone();
        let _ = app.update(Message::Equipment(Action::PartsList));
        assert!(!app.error, "{}", app.status);
        let doc = &app.tab.doc;
        let table_id = app.tab.selected[0];
        let table = doc
            .graphics
            .iter()
            .find(|g| g.id == table_id)
            .unwrap()
            .table
            .as_ref()
            .unwrap();
        assert_eq!(table.cells.len(), 3 * HEADERS.len());
        assert_eq!(doc.seats, before.seats);
        let membership = table::Membership::new(doc).unwrap();
        assert!(
            doc.annotations
                .iter()
                .all(|note| membership.cell_of(note.id).is_some())
        );
        assert!(doc.annotations.iter().any(|note| note.text == "60 mm"));
        assert!(
            doc.annotations
                .iter()
                .any(|note| note.text.starts_with("#1 · "))
        );
        let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened.seats, doc.seats);
        assert_eq!(reopened.annotations, doc.annotations);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }
}
