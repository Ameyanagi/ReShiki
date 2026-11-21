//! The picker's Tree view: the whole repository as folders that open in
//! place, one row each, and a preview of the selected file beside them.
use super::*;
use crate::app::icons::{Glyph, Icon};
use iced::widget::column;

/// A row's height, to tell which rows are on screen.
const ROW: f32 = 28.;
const TREE: &str = "github-tree";
const TRUNCATED: &str = "Only part of this repository is listed";

pub(in crate::app::github) struct Row<'a> {
    pub(in crate::app::github) entry: &'a Entry,
    pub(in crate::app::github) depth: usize,
    pub(in crate::app::github) open: bool,
    /// The folders and files listed in a folder.
    items: usize,
}

/// The rows on screen, by index.
pub(super) fn on_screen(view: Option<(f32, f32)>, len: usize) -> std::ops::Range<usize> {
    let (top, height) = view.unwrap_or((0., 420.));
    let first = (top / ROW) as usize;
    let last = ((top + height) / ROW).ceil() as usize + 1;
    first.min(len)..last.min(len)
}

impl Picker {
    /// Folders first, then files, each by name, the open folders' below
    /// them.
    pub(in crate::app::github) fn rows(&self) -> Vec<Row<'_>> {
        let Some((entries, _)) = &self.whole else {
            return Vec::new();
        };
        let mut children: HashMap<&str, Vec<&Entry>> = HashMap::new();
        for entry in entries.iter().filter(|e| e.folder || gh::listed(&e.name)) {
            children.entry(parent(&entry.name)).or_default().push(entry);
        }
        for listed in children.values_mut() {
            listed.sort_by_cached_key(|entry| (!entry.folder, last(&entry.name).to_lowercase()));
        }
        let mut rows = Vec::new();
        let mut stack: Vec<(&Entry, usize)> = (children.get("").into_iter().flatten().rev())
            .map(|entry| (*entry, 0))
            .collect();
        while let Some((entry, depth)) = stack.pop() {
            let below = children.get(entry.name.as_str()).filter(|_| entry.folder);
            let open = entry.folder && self.opened.contains(&entry.name);
            rows.push(Row {
                entry,
                depth,
                open,
                items: below.map_or(0, Vec::len),
            });
            if open {
                stack.extend(below.into_iter().flatten().rev().map(|e| (*e, depth + 1)));
            }
        }
        rows
    }

    /// Scrolls the tree so the selected row shows.
    pub(super) fn reveal_row(&mut self) -> Task<Message> {
        let rows = self.rows();
        let Some(at) = (rows.iter()).position(|row| Some(&row.entry.name) == self.row.as_ref())
        else {
            return Task::none();
        };
        self.reveal_tree_index(at)
    }

    pub(super) fn reveal_new_item(&mut self) -> Task<Message> {
        let Some(draft) = &self.new_item else {
            return Task::none();
        };
        let mut path = draft.parent.as_str();
        while !path.is_empty() {
            self.opened.insert(path.to_owned());
            path = parent(path);
        }
        let at = self.draft_row(&self.rows()).map(|(at, _, _)| at);
        at.map_or_else(Task::none, |at| self.reveal_tree_index(at))
    }

    fn draft_row<'a>(&'a self, rows: &[Row<'_>]) -> Option<(usize, usize, &'a NewItem)> {
        let draft = self.new_item.as_ref().filter(|_| self.can_create())?;
        if draft.parent.is_empty() {
            return Some((0, 0, draft));
        }
        let (at, parent) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.entry.name == draft.parent)?;
        Some((at + 1, parent.depth + 1, draft))
    }

    fn reveal_tree_index(&mut self, at: usize) -> Task<Message> {
        let (top, height) = self.tree_view.unwrap_or((0., 420.));
        let y = at as f32 * ROW;
        let to = if y < top {
            y
        } else if y + ROW > top + height {
            y + ROW - height
        } else {
            return Task::none();
        };
        self.tree_view = Some((to, height));
        iced::widget::operation::scroll_to(
            TREE,
            iced::widget::operation::AbsoluteOffset { x: 0., y: to },
        )
    }
}

impl App {
    /// The rows on screen and the selected file's preview pane.
    pub(super) fn picker_tree<'a>(&'a self, picker: &'a Picker) -> Element<'a, Message> {
        let Some((_, truncated)) = &picker.whole else {
            return container(text("Listing…").size(12).style(muted_text))
                .center(Length::Fill)
                .into();
        };
        let signed_in = self.github.client().is_some();
        let rows = picker.rows();
        let draft = picker.draft_row(&rows);
        let len = rows.len() + usize::from(draft.is_some());
        let range = on_screen(picker.tree_view, len);
        // The rows off screen are only room to scroll.
        let mut list = column![Space::new().height(range.start as f32 * ROW)];
        for at in range.clone() {
            if let Some((draft_at, depth, draft)) = draft
                && at == draft_at
            {
                list = list.push(draft_tree_row(draft, depth));
            } else {
                let entry_at =
                    at - usize::from(draft.is_some_and(|(draft_at, _, _)| at > draft_at));
                if let Some(row) = rows.get(entry_at) {
                    list = list.push(tree_row(picker, entry_at, row, signed_in));
                }
            }
        }
        list = list.push(Space::new().height((len - range.end) as f32 * ROW));
        if len == 0 {
            list = list.push(container(text("Empty").size(13).style(muted_text)).padding(10));
        }
        if *truncated {
            list = list.push(note(TRUNCATED));
        }
        if picker.signed_out {
            list = list.push(note(SIGNED_OUT_HINT));
        }
        let list = scrollable(list)
            .id(TREE)
            .height(Length::Fill)
            .on_scroll(|view| {
                let top = view.absolute_offset().y;
                msg(Action::TreeScrolled(top, view.bounds().height))
            });
        row![
            container(list)
                .padding(4)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(thumb_frame),
            tree_pane(picker, signed_in),
        ]
        .spacing(12)
        .height(Length::Fill)
        .into()
    }
}

fn draft_tree_row(draft: &NewItem, depth: usize) -> Element<'_, Message> {
    let icon = if draft.folder {
        Icon::Folder
    } else {
        Icon::File
    };
    container(
        row![
            Space::new().width(depth as f32 * 18. + 12.),
            iced::widget::canvas(Glyph(icon, true)).width(16).height(16),
            new_item_field(draft).padding([2, 6]),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .height(ROW)
    .padding([0, 8])
    .align_y(Alignment::Center)
    .into()
}

fn note<'a>(line: &'static str) -> Element<'a, Message> {
    container(text(line).size(11).style(muted_text))
        .padding([6, 8])
        .into()
}

/// Chevron, icon, name, the format of a file other than a drawing, and who
/// changed it when, muted at the end. Signed out, names and formats only.
fn tree_row<'a>(
    picker: &'a Picker,
    at: usize,
    row: &Row<'a>,
    signed_in: bool,
) -> Element<'a, Message> {
    let entry = row.entry;
    let name = last(&entry.name);
    let chevron = match (entry.folder, row.open) {
        (false, _) => "",
        (true, true) => "▾",
        (true, false) => "▸",
    };
    let icon = if entry.folder {
        Icon::Folder
    } else {
        Icon::File
    };
    let mut content = row![
        Space::new().width(row.depth as f32 * 18.),
        text(chevron).size(11).width(12).style(muted_text),
        iced::widget::canvas(Glyph(icon, true)).width(16).height(16),
        text(name.to_owned())
            .size(13)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut spoken = match entry.folder {
        true => format!("{name}, folder"),
        false => name.to_owned(),
    };
    if let Some(label) = gh::format_label(name) {
        content = content.push(format_label(label));
    }
    content = content.push(Space::new().width(Length::Fill));
    let meta = match entry.folder {
        _ if !signed_in => None,
        true => Some(match row.items {
            0 => "Empty".to_owned(),
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        }),
        false => (picker.changed.get(&entry.name))
            .map(|(who, at)| format!("{who} · {}", super::super::versions::tile_time(*at))),
    };
    if let Some(meta) = meta {
        spoken = format!("{spoken}, {meta}");
        content = content.push(
            text(meta)
                .size(12)
                .style(muted_text)
                .wrapping(text::Wrapping::None),
        );
    }
    // A long name or date is cut, never wrapped: rows stay one line.
    let content = content.clip(true);
    let selected = picker.row.as_ref() == Some(&entry.name);
    let mut button = reshiki::accessibility::button(format!("github-row-{at}"), spoken, content)
        .width(Length::Fill)
        .height(ROW)
        .padding([0, 8])
        .style(crate::app::workspace::control(selected))
        .on_press(msg(Action::Row(entry.name.clone())));
    if entry.folder {
        return button.expanded(row.open).into();
    }
    button = button.checked(selected);
    file_menu(picker, entry.name.clone(), button.into())
}

/// The selected file: its preview, name, path, who changed it when and its
/// format, and whether it opens read only.
fn tree_pane<'a>(picker: &'a Picker, signed_in: bool) -> Element<'a, Message> {
    let content: Element<'a, Message> = match picker.selected_file() {
        None => container(
            text("Select a drawing to preview it.")
                .size(12)
                .style(muted_text),
        )
        .center_x(Length::Fill)
        .padding([24, 0])
        .into(),
        Some((path, sha)) => {
            let name = last(&path).to_owned();
            let changed = (picker.changed.get(&path))
                .filter(|_| signed_in)
                .map(|(who, at)| format!("{who} · {}", super::super::versions::tile_time(*at)));
            let facts: Vec<String> = (changed.into_iter())
                .chain(gh::format_label(&name).map(str::to_owned))
                .collect();
            let read_only =
                !name.ends_with(".rsk") || picker.repo.as_ref().is_some_and(|r| !r.push);
            let mut pane = column![].spacing(4);
            if signed_in {
                pane = pane.push(
                    container(drawing(picker, &sha))
                        .width(Length::Fill)
                        .height(136)
                        .padding(6)
                        .style(thumb_frame),
                );
            }
            pane = pane
                .push(
                    text(name)
                        .size(13)
                        .font(iced::Font {
                            weight: iced::font::Weight::Semibold,
                            ..iced::Font::with_name(reshiki::style::ui_font_family())
                        })
                        .wrapping(text::Wrapping::WordOrGlyph),
                )
                .push(
                    text(path)
                        .size(11)
                        .style(muted_text)
                        .wrapping(text::Wrapping::WordOrGlyph),
                );
            if !facts.is_empty() {
                pane = pane.push(text(facts.join(" · ")).size(11).style(muted_text));
            }
            if read_only {
                pane = pane.push(
                    row![
                        iced::widget::canvas(Glyph(Icon::Eye, true))
                            .width(14)
                            .height(14),
                        text("Read only: Open makes a local copy")
                            .size(11)
                            .style(muted_text),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
            }
            pane.into()
        }
    };
    container(content)
        .width(200)
        .height(Length::Fill)
        .padding(10)
        .style(thumb_frame)
        .into()
}
