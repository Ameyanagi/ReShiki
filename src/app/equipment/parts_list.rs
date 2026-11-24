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
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
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
        // Like identity text copies, publication owns the global slot until
        // its receipt, even if the source document is replaced or closed.
        self.copy_as_busy = true;
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
    } else if let Some(hose) = &row.hose {
        for ((at, end), interface) in values[4..6]
            .iter_mut()
            .zip(&hose.ends)
            .zip(&hose.interfaces)
        {
            let connection = end
                .connection
                .as_ref()
                .map_or_else(|| "Free end".to_owned(), link);
            *at = format!("{interface}\n{connection}");
        }
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
    let mut cells = grid.cells.iter_mut();
    for (row, (values, row_height)) in data.iter().zip(heights).enumerate() {
        let mut left = 0.;
        for ((value, cell_width), width_pt) in values.iter().zip(widths).zip(COLUMN_PT) {
            let cell = cells.next().ok_or("The parts table is missing a cell")?;
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
                let format = cell_format(width_pt, row == 0);
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

    fn equipment_drawing() -> App {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
        app.tab.doc.graphics.push(placed::graphic(
            1,
            builtin::find("reshiki/compression-tube-60-1-4").unwrap(),
            Point::default(),
            scale,
        ));
        app.tab.saved = app.tab.doc.clone();
        app
    }

    fn copying_parts_list() -> App {
        let mut app = equipment_drawing();
        // Start the production command without polling its OS/browser
        // clipboard task; deliver the controlled receipt through tab routing.
        let task = app.update(Message::Equipment(Action::CopyPartsList));
        assert!(task.units() > 0);
        assert_eq!(app.status, "Copying the equipment parts list…");
        assert!(!app.error);
        assert!(app.clipboard_working());
        app
    }

    #[test]
    fn flexible_assembly_lists_its_ports_without_a_cut_length() {
        let mut doc = Document::default();
        let definition = builtin::find("reshiki/flexible-metal-hose-fm-1-4").unwrap();
        let (id, _) =
            solve::hose::place_hose(&mut doc, definition, [100., 100.], [400., 150.], 14.);
        let report = reshiki::equipment::parts_list(&doc, &[id]).unwrap();
        let values = cells(&report[0], &BTreeSet::from([id]));
        assert!(values[1].contains("SS-FM4TA4TA4-36"));
        assert!(values[1].contains("36 in (914.4 mm) catalog assembly"));
        assert!(values[3].is_empty());
        assert_eq!(values[4], "Ø6.35 mm compression male\nFree end");
        assert_eq!(values[5], "Ø6.35 mm compression male\nFree end");
    }

    #[test]
    fn parts_list_copy_feedback_follows_its_source_tab_for_success_and_failure() {
        for result in [Ok(()), Err("Clipboard permission denied".into())] {
            let mut app = copying_parts_list();
            let (source, epoch) = (app.tab.id, app.tab.file_epoch);
            let document = app.tab.doc.clone();
            app.add_tab();
            let front = app.tab.id;
            app.status = "Other tab is ready".into();
            let completion = Message::Equipment(Action::PartsListCopied {
                tab: source,
                epoch,
                result: result.clone(),
            });
            assert!(crate::app::tabs::document_result(&completion));
            let _ = app.update(Message::Tab(source, Box::new(completion)));
            assert!(!app.clipboard_working());
            assert_eq!(app.tab.id, front);
            assert_eq!(app.status, "Other tab is ready");
            assert!(!app.error);
            let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(source)));
            assert_eq!(app.tab.doc, document);
            assert_eq!(app.error, result.is_err());
            assert_eq!(
                app.status,
                result.map_or_else(|error| error, |()| "Copied the equipment parts list".into())
            );
        }
    }

    #[test]
    fn parts_list_copy_feedback_cannot_update_a_reopened_source_document() {
        for result in [Ok(()), Err("Clipboard permission denied".into())] {
            let mut app = copying_parts_list();
            let (source, epoch) = (app.tab.id, app.tab.file_epoch);
            app.show_opened(Document::default(), None);
            assert_eq!(app.tab.id, source);
            assert_ne!(app.tab.file_epoch, epoch);
            assert!(app.clipboard_working());
            let replacement = app.tab.doc.clone();
            app.status = "Replacement document is ready".into();
            app.error = false;
            app.add_tab();
            let front = app.tab.id;
            app.status = "Other tab is ready".into();
            let _ = app.update(Message::Tab(
                source,
                Box::new(Message::Equipment(Action::PartsListCopied {
                    tab: source,
                    epoch,
                    result,
                })),
            ));
            assert!(!app.clipboard_working());
            assert_eq!(app.tab.id, front);
            assert_eq!(app.status, "Other tab is ready");
            assert!(!app.error);
            let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(source)));
            assert_eq!(app.status, "Replacement document is ready");
            assert!(!app.error);
            assert_eq!(app.tab.doc, replacement);
        }
    }

    #[test]
    fn parts_list_copy_waits_for_pending_cut_copy_and_other_clipboard_work() {
        for cut in [false, true] {
            let mut app = equipment_drawing();
            app.tab.selected = vec![1];
            let before = app.tab.doc.clone();
            assert!(app.copy_native(cut, false, None).units() > 0);
            assert!(app.native_copy_busy && app.tab.clipboard_busy);
            assert_eq!(app.copy_equipment_parts_list().units(), 0);
            assert_eq!(app.status, "A clipboard operation is already in progress");
            assert_eq!(app.tab.doc, before);
            assert!(app.native_copy_busy && app.tab.clipboard_busy);
        }
        for pending in ["copy-as", "paste", "background-paste"] {
            let mut app = equipment_drawing();
            if pending == "copy-as" {
                assert!(app.copy_name("Pending name".into()).units() > 0);
            } else {
                assert!(app.paste_native(false).units() > 0);
                if pending == "background-paste" {
                    app.add_tab();
                }
            }
            assert!(app.clipboard_working());
            assert_eq!(app.copy_equipment_parts_list().units(), 0);
            assert_eq!(app.status, "A clipboard operation is already in progress");
            assert!(app.clipboard_working());
        }
    }

    #[test]
    fn parts_list_copy_holds_the_global_slot_and_restores_command_availability() {
        let mut app = equipment_drawing();
        let commands = crate::app::command_palette::catalog::build(&app);
        let copy = commands
            .iter()
            .find(|c| c.name == "Copy equipment parts list")
            .unwrap();
        let insert = commands
            .iter()
            .find(|c| c.name == "Insert equipment parts list")
            .unwrap();
        assert!(app.command_unavailable(copy).is_none());
        assert!(app.copy_equipment_parts_list().units() > 0);
        assert!(app.command_unavailable(copy).is_some());
        assert!(app.command_unavailable(insert).is_none());
        assert_eq!(app.copy_native(true, false, None).units(), 0);
        assert_eq!(app.copy_native(false, false, None).units(), 0);
        assert_eq!(app.copy_name("Another copy".into()).units(), 0);
        assert_eq!(app.paste_native(false).units(), 0);
        assert_eq!(app.copy_equipment_parts_list().units(), 0);
        assert!(app.clipboard_working());
        let (tab, epoch) = (app.tab.id, app.tab.file_epoch);
        let _ = app.update(Message::Equipment(Action::PartsListCopied {
            tab,
            epoch,
            result: Ok(()),
        }));
        assert!(!app.clipboard_working());
        assert!(app.command_unavailable(copy).is_none());
        assert!(app.copy_equipment_parts_list().units() > 0);
        assert!(app.clipboard_working());

        let (mut empty, _) = App::new();
        assert_eq!(empty.copy_equipment_parts_list().units(), 0);
        assert!(!empty.clipboard_working());
    }

    #[test]
    fn closing_the_parts_copy_source_keeps_the_global_slot_until_its_receipt() {
        for result in [Ok(()), Err("Clipboard permission denied".into())] {
            let mut app = copying_parts_list();
            let (source, epoch) = (app.tab.id, app.tab.file_epoch);
            let _ = app.close_active_tab();
            assert!(app.tab_index(source).is_none());
            assert!(app.clipboard_working());
            assert_eq!(
                app.copy_name("Wait for the previous write".into()).units(),
                0
            );
            app.status = "Other tab is ready".into();
            app.error = false;
            let front = app.tab.id;
            let _ = app.update(Message::Tab(
                source,
                Box::new(Message::Equipment(Action::PartsListCopied {
                    tab: source,
                    epoch,
                    result,
                })),
            ));
            assert!(!app.clipboard_working());
            assert_eq!(app.tab.id, front);
            assert_eq!(app.status, "Other tab is ready");
            assert!(!app.error);
            assert!(app.paste_native(false).units() > 0);
        }
    }

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
